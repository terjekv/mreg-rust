use std::{collections::BTreeSet, time::Duration};

use async_trait::async_trait;
use ldap3::{LdapConnAsync, LdapConnSettings, Scope, SearchEntry};
use regex::Regex;
use url::Url;

use crate::{
    config::{GroupMappingRuleConfig, LdapSearchScope},
    errors::AppError,
};

use super::{AuthProviderBackend, AuthenticatedIdentity, BackendLoginRequest};

#[derive(Clone)]
pub struct LdapAuthenticatorConfig {
    pub url: String,
    pub bind_dn: Option<String>,
    pub bind_password: Option<String>,
    pub connect_timeout_seconds: u64,
    pub operation_timeout_seconds: u64,
    pub user_base_dn: String,
    pub user_filter: String,
    pub user_scope: LdapSearchScope,
    pub username_attribute: String,
    pub subject_attribute: String,
    pub display_name_attribute: Option<String>,
    pub email_attribute: Option<String>,
    pub group_attributes: Vec<String>,
    pub group_filters: Vec<String>,
    pub group_rules: Vec<GroupMappingRuleConfig>,
}

#[derive(Clone)]
struct GroupMappingRule {
    pattern: Regex,
    name: String,
}

#[derive(Clone)]
pub struct LdapAuthProvider {
    config: LdapAuthenticatorConfig,
    group_filters: Vec<Regex>,
    group_rules: Vec<GroupMappingRule>,
    starttls: bool,
}

