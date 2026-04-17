use std::{collections::BTreeMap, fmt, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};

use actix_web::{HttpMessage, HttpRequest};
use treetop_client::{
    Action as TreetopAction, AttrValue as TreetopAttrValue,
    AuthorizeRequest as TreetopAuthorizeRequest, BatchResult as TreetopBatchResult,
    Client as TreetopClient, DecisionBrief as TreetopDecisionBrief, Group as TreetopGroup,
    Request as TreetopRequest, Resource as TreetopResource, TreetopError, User as TreetopUser,
};

use crate::{authn, config::Config, errors::AppError};

pub mod actions;

pub use treetop_client::AttrValue;

/// Client for authorization decisions, delegating to a pluggable [`Authorizer`] implementation.
#[derive(Clone)]
pub struct AuthorizerClient {
    inner: Arc<dyn Authorizer>,
}

impl AuthorizerClient {
    /// Build an authorizer client from a concrete authorizer implementation.
    pub fn new(inner: Arc<dyn Authorizer>) -> Self {
        Self { inner }
    }

    /// Build an authorizer client from application configuration.
    ///
    /// Selects Treetop (remote), AllowAll (dev bypass), or DenyAll based on config.
    pub fn from_config(config: &Config) -> Self {
        let inner: Arc<dyn Authorizer> = match &config.treetop_url {
            Some(base_url) => Arc::new(TreetopAuthorizer::new(
                base_url.clone(),
                Duration::from_millis(config.treetop_timeout_ms),
            )),
            None if config.allow_dev_authz_bypass => Arc::new(AllowAllAuthorizer),
            None => Arc::new(DenyAllAuthorizer),
        };

        Self::new(inner)
    }

    /// Issue an authorization decision for the given request.
    pub async fn authorize(
        &self,
        request: AuthorizationRequest,
    ) -> Result<AuthorizationDecision, AppError> {
        let mut decisions = self.authorize_many(vec![request]).await?;
        decisions
            .pop()
            .ok_or_else(|| AppError::authz("authorizer returned no decision"))
    }

    /// Issue authorization decisions for a batch of requests.
    pub async fn authorize_many(
        &self,
        requests: Vec<AuthorizationRequest>,
    ) -> Result<Vec<AuthorizationDecision>, AppError> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }

        self.inner.authorize_many(requests).await
    }
}

/// Pluggable authorization backend trait.
#[async_trait::async_trait]
pub trait Authorizer: Send + Sync {
    async fn authorize_many(
        &self,
        requests: Vec<AuthorizationRequest>,
    ) -> Result<Vec<AuthorizationDecision>, AppError>;
}

/// Authenticated user identity with group memberships.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Principal {
    pub id: String,
    #[serde(default)]
    pub namespace: Vec<String>,
    #[serde(default)]
    pub groups: Vec<Group>,
}

/// Named group that a principal may belong to.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    #[serde(default)]
    pub namespace: Vec<String>,
}

/// Action being authorized (for example `host.create` or `host.update.zone`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    #[serde(default)]
    pub namespace: Vec<String>,
}

impl Action {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            namespace: Vec::new(),
        }
    }
}

/// Resource being accessed, identified by kind and ID with optional attributes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Resource {
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub attrs: BTreeMap<String, TreetopAttrValue>,
}

impl Resource {
    pub fn new(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
            attrs: BTreeMap::new(),
        }
    }

    pub fn with_attrs(mut self, attrs: BTreeMap<String, TreetopAttrValue>) -> Self {
        self.attrs = attrs;
        self
    }

    pub fn with_attr(mut self, key: impl Into<String>, value: TreetopAttrValue) -> Self {
        self.attrs.insert(key.into(), value);
        self
    }
}

/// Complete authorization request combining principal, action, and resource.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthorizationRequest {
    pub principal: Principal,
    pub action: Action,
    pub resource: Resource,
}

