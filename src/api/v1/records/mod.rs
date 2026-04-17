pub mod instances;
pub mod record_types;
pub mod rrsets;

// Glob re-exports so that utoipa's generated `__path_*` types are visible
// to the OpenAPI derive in `src/api/mod.rs`.
pub use instances::*;
pub use record_types::*;
pub use rrsets::*;

use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    record_types::configure(cfg);
    instances::configure(cfg);
    rrsets::configure(cfg);
}
