use serde::{Deserialize, Serialize};

/// Entity type that owns a DNS record (host, zone, delegation, or nameserver).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordOwnerKind {
    Host,
    ForwardZone,
    ForwardZoneDelegation,
    ReverseZone,
    ReverseZoneDelegation,
    NameServer,
}

/// Syntax constraint for the owner name of a record type.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordOwnerNameSyntax {
    DnsName,
    Hostname,
}

/// DNS class (currently only IN supported).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum DnsClass {
    IN,
}
