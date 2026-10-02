pub mod delegations;
pub mod forward;
pub mod reverse;

use actix_web::web;

use crate::domain::types::{SerialNumber, SoaSeconds, Ttl};

// Re-export all public types for backward compatibility
pub use delegations::{
    CreateDelegationRequest, ForwardZoneDelegationResponse, ReverseZoneDelegationResponse,
};
pub use delegations::{
    create_forward_zone_delegation, create_reverse_zone_delegation, delete_forward_zone_delegation,
    delete_reverse_zone_delegation, list_forward_zone_delegations, list_reverse_zone_delegations,
};
pub use forward::{
    CreateForwardZoneRequest, ForwardZoneResponse, UpdateForwardZoneRequest, create_forward_zone,
    delete_forward_zone, get_forward_zone, list_forward_zones, update_forward_zone,
};
pub use reverse::{
    CreateReverseZoneRequest, ReverseZoneResponse, UpdateReverseZoneRequest, create_reverse_zone,
    delete_reverse_zone, get_reverse_zone, list_reverse_zones, update_reverse_zone,
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    forward::configure(cfg);
    reverse::configure(cfg);
    delegations::configure(cfg);
}

// Shared default functions used by serde defaults in forward and reverse modules
fn default_serial_no() -> SerialNumber {
    SerialNumber::new(1).expect("valid default serial_no")
}

fn default_refresh() -> SoaSeconds {
    SoaSeconds::new(10_800).expect("valid default refresh")
}

fn default_retry() -> SoaSeconds {
    SoaSeconds::new(3_600).expect("valid default retry")
}

fn default_expire() -> SoaSeconds {
    SoaSeconds::new(1_814_400).expect("valid default expire")
}

fn default_ttl_value() -> Ttl {
    Ttl::new(43_200).expect("valid default ttl_value")
}

fn default_negative_ttl() -> Ttl {
    Ttl::new(3_600).expect("valid default negative_ttl")
}
