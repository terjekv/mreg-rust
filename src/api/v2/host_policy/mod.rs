pub mod atoms;
pub mod role_membership;
pub mod roles;

pub use atoms::*;
pub use role_membership::*;
pub use roles::*;

use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    atoms::configure(cfg);
    roles::configure(cfg);
    role_membership::configure(cfg);
}
