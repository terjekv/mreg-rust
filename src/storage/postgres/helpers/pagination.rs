use crate::{
    domain::pagination::{Page, PageRequest},
    storage::has_id::HasId,
};

pub(in crate::storage::postgres) fn vec_to_page<T: HasId>(
    items: Vec<T>,
    page: &PageRequest,
) -> Page<T> {
    vec_to_page_with_cursor(items, page)
}

/// Build a page from rows that may only have been limited in SQL for the first page.
/// `total` is a precomputed `COUNT(*)` from a separate query.
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

/// Build a page from rows where SQL has already applied both the cursor and
/// `LIMIT limit + 1`.
pub(in crate::storage::postgres) fn limited_rows_to_page<T: HasId>(
    mut items: Vec<T>,
    page: &PageRequest,
    total: u64,
) -> Page<T> {
    let limit = page.limit() as usize;
    let has_more = items.len() > limit;
    if has_more {
        items.pop();
    }
    Page {
        next_cursor: if has_more {
            items.last().map(HasId::id)
        } else {
            None
        },
        items,
        total,
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
            .unwrap_or(items.len())
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
