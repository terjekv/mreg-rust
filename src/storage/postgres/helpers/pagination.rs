use crate::{
    domain::pagination::{Page, PageRequest, SortDirection, decode_cursor, encode_cursor},
    errors::AppError,
    storage::has_id::HasId,
};

pub(in crate::storage::postgres) fn vec_to_page_by<T: HasId>(
    items: Vec<T>,
    page: &PageRequest,
    sort_by: &str,
    sort_dir: &SortDirection,
    key_fn: impl Fn(&T) -> String,
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
                }) || (comparison.is_eq() && item.id() > cursor.id())
            })
            .unwrap_or(items.len())
    } else {
        0
    };
    page_from_start(items, page, total, start, sort_by, sort_dir, key_fn)
}

pub(in crate::storage::postgres) fn sort_and_vec_to_page_by<T: HasId>(
    mut items: Vec<T>,
    page: &PageRequest,
    valid_fields: &[&str],
    key_fn: impl Fn(&T, &str) -> String,
) -> Result<Page<T>, AppError> {
    if let Some(field) = page.sort_by()
        && !valid_fields.contains(&field)
    {
        return Err(AppError::validation(format!(
            "unsupported sort_by field: {field}"
        )));
    }
    let sort_by = page.sort_by().unwrap_or("name");
    let sort_dir = page.sort_direction();
    items.sort_by(|left, right| {
        let comparison = key_fn(left, sort_by).cmp(&key_fn(right, sort_by));
        let comparison = if *sort_dir == SortDirection::Desc {
            comparison.reverse()
        } else {
            comparison
        };
        comparison.then_with(|| left.id().cmp(&right.id()))
    });
    vec_to_page_by(items, page, sort_by, sort_dir, |item| key_fn(item, sort_by))
}

pub(in crate::storage::postgres) fn rows_to_page_by<T: HasId>(
    items: Vec<T>,
    page: &PageRequest,
    total: u64,
    sort_by: &str,
    sort_dir: &SortDirection,
    key_fn: impl Fn(&T) -> String,
) -> Result<Page<T>, AppError> {
    let mut result = vec_to_page_by(items, page, sort_by, sort_dir, key_fn)?;
    result.total = total;
    Ok(result)
}

pub(in crate::storage::postgres) fn limited_rows_to_page_by<T: HasId>(
    items: Vec<T>,
    page: &PageRequest,
    total: u64,
    sort_by: &str,
    sort_dir: &SortDirection,
    key_fn: impl Fn(&T) -> String,
) -> Result<Page<T>, AppError> {
    page_from_start(items, page, total, 0, sort_by, sort_dir, key_fn)
}

fn page_from_start<T: HasId>(
    items: Vec<T>,
    page: &PageRequest,
    total: u64,
    start: usize,
    sort_by: &str,
    sort_dir: &SortDirection,
    key_fn: impl Fn(&T) -> String,
) -> Result<Page<T>, AppError> {
    let limit = page.limit() as usize;
    let take_count = limit.saturating_add(1);
    let mut page_items = items
        .into_iter()
        .skip(start)
        .take(take_count)
        .collect::<Vec<_>>();
    let has_more = page_items.len() > limit;
    if has_more {
        page_items.pop();
    }
    let next_cursor = if has_more {
        page_items
            .last()
            .map(|item| encode_cursor(sort_by, sort_dir, key_fn(item), item.id()))
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
