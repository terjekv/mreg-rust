use crate::{
    domain::{
        filters::HostFilter,
        host::Host,
        host_view::{HostView, HostViewExpansions},
        pagination::{Page, PageRequest},
        types::Hostname,
    },
    errors::AppError,
    storage::HostViewStore,
};

pub struct HostViewService<'a> {
    pub(crate) store: &'a (dyn HostViewStore + Send + Sync),
}

impl HostViewService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &HostFilter,
        expansions: HostViewExpansions,
    ) -> Result<Page<HostView>, AppError> {
        self.store.list_host_views(page, filter, expansions).await
    }

    pub async fn get(
        &self,
        name: &Hostname,
        expansions: HostViewExpansions,
    ) -> Result<HostView, AppError> {
        self.store.get_host_view(name, expansions).await
    }

    pub async fn from_host(
        &self,
        host: &Host,
        expansions: HostViewExpansions,
    ) -> Result<HostView, AppError> {
        self.store.get_host_view(host.name(), expansions).await
    }
}
