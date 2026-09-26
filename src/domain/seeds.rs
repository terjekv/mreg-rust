//! Validated, deployment-defined initial catalog data, independent of HTTP APIs.

use std::collections::BTreeSet;

use serde::Deserialize;

use crate::{
    domain::types::{
        CommunityTemplatePattern, DnsName, HostPolicyName, LabelName, NetworkPolicyAttributeName,
        NetworkPolicyName, RequiredDescription, Ttl,
    },
    errors::AppError,
};

/// An optional set of create-if-missing entries, applied in document order.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(try_from = "SeedDocument")]
pub struct SeedData {
    items: Vec<SeedItem>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedDocument {
    #[serde(default)]
    items: Vec<SeedItem>,
}

impl TryFrom<SeedDocument> for SeedData {
    type Error = AppError;

    fn try_from(document: SeedDocument) -> Result<Self, Self::Error> {
        let mut identities = BTreeSet::new();
        for item in &document.items {
            if !identities.insert((item.kind(), item.name())) {
                return Err(AppError::validation(format!(
                    "duplicate seed {} '{}'",
                    item.kind(),
                    item.name()
                )));
            }
        }
        Ok(Self {
            items: document.items,
        })
    }
}

impl SeedData {
    pub(crate) fn items(&self) -> &[SeedItem] {
        &self.items
    }
}

/// Each variant carries the same validated scalar types as its domain command.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SeedItem {
    Label {
        name: LabelName,
        description: RequiredDescription,
    },
    Nameserver {
        name: DnsName,
        ttl: Option<Ttl>,
    },
    NetworkPolicyAttribute {
        name: NetworkPolicyAttributeName,
        #[serde(default)]
        description: String,
    },
    NetworkPolicy {
        name: NetworkPolicyName,
        description: RequiredDescription,
        community_template_pattern: Option<CommunityTemplatePattern>,
        #[serde(default)]
        attributes: Vec<SeedAttributeValue>,
    },
    HostPolicyAtom {
        name: HostPolicyName,
        #[serde(default)]
        description: String,
    },
    HostPolicyRole {
        name: HostPolicyName,
        #[serde(default)]
        description: String,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SeedAttributeValue {
    name: NetworkPolicyAttributeName,
    value: bool,
}

impl SeedAttributeValue {
    pub(crate) fn name(&self) -> &NetworkPolicyAttributeName {
        &self.name
    }
    pub(crate) fn value(&self) -> bool {
        self.value
    }
}

impl SeedItem {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Label { .. } => "label",
            Self::Nameserver { .. } => "nameserver",
            Self::NetworkPolicyAttribute { .. } => "network_policy_attribute",
            Self::NetworkPolicy { .. } => "network_policy",
            Self::HostPolicyAtom { .. } => "host_policy_atom",
            Self::HostPolicyRole { .. } => "host_policy_role",
        }
    }

    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Label { name, .. } => name.as_str(),
            Self::Nameserver { name, .. } => name.as_str(),
            Self::NetworkPolicyAttribute { name, .. } => name.as_str(),
            Self::NetworkPolicy { name, .. } => name.as_str(),
            Self::HostPolicyAtom { name, .. } | Self::HostPolicyRole { name, .. } => name.as_str(),
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::{Value, json};

    use super::SeedData;

    #[rstest]
    #[case(json!({"entries": []}))]
    #[case(json!({"items": [{"kind": "unknown", "name": "test"}]}))]
    #[case(json!({"items": [{"kind": "network_policy_attribute", "name": "bad name"}]}))]
    #[case(json!({"items": [{"kind": "label", "name": "test", "description": " "}]}))]
    #[case(json!({"items": [{"kind": "network_policy", "name": "test", "description": " "}]}))]
    #[case(json!({"items": [{"kind": "host_policy_atom", "name": "bad name"}]}))]
    #[case(json!({"items": [{"kind": "host_policy_role", "name": "bad name"}]}))]
    #[case(json!({"items": [{"kind": "nameserver", "name": "ns.example.org", "ttl": -1}]}))]
    #[case(json!({"items": [{"kind": "network_policy_attribute", "name": "test", "descripton": "typo"}]}))]
    #[case(json!({"items": [
        {"kind": "network_policy_attribute", "name": "CUSTOM"},
        {"kind": "network_policy_attribute", "name": "custom"}
    ]}))]
    fn rejects_invalid_seed_documents(#[case] document: Value) {
        assert!(serde_json::from_value::<SeedData>(document).is_err());
    }

    #[rstest]
    #[case(include_str!("../../seeds.example.toml"), 6)]
    #[case(include_str!("../../scripts/mreg-cli-seeds.toml"), 1)]
    #[case("", 0)]
    fn parses_seed_files(#[case] document: &str, #[case] count: usize) {
        assert_eq!(
            toml::from_str::<SeedData>(document).unwrap().items().len(),
            count
        );
    }
}