impl AuthorizationRequest {
    pub fn new(principal: Principal, action: Action, resource: Resource) -> Self {
        Self {
            principal,
            action,
            resource,
        }
    }

    pub fn builder(
        principal: Principal,
        action: impl Into<String>,
        resource_kind: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> AuthorizationRequestBuilder {
        AuthorizationRequestBuilder {
            principal,
            action: Action::new(action),
            resource: Resource::new(resource_kind, resource_id),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AuthorizationRequestBuilder {
    principal: Principal,
    action: Action,
    resource: Resource,
}

impl AuthorizationRequestBuilder {
    pub fn namespace<I, S>(mut self, namespace: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.action.namespace = namespace.into_iter().map(Into::into).collect();
        self
    }

    pub fn attr(mut self, key: impl Into<String>, value: TreetopAttrValue) -> Self {
        self.resource.attrs.insert(key.into(), value);
        self
    }

    pub fn attrs(mut self, attrs: BTreeMap<String, TreetopAttrValue>) -> Self {
        self.resource.attrs.extend(attrs);
        self
    }

    pub fn build(self) -> AuthorizationRequest {
        AuthorizationRequest::new(self.principal, self.action, self.resource)
    }
}

/// Result of an authorization check: allow or deny.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AuthorizationDecision {
    Allow,
    Deny,
}

impl fmt::Display for AuthorizationDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Allow => f.write_str("allow"),
            Self::Deny => f.write_str("deny"),
        }
    }
}

/// Extract a principal from the HTTP request.
///
/// Looks for `X-Mreg-User` header (required) and optional `X-Mreg-Groups`
/// (comma-separated). When the dev auth bypass is active this header can be
/// omitted and a default "anonymous" principal is returned.
pub fn extract_principal(req: &HttpRequest) -> Principal {
    if let Some(context) = req.extensions().get::<authn::PrincipalContext>() {
        return context.principal.clone();
    }
    authn::header_principal(req)
}

/// Check whether the principal is authorized for the given request.
///
/// Returns `Ok(())` on Allow, `Err(Forbidden)` on Deny.
pub async fn require_permission(
    authz: &AuthorizerClient,
    request: AuthorizationRequest,
) -> Result<(), AppError> {
    require_permissions(authz, vec![request]).await
}

/// Check whether all authorization requests are allowed.
///
/// Intended for PATCH handlers and other multi-action cases that need to batch
/// related checks into a single treetop round trip.
pub async fn require_permissions(
    authz: &AuthorizerClient,
    requests: Vec<AuthorizationRequest>,
) -> Result<(), AppError> {
    if requests.is_empty() {
        return Ok(());
    }

    let decisions = authz.authorize_many(requests.clone()).await?;
    if decisions.len() != requests.len() {
        return Err(AppError::authz(format!(
            "authorizer returned {} decisions for {} requests",
            decisions.len(),
            requests.len()
        )));
    }

    for (request, decision) in requests.into_iter().zip(decisions) {
        let principal_id = request.principal.id;
        let action = request.action.id;
        let resource_kind = request.resource.kind;
        let resource_id = request.resource.id;

        if matches!(decision, AuthorizationDecision::Deny) {
            tracing::warn!(
                principal = %principal_id,
                %action,
                %resource_kind,
                %resource_id,
                decision = %decision,
                "authorization denied"
            );
            Err(AppError::forbidden(format!(
                "permission denied for action '{}' on {} '{}'",
                action, resource_kind, resource_id
            )))?;
        }

        tracing::debug!(
            principal = %principal_id,
            %action,
            %resource_kind,
            %resource_id,
            decision = %decision,
            "authorization allowed"
        );
    }

    Ok(())
}

struct AllowAllAuthorizer;
struct DenyAllAuthorizer;

