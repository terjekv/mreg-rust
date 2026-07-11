pub mod delegations;
pub mod forward;
pub mod reverse;

use actix_web::web;

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
fn default_serial_no() -> u64 {
    1
}

fn default_refresh() -> u32 {
    10_800
}

fn default_retry() -> u32 {
    3_600
}

fn default_expire() -> u32 {
    1_814_400
}

fn default_ttl_value() -> u32 {
    43_200
}
