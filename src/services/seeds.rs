use serde_json::json;
use uuid::Uuid;

use crate::{
    audit::{CreateHistoryEvent, actions},
    domain::{
        host_policy::{CreateHostPolicyAtom, CreateHostPolicyRole},
        label::CreateLabel,
        nameserver::CreateNameServer,
        network_policy::{
            CreateNetworkPolicy, CreateNetworkPolicyAttribute, SetNetworkPolicyAttributeValue,
        },
        seeds::{SeedData, SeedItem},
    },
    errors::AppError,
    events::{DomainEvent, EventSinkClient},
    storage::{DynStorage, TxStorage},
};

pub(super) async fn apply(
    storage: &DynStorage,
    data: &SeedData,
    events: &EventSinkClient,
) -> Result<usize, AppError> {
    if data.items().is_empty() {
        return Ok(0);
    }
    let data = data.clone();
    let history = storage
        .transaction(move |tx| {
            tx.lock_seed_data()?;
            let mut history = Vec::new();
            for item in data.items() {
                if let Some(id) = item.create_if_missing(tx).map_err(|error| {
                    AppError::config(format!(
                        "seed {} '{}' failed: {error}",
                        item.kind(),
                        item.name()
                    ))
                })? {
                    history.push(tx.audit().record_event(CreateHistoryEvent::new(
                        "system:seed",
                        item.kind(),
                        Some(id),
                        item.name(),
                        actions::CREATE,
                        json!({"name": item.name(), "source": "seed"}),
                    ))?);
                }
            }
            Ok(history)
        })
        .await?;
    for event in &history {
        events.emit(&DomainEvent::from(event)).await;
    }
    Ok(history.len())
}

/// Only a missing natural key permits creation. Existing data is never modified.
fn is_missing<T>(lookup: Result<T, AppError>) -> Result<bool, AppError> {
    match lookup {
        Ok(_) => Ok(false),
        Err(AppError::NotFound(_)) => Ok(true),
        Err(error) => Err(error),
    }
}

impl SeedItem {
    fn create_if_missing(&self, tx: &dyn TxStorage) -> Result<Option<Uuid>, AppError> {
        let id = match self {
            Self::Label { name, description } => {
                if !is_missing(tx.labels().get_label_by_name(name))? {
                    return Ok(None);
                }
                tx.labels()
                    .create_label(CreateLabel::new(name.clone(), description.as_str())?)?
                    .id()
            }
            Self::Nameserver { name, ttl } => {
                if !is_missing(tx.nameservers().get_nameserver_by_name(name))? {
                    return Ok(None);
                }
                tx.nameservers()
                    .create_nameserver(CreateNameServer::new(name.clone(), *ttl))?
                    .id()
            }
            Self::NetworkPolicyAttribute { name, description } => {
                if !is_missing(
                    tx.network_policies()
                        .get_network_policy_attribute_by_name(name),
                )? {
                    return Ok(None);
                }
                tx.network_policies()
                    .create_network_policy_attribute(CreateNetworkPolicyAttribute::new(
                        name.clone(),
                        description.clone(),
                    ))?
                    .id()
            }
            Self::NetworkPolicy {
                name,
                description,
                community_template_pattern,
                attributes,
            } => {
                if !is_missing(tx.network_policies().get_network_policy_by_name(name))? {
                    return Ok(None);
                }
                let command = CreateNetworkPolicy::new(
                    name.clone(),
                    description.as_str(),
                    community_template_pattern
                        .as_ref()
                        .map(|pattern| pattern.as_str().to_owned()),
                )?
                .with_attributes(
                    attributes
                        .iter()
                        .map(|attribute| {
                            SetNetworkPolicyAttributeValue::new(
                                attribute.name().clone(),
                                attribute.value(),
                            )
                        })
                        .collect(),
                );
                tx.network_policies().create_network_policy(command)?.id()
            }
            Self::HostPolicyAtom { name, description } => {
                if !is_missing(tx.host_policy().get_atom_by_name(name))? {
                    return Ok(None);
                }
                tx.host_policy()
                    .create_atom(CreateHostPolicyAtom::new(name.clone(), description.clone()))?
                    .id()
            }
            Self::HostPolicyRole { name, description } => {
                if !is_missing(tx.host_policy().get_role_by_name(name))? {
                    return Ok(None);
                }
                tx.host_policy()
                    .create_role(CreateHostPolicyRole::new(name.clone(), description.clone()))?
                    .id()
            }
        };
        Ok(Some(id))
    }
}
