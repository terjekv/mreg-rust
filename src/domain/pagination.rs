use serde::{Deserialize, Serialize};
use utoipa::IntoParams;
use uuid::Uuid;

const DEFAULT_LIMIT: u64 = 100;
const MAX_LIMIT: u64 = 1000;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

/// Cursor-based page request with sort support.
#[derive(Clone, Debug, Default, Deserialize, IntoParams)]
pub struct PageRequest {
    /// UUID of the last item from the previous page. Omit for the first page.
    pub after: Option<Uuid>,
    /// Maximum number of items to return (default 100, max 1000).
    #[serde(default)]
    pub limit: Option<u64>,
    /// Field name to sort by. Entity-specific; defaults vary per entity.
    pub sort_by: Option<String>,
    /// Sort direction: "asc" (default) or "desc".
    #[serde(default)]
    pub sort_dir: Option<SortDirection>,
}

impl PageRequest {
    /// Returns a page request that fetches all items (no limit).
    pub fn all() -> Self {
        Self {
            after: None,
            limit: Some(u64::MAX),
            sort_by: None,
            sort_dir: None,
        }
    }

    pub fn limit(&self) -> u64 {
        match self.limit {
            Some(u64::MAX) => u64::MAX,
            Some(l) if l > MAX_LIMIT => MAX_LIMIT,
            Some(l) => l,
            None => DEFAULT_LIMIT,
        }
    }

    pub fn after(&self) -> Option<Uuid> {
        self.after
    }

    pub fn sort_by(&self) -> Option<&str> {
        self.sort_by.as_deref()
    }

    pub fn sort_direction(&self) -> &SortDirection {
        self.sort_dir.as_ref().unwrap_or(&SortDirection::Asc)
    }
}

/// Cursor-based page returned from the storage layer.
#[derive(Clone, Debug)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub next_cursor: Option<Uuid>,
}

/// Serializable page response for the API layer.
#[derive(Clone, Debug, Serialize)]
pub struct PageResponse<T: Serialize> {
    pub items: Vec<T>,
    pub total: u64,
    pub next_cursor: Option<Uuid>,
}

/// Macro to generate a concrete, utoipa-visible page-response wrapper for a given item type.
///
/// Usage:
/// ```ignore
/// page_response!(LabelPageResponse, LabelResponse, "Paginated list of labels");
/// ```
#[macro_export]
macro_rules! page_response {
    ($name:ident, $item:ty, $desc:expr) => {
        #[doc = $desc]
        #[derive(serde::Serialize, utoipa::ToSchema)]
        pub struct $name {
            pub items: Vec<$item>,
            pub total: u64,
            pub next_cursor: Option<uuid::Uuid>,
        }

        impl From<$crate::domain::pagination::PageResponse<$item>> for $name {
            fn from(page: $crate::domain::pagination::PageResponse<$item>) -> Self {
                Self {
                    items: page.items,
                    total: page.total,
                    next_cursor: page.next_cursor,
                }
            }
        }
    };
}

impl<T: Serialize> PageResponse<T> {
    pub fn from_page<D>(page: Page<D>, mapper: impl Fn(&D) -> T) -> Self {
        Self {
            items: page.items.iter().map(mapper).collect(),
            total: page.total,
            next_cursor: page.next_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_LIMIT, PageRequest};

    #[test]
    fn page_limit_defaults_when_unspecified() {
        assert_eq!(PageRequest::default().limit(), 100);
    }

    #[test]
    fn page_limit_caps_user_supplied_values() {
        let page = PageRequest {
            limit: Some(MAX_LIMIT + 1),
            ..Default::default()
        };
        assert_eq!(page.limit(), MAX_LIMIT);
    }

    #[test]
    fn page_limit_preserves_internal_fetch_all() {
        assert_eq!(PageRequest::all().limit(), u64::MAX);
    }
}