impl LdapAuthProvider {
    pub fn new(config: LdapAuthenticatorConfig) -> Result<Self, AppError> {
        let parsed_url = Url::parse(&config.url)
            .map_err(|error| AppError::config(format!("invalid LDAP URL: {error}")))?;
        let starttls = match parsed_url.scheme() {
            "ldap" => true,
            "ldaps" => false,
            scheme => {
                return Err(AppError::config(format!(
                    "LDAP URL must use ldap or ldaps, got `{scheme}`"
                )));
            }
        };
        let group_filters = config
            .group_filters
            .iter()
            .map(|filter| {
                Regex::new(filter).map_err(|error| {
                    AppError::config(format!("invalid LDAP group filter `{filter}`: {error}"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let group_rules = config
            .group_rules
            .iter()
            .map(|rule| {
                Ok(GroupMappingRule {
                    pattern: Regex::new(&rule.pattern).map_err(|error| {
                        AppError::config(format!(
                            "invalid LDAP group rule `{}`: {error}",
                            rule.pattern
                        ))
                    })?,
                    name: rule.name.clone(),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        Ok(Self {
            config,
            group_filters,
            group_rules,
            starttls,
        })
    }

    async fn connect(&self) -> Result<ldap3::Ldap, AppError> {
        let settings = LdapConnSettings::new()
            .set_conn_timeout(Duration::from_secs(self.config.connect_timeout_seconds))
            .set_starttls(self.starttls);
        let (connection, ldap) = LdapConnAsync::with_settings(settings, &self.config.url)
            .await
            .map_err(|error| AppError::unavailable(format!("LDAP connection failed: {error}")))?;
        ldap3::drive!(connection);
        Ok(ldap)
    }

    fn operation_timeout(&self) -> Duration {
        Duration::from_secs(self.config.operation_timeout_seconds)
    }

    async fn bind_service(&self, ldap: &mut ldap3::Ldap) -> Result<(), AppError> {
        match (&self.config.bind_dn, &self.config.bind_password) {
            (Some(dn), Some(password)) => ldap
                .with_timeout(self.operation_timeout())
                .simple_bind(dn, password)
                .await
                .map_err(|error| {
                    AppError::unavailable(format!("LDAP service bind failed: {error}"))
                })?
                .success()
                .map(|_| ())
                .map_err(|error| {
                    AppError::unavailable(format!("LDAP service bind failed: {error}"))
                }),
            (None, None) => Ok(()),
            _ => Err(AppError::config(
                "LDAP bind DN and password must be configured together",
            )),
        }
    }

    fn search_scope(&self) -> Scope {
        match self.config.user_scope {
            LdapSearchScope::Base => Scope::Base,
            LdapSearchScope::One => Scope::OneLevel,
            LdapSearchScope::Subtree => Scope::Subtree,
        }
    }

    fn search_attributes(&self) -> Vec<String> {
        let mut attributes = BTreeSet::new();
        attributes.insert(self.config.username_attribute.clone());
        if !self.config.subject_attribute.eq_ignore_ascii_case("dn") {
            attributes.insert(self.config.subject_attribute.clone());
        }
        if let Some(attribute) = &self.config.display_name_attribute {
            attributes.insert(attribute.clone());
        }
        if let Some(attribute) = &self.config.email_attribute {
            attributes.insert(attribute.clone());
        }
        attributes.extend(self.config.group_attributes.iter().cloned());
        attributes.into_iter().collect()
    }

    async fn load_user(&self, username: &str) -> Result<(String, SearchEntry), AppError> {
        let mut ldap = self.connect().await?;
        self.bind_service(&mut ldap).await?;
        let filter = self
            .config
            .user_filter
            .replace("{username}", &ldap3::ldap_escape(username));
        let (entries, _) = ldap
            .with_timeout(self.operation_timeout())
            .search(
                &self.config.user_base_dn,
                self.search_scope(),
                &filter,
                self.search_attributes(),
            )
            .await
            .map_err(|error| AppError::unavailable(format!("LDAP user search failed: {error}")))?
            .success()
            .map_err(|error| AppError::unavailable(format!("LDAP user search failed: {error}")))?;
        if entries.len() != 1 {
            return Err(AppError::unauthorized("invalid credentials"));
        }
        let entry = SearchEntry::construct(entries.into_iter().next().expect("one LDAP entry"));
        Ok((entry.dn.clone(), entry))
    }

    async fn bind_user(&self, dn: &str, password: &str) -> Result<(), AppError> {
        if password.is_empty() {
            return Err(AppError::unauthorized("invalid credentials"));
        }
        let mut ldap = self.connect().await?;
        ldap.with_timeout(self.operation_timeout())
            .simple_bind(dn, password)
            .await
            .map_err(|error| AppError::unavailable(format!("LDAP user bind failed: {error}")))?
            .success()
            .map(|_| ())
            .map_err(|_| AppError::unauthorized("invalid credentials"))
    }

    fn groups_from_entry(&self, entry: &SearchEntry) -> Vec<String> {
        let mut groups = BTreeSet::new();
        for attribute in &self.config.group_attributes {
            let Some(values) = attribute_values(entry, attribute) else {
                continue;
            };
            for value in values {
                for rule in &self.group_rules {
                    let Some(captures) = rule.pattern.captures(value) else {
                        continue;
                    };
                    let mut name = String::new();
                    captures.expand(&rule.name, &mut name);
                    if name.trim().is_empty() {
                        continue;
                    }
                    // Filters intentionally apply to the extracted group name, not the
                    // provider's raw attribute value.
                    if !self.group_filters.is_empty()
                        && !self
                            .group_filters
                            .iter()
                            .any(|filter| filter.is_match(&name))
                    {
                        continue;
                    }
                    groups.insert(name);
                }
            }
        }
        groups.into_iter().collect()
    }
}

#[async_trait]
impl AuthProviderBackend for LdapAuthProvider {
    async fn authenticate(
        &self,
        credentials: BackendLoginRequest,
    ) -> Result<AuthenticatedIdentity, AppError> {
        let (user_dn, entry) = self.load_user(&credentials.username).await?;
        self.bind_user(&user_dn, &credentials.password).await?;
        let username = first_attribute(&entry, &self.config.username_attribute)
            .unwrap_or(credentials.username);
        Ok(AuthenticatedIdentity {
            username,
            groups: self.groups_from_entry(&entry),
            max_expires_at: None,
        })
    }
}

fn attribute_values<'a>(entry: &'a SearchEntry, name: &str) -> Option<&'a Vec<String>> {
    entry
        .attrs
        .iter()
        .find(|(attribute, _)| attribute.eq_ignore_ascii_case(name))
        .map(|(_, values)| values)
}

fn first_attribute(entry: &SearchEntry, name: &str) -> Option<String> {
    attribute_values(entry, name).and_then(|values| values.first().cloned())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rstest::rstest;

    use super::*;

    fn config() -> LdapAuthenticatorConfig {
        LdapAuthenticatorConfig {
            url: "ldap://ldap.example.org".to_string(),
            bind_dn: None,
            bind_password: None,
            connect_timeout_seconds: 5,
            operation_timeout_seconds: 10,
            user_base_dn: "ou=people,dc=example,dc=org".to_string(),
            user_filter: "(uid={username})".to_string(),
            user_scope: LdapSearchScope::Subtree,
            username_attribute: "uid".to_string(),
            subject_attribute: "entryUUID".to_string(),
            display_name_attribute: Some("cn".to_string()),
            email_attribute: Some("mail".to_string()),
            group_attributes: vec!["memberOf".to_string()],
            group_filters: vec!["^(admin|mreg-.+)$".to_string()],
            group_rules: vec![GroupMappingRuleConfig {
                pattern: "^cn=([^,]+),ou=groups,dc=example,dc=org$".to_string(),
                name: "$1".to_string(),
            }],
        }
    }

    #[rstest]
    #[case("ldap://ldap.example.org", true)]
    #[case("ldaps://ldap.example.org", false)]
    fn ldap_transport_cases(#[case] url: &str, #[case] starttls: bool) {
        let mut provider_config = config();
        provider_config.url = url.to_string();

        assert_eq!(
            LdapAuthProvider::new(provider_config).unwrap().starttls,
            starttls
        );
    }

    #[test]
    fn group_filters_apply_after_name_extraction() {
        let provider = LdapAuthProvider::new(config()).unwrap();
        let entry = SearchEntry {
            dn: "uid=alice,ou=people,dc=example,dc=org".to_string(),
            attrs: HashMap::from([(
                "MEMBEROF".to_string(),
                vec![
                    "cn=admin,ou=groups,dc=example,dc=org".to_string(),
                    "cn=mreg-editors,ou=groups,dc=example,dc=org".to_string(),
                    "cn=irrelevant,ou=groups,dc=example,dc=org".to_string(),
                ],
            )]),
            bin_attrs: HashMap::new(),
        };

        assert_eq!(
            provider.groups_from_entry(&entry),
            vec!["admin".to_string(), "mreg-editors".to_string()]
        );
    }
}
