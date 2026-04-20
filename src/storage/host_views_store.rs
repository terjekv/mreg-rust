use async_trait::async_trait;

use crate::{
    domain::{
        filters::HostFilter,
        host_view::{HostView, HostViewExpansions},
        pagination::{Page, PageRequest},
        types::Hostname,
    },
    errors::AppError,
};

/// Projection-oriented host reads used by handlers and other consumers that
/// need assembled inventory detail rather than the core `Host` entity alone.
#[async_trait]
pub trait HostViewStore: Send + Sync {
    async fn list_host_views(
        &self,
        page: &PageRequest,
        filter: &HostFilter,
        expansions: HostViewExpansions,
    ) -> Result<Page<HostView>, AppError>;

    async fn get_host_view(
        &self,
        name: &Hostname,
        expansions: HostViewExpansions,
    ) -> Result<HostView, AppError>;
}
