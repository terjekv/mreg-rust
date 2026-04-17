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
