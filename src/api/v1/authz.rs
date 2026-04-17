use std::collections::BTreeMap;

use actix_web::HttpRequest;

use crate::authz::{
    AttrValue, AuthorizationRequest, AuthorizationRequestBuilder, extract_principal,
};
use crate::{
    AppState,
    domain::{
        host::HostAuthContext,
        types::{Hostname, UpdateField},
    },
    errors::AppError,
};

/// Builder for collecting per-field authorization requests in PATCH handlers.
///
/// Each `field_*` method checks whether the corresponding update field is `Some`
/// and, if so, appends an [`AuthorizationRequest`] with the correct action and
/// attribute.  After all fields have been registered, call [`build`] to produce
/// the final `Vec<AuthorizationRequest>`.
pub(crate) struct UpdateAuthzBuilder<'a> {
    req: &'a HttpRequest,
    resource_kind: &'a str,
    resource_id: &'a str,
    base_attrs: BTreeMap<String, AttrValue>,
    requests: Vec<AuthorizationRequest>,
}

impl<'a> UpdateAuthzBuilder<'a> {
    /// Create a new builder for a specific resource.
    pub fn new(req: &'a HttpRequest, resource_kind: &'a str, resource_id: &'a str) -> Self {
        Self {
            req,
            resource_kind,
            resource_id,
            base_attrs: BTreeMap::new(),
            requests: Vec::new(),
        }
    }

    /// Attach base attributes that will be merged into every request produced
    /// by this builder (used by host update which needs host context on each
    /// sub-request).
    pub fn with_base_attrs(mut self, attrs: BTreeMap<String, AttrValue>) -> Self {
        self.base_attrs = attrs;
        self
    }

    /// If `value` is `Some`, emit an authz request with a `String` attribute
    /// named `attr_name`.
    pub fn field_string(
        &mut self,
        value: &Option<String>,
        action: &str,
        attr_name: &str,
    ) -> &mut Self {
        if let Some(val) = value {
            self.requests.push(
                self.base_request(action)
                    .attr(attr_name, AttrValue::String(val.clone()))
                    .build(),
            );
        }
        self
    }

    /// If `value` is `Some`, emit an authz request with a `Bool` attribute.
    pub fn field_bool(&mut self, value: Option<bool>, action: &str, attr_name: &str) -> &mut Self {
        if let Some(val) = value {
            self.requests.push(
                self.base_request(action)
                    .attr(attr_name, AttrValue::Bool(val))
                    .build(),
            );
        }
        self
    }

    /// If `value` is `Some`, emit an authz request with a `Long` attribute
    /// (from `u32`).
    pub fn field_u32(&mut self, value: Option<u32>, action: &str, attr_name: &str) -> &mut Self {
        if let Some(val) = value {
            self.requests.push(
                self.base_request(action)
                    .attr(attr_name, AttrValue::Long(i64::from(val)))
                    .build(),
            );
        }
        self
    }

    /// If `value` is `Some`, emit an authz request with a string-set attribute.
    pub fn field_string_set(
        &mut self,
        value: &Option<Vec<String>>,
        action: &str,
        attr_name: &str,
    ) -> &mut Self {
        if let Some(vals) = value {
            self.requests.push(
                self.base_request(action)
                    .attr(attr_name, string_set(vals.clone()))
                    .build(),
            );
        }
        self
    }

    /// If `value` is `Some`, emit an authz request with no field-specific
    /// attribute (presence-only check).
    pub fn field_present<T>(&mut self, value: &Option<T>, action: &str) -> &mut Self {
        if value.is_some() {
            self.requests.push(self.base_request(action).build());
        }
        self
    }

    /// Handle an `UpdateField<T>` clearable field.
    ///
    /// - `Set(v)` → emit request with `set_attr_name` = the value
    /// - `Clear` → emit request with `clear_attr_name` = `true`
    /// - `Unchanged` → no request
    ///
    /// The caller provides a closure to convert the inner value to an
    /// `AttrValue`.
    pub fn field_clearable<T>(
        &mut self,
        value: &UpdateField<T>,
        action: &str,
        set_attr_name: &str,
        clear_attr_name: &str,
        to_attr: impl FnOnce(&T) -> AttrValue,
    ) -> &mut Self {
        match value {
            UpdateField::Set(val) => {
                let authz = self.base_request(action).attr(set_attr_name, to_attr(val));
                self.requests.push(authz.build());
            }
            UpdateField::Clear => {
                let authz = self
                    .base_request(action)
                    .attr(clear_attr_name, AttrValue::Bool(true));
                self.requests.push(authz.build());
            }
            UpdateField::Unchanged => {}
        }
        self
    }

