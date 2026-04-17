use uuid::Uuid;

use crate::{
    audit::HistoryEvent,
    domain::{
        attachment::{AttachmentCommunityAssignment, HostAttachment},
        community::Community,
        exports::{ExportRun, ExportTemplate},
        host::{Host, IpAddressAssignment},
        host_community_assignment::HostCommunityAssignment,
        host_contact::HostContact,
        host_group::HostGroup,
        host_policy::{HostPolicyAtom, HostPolicyRole},
        imports::ImportBatchSummary,
        label::Label,
        nameserver::NameServer,
        network::{ExcludedRange, Network},
        network_policy::NetworkPolicy,
        pagination::{Page, PageRequest},
        ptr_override::PtrOverride,
        resource_records::{RecordInstance, RecordRrset, RecordTypeDefinition},
        tasks::TaskEnvelope,
        zone::{ForwardZone, ForwardZoneDelegation, ReverseZone, ReverseZoneDelegation},
    },
};

pub(in crate::storage::postgres) trait HasId {
    fn id(&self) -> Uuid;
}

macro_rules! impl_has_id {
    ($($type:ty),*$(,)?) => {
        $(
            impl HasId for $type {
                fn id(&self) -> Uuid {
                    self.id()
                }
            }
        )*
    };
}

impl_has_id!(
    HostPolicyAtom,
    HostPolicyRole,
    Label,
    NameServer,
    ForwardZone,
    ReverseZone,
    ForwardZoneDelegation,
    ReverseZoneDelegation,
    Network,
    ExcludedRange,
    Host,
    IpAddressAssignment,
    HostContact,
    HostGroup,
    PtrOverride,
    NetworkPolicy,
    Community,
    HostAttachment,
    AttachmentCommunityAssignment,
    HostCommunityAssignment,
    TaskEnvelope,
    ImportBatchSummary,
    ExportTemplate,
    ExportRun,
    RecordTypeDefinition,
    RecordRrset,
    RecordInstance,
    HistoryEvent,
);

pub(in crate::storage::postgres) fn vec_to_page<T: HasId>(
    items: Vec<T>,
    page: &PageRequest,
) -> Page<T> {
    vec_to_page_with_cursor(items, page)
}

/// Build a Page from rows already limited by SQL (LIMIT limit+1 when no cursor).
/// When no cursor: rows has at most limit+1 items; pop the extra to determine has_next.
/// When cursor present: rows may be the full set (cursor filtering done in Rust via vec_to_page).
/// `total` is a precomputed COUNT(*) from a separate query.
pub(in crate::storage::postgres) fn rows_to_page<T: HasId>(
    items: Vec<T>,
    page: &PageRequest,
    total: u64,
) -> Page<T> {
    if page.after().is_some() {
        // Cursor present: use existing Rust-side cursor logic (full result set required).
        let mut page_result = vec_to_page_with_cursor(items, page);
        page_result.total = total;
        page_result
    } else {
        // No cursor: SQL already applied LIMIT limit+1, just check for next page.
        let limit = page.limit() as usize;
        let mut page_items = items;
        let has_more = page_items.len() > limit;
        if has_more {
            page_items.pop();
        }
        Page {
            next_cursor: if has_more {
                page_items.last().map(|item| item.id())
            } else {
                None
            },
            items: page_items,
            total,
        }
    }
}

pub(in crate::storage::postgres) fn paginate_simple<T>(
    items: Vec<T>,
    page: &PageRequest,
) -> Page<T> {
    let total = items.len() as u64;
    let limit = page.limit() as usize;
    let page_items: Vec<T> = items.into_iter().take(limit).collect();
    Page {
        items: page_items,
        total,
        next_cursor: None,
    }
}

fn vec_to_page_with_cursor<T: HasId>(items: Vec<T>, page: &PageRequest) -> Page<T> {
    let total = items.len() as u64;
    let start = if let Some(cursor) = page.after() {
        items
            .iter()
            .position(|item| item.id() == cursor)
            .map(|position| position + 1)
            .unwrap_or(0)
    } else {
        0
    };
    let limit = page.limit() as usize;
    let take_count = limit.saturating_add(1);
    let mut page_items: Vec<T> = items.into_iter().skip(start).take(take_count).collect();
    let has_more = page_items.len() > limit;
    if has_more {
        page_items.pop();
    }
    Page {
        next_cursor: if has_more {
            page_items.last().map(|item| item.id())
        } else {
            None
        },
        items: page_items,
        total,
    }
}
