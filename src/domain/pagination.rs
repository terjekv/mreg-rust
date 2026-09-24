use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use utoipa::IntoParams;
use uuid::Uuid;

use crate::errors::AppError;

const DEFAULT_LIMIT: u64 = 100;
const MAX_LIMIT: u64 = 1000;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

/// Cursor-based page request with sort support.
#[derive(Clone, Debug, Default, Deserialize, IntoParams)]
pub struct PageRequest {
    /// Opaque cursor returned by the preceding page. Omit for the first page.
    pub after: Option<String>,
    /// Maximum number of items to return (default 100, max 1000).
    #[serde(default, deserialize_with = "deserialize_page_limit")]
    pub limit: Option<u64>,
    /// Field name to sort by. Entity-specific; defaults vary per entity.
    pub sort_by: Option<String>,
    /// Sort direction: "asc" (default) or "desc".
    #[serde(default)]
    pub sort_dir: Option<SortDirection>,
}

/// Deserialize a public page size, rejecting the non-progressing zero-size
/// request before it can reach either storage backend.
pub fn deserialize_page_limit<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let limit = Option::<u64>::deserialize(deserializer)?;
    if limit == Some(0) {
        return Err(D::Error::custom("pagination limit must be at least 1"));
    }
    Ok(limit)
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

    pub fn after(&self) -> Option<&str> {
        self.after.as_deref()
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
    pub next_cursor: Option<String>,
}

/// Serializable page response for the API layer.
#[derive(Clone, Debug, Serialize)]
pub struct PageResponse<T: Serialize> {
    pub items: Vec<T>,
    pub total: u64,
    pub next_cursor: Option<String>,
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
            pub next_cursor: Option<String>,
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

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct KeysetCursor {
    version: u8,
    sort_by: String,
    sort_dir: SortDirection,
    key: String,
    id: Uuid,
}

impl KeysetCursor {
    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    pub(crate) fn id(&self) -> Uuid {
        self.id
    }
}

pub(crate) fn encode_cursor(
    sort_by: &str,
    sort_dir: &SortDirection,
    key: String,
    id: Uuid,
) -> Result<String, AppError> {
    let cursor = KeysetCursor {
        version: 1,
        sort_by: sort_by.to_string(),
        sort_dir: sort_dir.clone(),
        key,
        id,
    };
    serde_json::to_vec(&cursor)
        .map(|bytes| URL_SAFE_NO_PAD.encode(bytes))
        .map_err(AppError::internal)
}

pub(crate) fn decode_cursor(
    value: &str,
    sort_by: &str,
    sort_dir: &SortDirection,
) -> Result<KeysetCursor, AppError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AppError::validation("pagination cursor is malformed"))?;
    let cursor: KeysetCursor = serde_json::from_slice(&bytes)
        .map_err(|_| AppError::validation("pagination cursor is malformed"))?;
    if cursor.version != 1 {
        return Err(AppError::validation(
            "pagination cursor version is unsupported",
        ));
    }
    if cursor.sort_by != sort_by || &cursor.sort_dir != sort_dir {
        return Err(AppError::validation(
            "pagination cursor does not match the requested sort",
        ));
    }
    Ok(cursor)
}

/// Paginate an already-sorted collection using a stable key and identifier.
///
/// This is used for resources such as BACnet assignments whose natural
/// identifier is converted to a UUID-shaped cursor tie breaker.
pub(crate) fn paginate_by_key<T>(
    items: Vec<T>,
    page: &PageRequest,
    sort_by: &str,
    sort_dir: &SortDirection,
    key_fn: impl Fn(&T) -> String,
    id_fn: impl Fn(&T) -> Uuid,
) -> Result<Page<T>, AppError> {
    let total = items.len() as u64;
    let start = if let Some(value) = page.after() {
        let cursor = decode_cursor(value, sort_by, sort_dir)?;
        items
            .iter()
            .position(|item| {
                let comparison = key_fn(item).as_str().cmp(cursor.key());
                (match sort_dir {
                    SortDirection::Asc => comparison.is_gt(),
                    SortDirection::Desc => comparison.is_lt(),
                }) || (comparison.is_eq() && id_fn(item) > cursor.id())
            })
            .unwrap_or(items.len())
    } else {
        0
    };
    let limit = page.limit() as usize;
    let mut page_items = items
        .into_iter()
        .skip(start)
        .take(limit.saturating_add(1))
        .collect::<Vec<_>>();
    let has_more = page_items.len() > limit;
    if has_more {
        page_items.pop();
    }
    let next_cursor = if has_more {
        page_items
            .last()
            .map(|item| encode_cursor(sort_by, sort_dir, key_fn(item), id_fn(item)))
            .transpose()?
    } else {
        None
    };
    Ok(Page {
        items: page_items,
        total,
        next_cursor,
    })
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

    #[test]
    fn page_limit_rejects_zero_during_deserialization() {
        assert!(serde_json::from_str::<PageRequest>(r#"{"limit":0}"#).is_err());
    }
}
