use async_trait::async_trait;
use ldap3::{LdapConnAsync, Scope, SearchEntry};

use crate::errors::AppError;

use super::{AuthenticatedIdentity, BackendLoginRequest, ScopeAuthenticator};

#[derive(Clone)]
pub struct LdapScopeAuthenticator {
    url: String,
    timeout: std::time::Duration,
    user_search_base: String,
    user_search_filter: String,
    group_search_base: String,
    group_search_filter: String,
    bind_dn: Option<String>,
    bind_password: Option<String>,
}

impl LdapScopeAuthenticator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        url: String,
        timeout_ms: u64,
        user_search_base: String,
        user_search_filter: String,
        group_search_base: String,
        group_search_filter: String,
        bind_dn: Option<String>,
        bind_password: Option<String>,
    ) -> Self {
        Self {
            url,
            timeout: std::time::Duration::from_millis(timeout_ms),
            user_search_base,
            user_search_filter,
            group_search_base,
            group_search_filter,
            bind_dn,
            bind_password,
        }
    }

    async fn connect(&self) -> Result<ldap3::Ldap, AppError> {
        let (conn, mut ldap) = tokio::time::timeout(self.timeout, LdapConnAsync::new(&self.url))
            .await
            .map_err(|_| AppError::unavailable("LDAP connection timed out"))?
            .map_err(AppError::internal)?;
        ldap3::drive!(conn);
        if let Some(bind_dn) = &self.bind_dn {
            let password = self.bind_password.as_deref().unwrap_or_default();
            ldap.simple_bind(bind_dn, password)
                .await
                .map_err(AppError::internal)?
                .success()
                .map_err(|error| {
                    AppError::unavailable(format!("LDAP service bind failed: {error}"))
                })?;
        }
        Ok(ldap)
    }
}

#[async_trait]
impl ScopeAuthenticator for LdapScopeAuthenticator {
    async fn login(
        &self,
        credentials: BackendLoginRequest,
    ) -> Result<AuthenticatedIdentity, AppError> {
        let mut ldap = self.connect().await?;
        let user_filter = self
            .user_search_filter
            .replace("{username}", &ldap3::ldap_escape(&credentials.username));
        let (entries, _) = ldap
            .search(
                &self.user_search_base,
                Scope::Subtree,
                &user_filter,
                vec!["dn"],
            )
            .await
            .map_err(AppError::internal)?
            .success()
            .map_err(|error| AppError::unavailable(format!("LDAP user search failed: {error}")))?;
        let entry = entries
            .into_iter()
            .next()
            .map(SearchEntry::construct)
            .ok_or_else(|| AppError::unauthorized("invalid credentials"))?;
        let user_dn = entry.dn;

        let (user_conn, mut user_ldap) =
            tokio::time::timeout(self.timeout, LdapConnAsync::new(&self.url))
                .await
                .map_err(|_| AppError::unavailable("LDAP bind timed out"))?
                .map_err(AppError::internal)?;
        ldap3::drive!(user_conn);
        user_ldap
            .simple_bind(&user_dn, &credentials.password)
            .await
            .map_err(AppError::internal)?
            .success()
            .map_err(|_| AppError::unauthorized("invalid credentials"))?;

        let group_filter = self
            .group_search_filter
            .replace("{username}", &ldap3::ldap_escape(&credentials.username))
            .replace("{user_dn}", &ldap3::ldap_escape(&user_dn));
        let (groups, _) = ldap
            .search(
                &self.group_search_base,
                Scope::Subtree,
                &group_filter,
                vec!["cn"],
            )
            .await
            .map_err(AppError::internal)?
            .success()
            .map_err(|error| AppError::unavailable(format!("LDAP group search failed: {error}")))?;

        Ok(AuthenticatedIdentity {
            username: credentials.username,
            groups: groups
                .into_iter()
                .map(SearchEntry::construct)
                .map(|entry| {
                    entry
                        .attrs
                        .get("cn")
                        .and_then(|values| values.first().cloned())
                        .unwrap_or(entry.dn)
                })
                .collect(),
            max_expires_at: None,
        })
    }
}