#[async_trait::async_trait]
impl Authorizer for AllowAllAuthorizer {
    async fn authorize_many(
        &self,
        requests: Vec<AuthorizationRequest>,
    ) -> Result<Vec<AuthorizationDecision>, AppError> {
        Ok(vec![AuthorizationDecision::Allow; requests.len()])
    }
}

#[async_trait::async_trait]
impl Authorizer for DenyAllAuthorizer {
    async fn authorize_many(
        &self,
        requests: Vec<AuthorizationRequest>,
    ) -> Result<Vec<AuthorizationDecision>, AppError> {
        Ok(vec![AuthorizationDecision::Deny; requests.len()])
    }
}

struct TreetopAuthorizer {
    client: TreetopClient,
}

impl TreetopAuthorizer {
    fn new(base_url: String, timeout: Duration) -> Self {
        let client = TreetopClient::builder(base_url)
            .request_timeout(timeout)
            .build()
            .expect("treetop client must build");
        Self { client }
    }
}

#[async_trait::async_trait]
impl Authorizer for TreetopAuthorizer {
    async fn authorize_many(
        &self,
        requests: Vec<AuthorizationRequest>,
    ) -> Result<Vec<AuthorizationDecision>, AppError> {
        let request_count = requests.len();
        let batch = TreetopAuthorizeRequest::from_requests(
            requests.into_iter().map(authorization_request_to_treetop),
        );
        let response = self
            .client
            .authorize(&batch)
            .await
            .map_err(map_treetop_error)?;

        if response.total() != request_count {
            return Err(AppError::authz(format!(
                "treetop returned {} results for {} requests",
                response.total(),
                request_count
            )));
        }

        response
            .into_results()
            .into_iter()
            .map(|result| match result.result {
                TreetopBatchResult::Success { data } => Ok(match data.decision {
                    TreetopDecisionBrief::Allow => AuthorizationDecision::Allow,
                    TreetopDecisionBrief::Deny => AuthorizationDecision::Deny,
                }),
                TreetopBatchResult::Failed { message } => Err(AppError::authz(message)),
            })
            .collect()
    }
}

fn authorization_request_to_treetop(request: AuthorizationRequest) -> TreetopRequest {
    TreetopRequest::new(
        principal_to_treetop_user(request.principal),
        action_to_treetop(request.action),
        resource_to_treetop(request.resource),
    )
}

fn principal_to_treetop_user(principal: Principal) -> TreetopUser {
    let groups = principal
        .groups
        .into_iter()
        .map(|group| TreetopGroup::new(group.id).with_namespace(group.namespace))
        .collect();

    TreetopUser::new(principal.id)
        .with_namespace(principal.namespace)
        .with_groups(groups)
}

fn action_to_treetop(action: Action) -> TreetopAction {
    TreetopAction::new(action.id).with_namespace(action.namespace)
}

fn resource_to_treetop(resource: Resource) -> TreetopResource {
    let mut treetop = TreetopResource::new(resource.kind, resource.id);
    for (key, value) in resource.attrs {
        treetop = treetop.with_attr(key, value);
    }
    treetop
}

