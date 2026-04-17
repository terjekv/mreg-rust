use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    domain::types::{
        CidrValue, DnsName, EmailAddressValue, SerialNumber, SoaSeconds, Ttl, ZoneName,
    },
    errors::AppError,
};

/// Forward DNS zone with SOA parameters and nameserver list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForwardZone {
    id: Uuid,
    name: ZoneName,
    updated: bool,
    primary_ns: DnsName,
    nameservers: Vec<DnsName>,
    email: EmailAddressValue,
    serial_no: SerialNumber,
    serial_no_updated_at: DateTime<Utc>,
    refresh: SoaSeconds,
    retry: SoaSeconds,
    expire: SoaSeconds,
    soa_ttl: Ttl,
    default_ttl: Ttl,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ForwardZone {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        name: ZoneName,
        updated: bool,
        primary_ns: DnsName,
        nameservers: Vec<DnsName>,
        email: EmailAddressValue,
        serial_no: SerialNumber,
        serial_no_updated_at: DateTime<Utc>,
        refresh: SoaSeconds,
        retry: SoaSeconds,
        expire: SoaSeconds,
        soa_ttl: Ttl,
        default_ttl: Ttl,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        let nameservers = normalize_nameservers(primary_ns.clone(), nameservers);
        Ok(Self {
            id,
            name,
            updated,
            primary_ns,
            nameservers,
            email,
            serial_no,
            serial_no_updated_at,
            refresh,
            retry,
            expire,
            soa_ttl,
            default_ttl,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn name(&self) -> &ZoneName {
        &self.name
    }
    pub fn updated(&self) -> bool {
        self.updated
    }
    pub fn primary_ns(&self) -> &DnsName {
        &self.primary_ns
    }
    pub fn nameservers(&self) -> &[DnsName] {
        &self.nameservers
    }
    pub fn email(&self) -> &EmailAddressValue {
        &self.email
    }
    pub fn serial_no(&self) -> SerialNumber {
        self.serial_no
    }
    pub fn serial_no_updated_at(&self) -> DateTime<Utc> {
        self.serial_no_updated_at
    }
    pub fn refresh(&self) -> SoaSeconds {
        self.refresh
    }
    pub fn retry(&self) -> SoaSeconds {
        self.retry
    }
    pub fn expire(&self) -> SoaSeconds {
        self.expire
    }
    pub fn soa_ttl(&self) -> Ttl {
        self.soa_ttl
    }
    pub fn default_ttl(&self) -> Ttl {
        self.default_ttl
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Reverse DNS zone (in-addr.arpa / ip6.arpa) with optional network association.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReverseZone {
    id: Uuid,
    name: ZoneName,
    network: Option<CidrValue>,
    updated: bool,
    primary_ns: DnsName,
    nameservers: Vec<DnsName>,
    email: EmailAddressValue,
    serial_no: SerialNumber,
    serial_no_updated_at: DateTime<Utc>,
    refresh: SoaSeconds,
    retry: SoaSeconds,
    expire: SoaSeconds,
    soa_ttl: Ttl,
    default_ttl: Ttl,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ReverseZone {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        name: ZoneName,
        network: Option<CidrValue>,
        updated: bool,
        primary_ns: DnsName,
        nameservers: Vec<DnsName>,
        email: EmailAddressValue,
        serial_no: SerialNumber,
        serial_no_updated_at: DateTime<Utc>,
        refresh: SoaSeconds,
        retry: SoaSeconds,
        expire: SoaSeconds,
        soa_ttl: Ttl,
        default_ttl: Ttl,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        let nameservers = normalize_nameservers(primary_ns.clone(), nameservers);
        Ok(Self {
            id,
            name,
            network,
            updated,
            primary_ns,
            nameservers,
            email,
            serial_no,
            serial_no_updated_at,
            refresh,
            retry,
            expire,
            soa_ttl,
            default_ttl,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn name(&self) -> &ZoneName {
        &self.name
    }
    pub fn network(&self) -> Option<&CidrValue> {
        self.network.as_ref()
    }
    pub fn updated(&self) -> bool {
        self.updated
    }
    pub fn primary_ns(&self) -> &DnsName {
        &self.primary_ns
    }
    pub fn nameservers(&self) -> &[DnsName] {
        &self.nameservers
    }
    pub fn email(&self) -> &EmailAddressValue {
        &self.email
    }
    pub fn serial_no(&self) -> SerialNumber {
        self.serial_no
    }
    pub fn serial_no_updated_at(&self) -> DateTime<Utc> {
        self.serial_no_updated_at
    }
    pub fn refresh(&self) -> SoaSeconds {
        self.refresh
    }
    pub fn retry(&self) -> SoaSeconds {
        self.retry
    }
    pub fn expire(&self) -> SoaSeconds {
        self.expire
    }
    pub fn soa_ttl(&self) -> Ttl {
        self.soa_ttl
    }
    pub fn default_ttl(&self) -> Ttl {
        self.default_ttl
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Command to create a new forward DNS zone.
#[derive(Clone, Debug)]
pub struct CreateForwardZone {
    name: ZoneName,
    primary_ns: DnsName,
    nameservers: Vec<DnsName>,
    email: EmailAddressValue,
    serial_no: SerialNumber,
    refresh: SoaSeconds,
    retry: SoaSeconds,
    expire: SoaSeconds,
    soa_ttl: Ttl,
    default_ttl: Ttl,
}

impl CreateForwardZone {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: ZoneName,
        primary_ns: DnsName,
        nameservers: Vec<DnsName>,
        email: EmailAddressValue,
        serial_no: SerialNumber,
        refresh: SoaSeconds,
        retry: SoaSeconds,
        expire: SoaSeconds,
        soa_ttl: Ttl,
        default_ttl: Ttl,
    ) -> Self {
        Self {
            name,
            primary_ns: primary_ns.clone(),
            nameservers: normalize_nameservers(primary_ns, nameservers),
            email,
            serial_no,
            refresh,
            retry,
            expire,
            soa_ttl,
            default_ttl,
        }
    }

    pub fn name(&self) -> &ZoneName {
        &self.name
    }
    pub fn primary_ns(&self) -> &DnsName {
        &self.primary_ns
    }
    pub fn nameservers(&self) -> &[DnsName] {
        &self.nameservers
    }
    pub fn email(&self) -> &EmailAddressValue {
        &self.email
    }
    pub fn serial_no(&self) -> SerialNumber {
        self.serial_no
    }
    pub fn refresh(&self) -> SoaSeconds {
        self.refresh
    }
    pub fn retry(&self) -> SoaSeconds {
        self.retry
    }
    pub fn expire(&self) -> SoaSeconds {
        self.expire
    }
    pub fn soa_ttl(&self) -> Ttl {
        self.soa_ttl
    }
    pub fn default_ttl(&self) -> Ttl {
        self.default_ttl
    }
}

/// Command to create a new reverse DNS zone.
#[derive(Clone, Debug)]
pub struct CreateReverseZone {
    name: ZoneName,
    network: Option<CidrValue>,
    primary_ns: DnsName,
    nameservers: Vec<DnsName>,
    email: EmailAddressValue,
    serial_no: SerialNumber,
    refresh: SoaSeconds,
    retry: SoaSeconds,
    expire: SoaSeconds,
    soa_ttl: Ttl,
    default_ttl: Ttl,
}

impl CreateReverseZone {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: ZoneName,
        network: Option<CidrValue>,
        primary_ns: DnsName,
        nameservers: Vec<DnsName>,
        email: EmailAddressValue,
        serial_no: SerialNumber,
        refresh: SoaSeconds,
        retry: SoaSeconds,
        expire: SoaSeconds,
        soa_ttl: Ttl,
        default_ttl: Ttl,
    ) -> Self {
        Self {
            name,
            network,
            primary_ns: primary_ns.clone(),
            nameservers: normalize_nameservers(primary_ns, nameservers),
            email,
            serial_no,
            refresh,
            retry,
            expire,
            soa_ttl,
            default_ttl,
        }
    }

    pub fn name(&self) -> &ZoneName {
        &self.name
    }
    pub fn network(&self) -> Option<&CidrValue> {
        self.network.as_ref()
    }
    pub fn primary_ns(&self) -> &DnsName {
        &self.primary_ns
    }
    pub fn nameservers(&self) -> &[DnsName] {
        &self.nameservers
    }
    pub fn email(&self) -> &EmailAddressValue {
        &self.email
    }
    pub fn serial_no(&self) -> SerialNumber {
        self.serial_no
    }
    pub fn refresh(&self) -> SoaSeconds {
        self.refresh
    }
    pub fn retry(&self) -> SoaSeconds {
        self.retry
    }
    pub fn expire(&self) -> SoaSeconds {
        self.expire
    }
    pub fn soa_ttl(&self) -> Ttl {
        self.soa_ttl
    }
    pub fn default_ttl(&self) -> Ttl {
        self.default_ttl
    }
}

/// Partial update for a forward zone's SOA parameters and nameservers.
#[derive(Clone, Debug, Default)]
pub struct UpdateForwardZone {
    pub primary_ns: Option<DnsName>,
    pub nameservers: Option<Vec<DnsName>>,
    pub email: Option<EmailAddressValue>,
    pub refresh: Option<SoaSeconds>,
    pub retry: Option<SoaSeconds>,
    pub expire: Option<SoaSeconds>,
    pub soa_ttl: Option<Ttl>,
    pub default_ttl: Option<Ttl>,
}

/// Partial update for a reverse zone's SOA parameters and nameservers.
#[derive(Clone, Debug, Default)]
pub struct UpdateReverseZone {
    pub primary_ns: Option<DnsName>,
    pub nameservers: Option<Vec<DnsName>>,
    pub email: Option<EmailAddressValue>,
    pub refresh: Option<SoaSeconds>,
    pub retry: Option<SoaSeconds>,
    pub expire: Option<SoaSeconds>,
    pub soa_ttl: Option<Ttl>,
    pub default_ttl: Option<Ttl>,
}

/// Sub-zone delegation within a forward zone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForwardZoneDelegation {
    id: Uuid,
    zone_id: Uuid,
    name: DnsName,
    comment: String,
    nameservers: Vec<DnsName>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ForwardZoneDelegation {
    pub fn restore(
        id: Uuid,
        zone_id: Uuid,
        name: DnsName,
        comment: String,
        nameservers: Vec<DnsName>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        Ok(Self {
            id,
            zone_id,
            name,
            comment,
            nameservers,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn zone_id(&self) -> Uuid {
        self.zone_id
    }
    pub fn name(&self) -> &DnsName {
        &self.name
    }
    pub fn comment(&self) -> &str {
        &self.comment
    }
    pub fn nameservers(&self) -> &[DnsName] {
        &self.nameservers
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Command to create a forward zone delegation.
#[derive(Clone, Debug)]
pub struct CreateForwardZoneDelegation {
    zone_name: ZoneName,
    name: DnsName,
    comment: String,
    nameservers: Vec<DnsName>,
}

impl CreateForwardZoneDelegation {
    pub fn new(
        zone_name: ZoneName,
        name: DnsName,
        comment: String,
        nameservers: Vec<DnsName>,
    ) -> Self {
        Self {
            zone_name,
            name,
            comment,
            nameservers,
        }
    }

    pub fn zone_name(&self) -> &ZoneName {
        &self.zone_name
    }
    pub fn name(&self) -> &DnsName {
        &self.name
    }
    pub fn comment(&self) -> &str {
        &self.comment
    }
    pub fn nameservers(&self) -> &[DnsName] {
        &self.nameservers
    }
}

/// Sub-zone delegation within a reverse zone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReverseZoneDelegation {
    id: Uuid,
    zone_id: Uuid,
    name: DnsName,
    comment: String,
    nameservers: Vec<DnsName>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ReverseZoneDelegation {
    pub fn restore(
        id: Uuid,
        zone_id: Uuid,
        name: DnsName,
        comment: String,
        nameservers: Vec<DnsName>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        Ok(Self {
            id,
            zone_id,
            name,
            comment,
            nameservers,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn zone_id(&self) -> Uuid {
        self.zone_id
    }
    pub fn name(&self) -> &DnsName {
        &self.name
    }
    pub fn comment(&self) -> &str {
        &self.comment
    }
    pub fn nameservers(&self) -> &[DnsName] {
        &self.nameservers
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Command to create a reverse zone delegation.
#[derive(Clone, Debug)]
pub struct CreateReverseZoneDelegation {
    zone_name: ZoneName,
    name: DnsName,
    comment: String,
    nameservers: Vec<DnsName>,
}

impl CreateReverseZoneDelegation {
    pub fn new(
        zone_name: ZoneName,
        name: DnsName,
        comment: String,
        nameservers: Vec<DnsName>,
    ) -> Self {
        Self {
            zone_name,
            name,
            comment,
            nameservers,
        }
    }

    pub fn zone_name(&self) -> &ZoneName {
        &self.zone_name
    }
    pub fn name(&self) -> &DnsName {
        &self.name
    }
    pub fn comment(&self) -> &str {
        &self.comment
    }
    pub fn nameservers(&self) -> &[DnsName] {
        &self.nameservers
    }
}

fn normalize_nameservers(primary_ns: DnsName, nameservers: Vec<DnsName>) -> Vec<DnsName> {
    let mut normalized = vec![primary_ns.clone()];
    for nameserver in nameservers {
        if !normalized.iter().any(|existing| existing == &nameserver) {
            normalized.push(nameserver);
        }
    }
    normalized
}
