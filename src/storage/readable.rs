use crate::errors::AppError;

use super::{DynStorage, StorageBackendKind, StorageCapabilities, StorageHealthReport};

/// Narrow read-only storage view used by HTTP handlers for backend diagnostics.
///
/// Domain reads should go through the service facade so handlers cannot bypass
/// the service layer and reach write-capable store traits directly.
#[derive(Clone)]
pub struct ReadableStorage {
    inner: DynStorage,
}

impl ReadableStorage {
    pub fn new(storage: DynStorage) -> Self {
        Self { inner: storage }
    }

    pub fn backend_kind(&self) -> StorageBackendKind {
        self.inner.backend_kind()
    }

    pub fn capabilities(&self) -> StorageCapabilities {
        self.inner.capabilities()
    }

    pub async fn health(&self) -> Result<StorageHealthReport, AppError> {
        self.inner.health().await
    }
}