fn map_treetop_error(error: TreetopError) -> AppError {
    match error {
        TreetopError::Transport(error) => AppError::authz(error.to_string()),
        TreetopError::Api { status, message } => {
            AppError::authz(format!("treetop returned {status}: {message}"))
        }
        TreetopError::Deserialization(error) => AppError::authz(error.to_string()),
        TreetopError::Configuration(error) => AppError::authz(error.to_string()),
        TreetopError::InvalidUrl(error) => AppError::authz(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn authorization_request_builder_sets_attrs() {
        let principal = Principal {
            id: "alice".to_string(),
            namespace: vec!["uio".to_string()],
            groups: Vec::new(),
        };

        let request = AuthorizationRequest::builder(
            principal.clone(),
            actions::host::UPDATE_ZONE,
            actions::resource_kinds::HOST,
            "web-01.example.org",
        )
        .namespace(["mreg", "v1"])
        .attr("name", AttrValue::String("web-01.example.org".to_string()))
        .attr("zone", AttrValue::String("example.org".to_string()))
        .attr("new_zone", AttrValue::String("lab.example.org".to_string()))
        .build();

        assert_eq!(request.principal.id, principal.id);
        assert_eq!(request.action.id, actions::host::UPDATE_ZONE);
        assert_eq!(request.action.namespace, vec!["mreg", "v1"]);
        assert_eq!(request.resource.kind, actions::resource_kinds::HOST);
        assert_eq!(request.resource.id, "web-01.example.org");
        assert_eq!(
            request.resource.attrs.get("zone"),
            Some(&AttrValue::String("example.org".to_string()))
        );
        assert_eq!(
            request.resource.attrs.get("new_zone"),
            Some(&AttrValue::String("lab.example.org".to_string()))
        );
    }

    #[tokio::test]
    async fn require_permissions_batches_requests() {
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let authz = AuthorizerClient::new(Arc::new(RecordingAuthorizer {
            decisions: vec![AuthorizationDecision::Allow, AuthorizationDecision::Allow],
            recorded: recorded.clone(),
        }));

        let principal = Principal {
            id: "alice".to_string(),
            namespace: Vec::new(),
            groups: Vec::new(),
        };

        let requests = vec![
            AuthorizationRequest::builder(
                principal.clone(),
                actions::host::UPDATE_NAME,
                actions::resource_kinds::HOST,
                "web-01.example.org",
            )
            .attr(
                "new_name",
                AttrValue::String("web-02.example.org".to_string()),
            )
            .build(),
            AuthorizationRequest::builder(
                principal,
                actions::host::UPDATE_ZONE,
                actions::resource_kinds::HOST,
                "web-01.example.org",
            )
            .attr("new_zone", AttrValue::String("lab.example.org".to_string()))
            .build(),
        ];

        require_permissions(&authz, requests).await.unwrap();

        let captured = recorded.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].len(), 2);
        assert_eq!(captured[0][0].action.id, actions::host::UPDATE_NAME);
        assert_eq!(captured[0][1].action.id, actions::host::UPDATE_ZONE);
    }

    #[tokio::test]
    async fn require_permissions_returns_forbidden_on_deny() {
        let authz = AuthorizerClient::new(Arc::new(RecordingAuthorizer {
            decisions: vec![AuthorizationDecision::Allow, AuthorizationDecision::Deny],
            recorded: Arc::new(Mutex::new(Vec::new())),
        }));

        let principal = Principal {
            id: "alice".to_string(),
            namespace: Vec::new(),
            groups: Vec::new(),
        };

        let result = require_permissions(
            &authz,
            vec![
                AuthorizationRequest::builder(
                    principal.clone(),
                    actions::host::UPDATE_NAME,
                    actions::resource_kinds::HOST,
                    "web-01.example.org",
                )
                .build(),
                AuthorizationRequest::builder(
                    principal,
                    actions::host::DELETE,
                    actions::resource_kinds::HOST,
                    "web-01.example.org",
                )
                .build(),
            ],
        )
        .await;

        let err = result.expect_err("second authorization check should be denied");
        assert!(matches!(err, AppError::Forbidden(_)));
        assert_eq!(
            err.to_string(),
            "forbidden: permission denied for action 'host.delete' on host 'web-01.example.org'"
        );
    }

    struct RecordingAuthorizer {
        decisions: Vec<AuthorizationDecision>,
        recorded: Arc<Mutex<Vec<Vec<AuthorizationRequest>>>>,
    }

    #[async_trait::async_trait]
    impl Authorizer for RecordingAuthorizer {
        async fn authorize_many(
            &self,
            requests: Vec<AuthorizationRequest>,
        ) -> Result<Vec<AuthorizationDecision>, AppError> {
            self.recorded.lock().unwrap().push(requests);
            Ok(self.decisions.clone())
        }
    }
}