    /// Combine several optional `u32` timing fields into a single authz
    /// request.  Only emits a request if at least one field is `Some`.
    pub fn timing_fields(&mut self, action: &str, fields: &[(&str, Option<u32>)]) -> &mut Self {
        if fields.iter().any(|(_, v)| v.is_some()) {
            let mut authz = self.base_request(action);
            for &(name, value) in fields {
                if let Some(v) = value {
                    authz = authz.attr(name, AttrValue::Long(i64::from(v)));
                }
            }
            self.requests.push(authz.build());
        }
        self
    }

    /// Consume the builder and return the collected authorization requests.
    pub fn build(self) -> Vec<AuthorizationRequest> {
        self.requests
    }

    /// Internal: create a base `AuthorizationRequestBuilder` with the
    /// resource kind, id, and any base attrs already applied.
    fn base_request(&self, action: &str) -> AuthorizationRequestBuilder {
        let builder = request(self.req, action, self.resource_kind, self.resource_id);
        if self.base_attrs.is_empty() {
            builder
        } else {
            builder.attrs(self.base_attrs.clone())
        }
    }
}

pub(crate) fn request(
    req: &HttpRequest,
    action: &str,
    resource_kind: &str,
    resource_id: impl Into<String>,
) -> AuthorizationRequestBuilder {
    AuthorizationRequest::builder(
        extract_principal(req),
        action,
        resource_kind,
        resource_id.into(),
    )
}

pub(crate) fn string_set(values: impl IntoIterator<Item = String>) -> AttrValue {
    AttrValue::Set(values.into_iter().map(AttrValue::String).collect())
}

pub(crate) fn host_attrs(authz_context: &HostAuthContext) -> BTreeMap<String, AttrValue> {
    let host = authz_context.host();
    let mut attrs = BTreeMap::from([(
        "name".to_string(),
        AttrValue::String(host.name().as_str().to_string()),
    )]);
    if let Some(zone) = host.zone() {
        attrs.insert(
            "zone".to_string(),
            AttrValue::String(zone.as_str().to_string()),
        );
    }
    if !authz_context.addresses().is_empty() {
        attrs.insert(
            "addresses".to_string(),
            string_set(
                authz_context
                    .addresses()
                    .iter()
                    .map(|address| address.as_str().to_string()),
            ),
        );
    }
    attrs.insert(
        "networks".to_string(),
        string_set(
            authz_context
                .networks()
                .iter()
                .map(|network| network.as_str().to_string()),
        ),
    );
    attrs
}

pub(crate) async fn host_request(
    state: &AppState,
    req: &HttpRequest,
    action: &str,
    host_name: &Hostname,
) -> Result<AuthorizationRequestBuilder, AppError> {
    let authz_attrs = host_attrs_for_host(state, host_name).await?;
    Ok(request(
        req,
        action,
        crate::authz::actions::resource_kinds::HOST,
        host_name.as_str(),
    )
    .attrs(authz_attrs))
}

pub(crate) async fn host_attrs_for_host(
    state: &AppState,
    host_name: &Hostname,
) -> Result<BTreeMap<String, AttrValue>, AppError> {
    let context = state.services.hosts().get_auth_context(host_name).await?;
    Ok(host_attrs(&context))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::host_attrs;
    use crate::{
        authz::AttrValue,
        domain::{
            host::{Host, HostAuthContext},
            types::Hostname,
        },
    };

    #[test]
    fn host_attrs_always_include_networks_set() {
        let host = Host::restore(
            Uuid::new_v4(),
            Hostname::new("test.example.org").expect("valid hostname"),
            None,
            None,
            "host comment",
            Utc::now(),
            Utc::now(),
        )
        .expect("host should build");
        let attrs = host_attrs(&HostAuthContext::new(host, Vec::new(), Vec::new()));

        assert_eq!(attrs.get("networks"), Some(&AttrValue::Set(Vec::new())));
        assert!(!attrs.contains_key("network"));
    }
}
