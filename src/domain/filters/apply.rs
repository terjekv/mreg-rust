use chrono::{DateTime, Utc};

use super::operators::{FilterCondition, FilterOp};

// ─── Apply helpers (memory backend) ─────────────────────────────────

/// Apply a string filter condition. Returns true if the condition matches.
pub(super) fn apply_string_filter(haystack: &str, cond: &FilterCondition) -> bool {
    let val = &cond.value;
    match cond.op {
        FilterOp::Equals => haystack == val.as_str(),
        FilterOp::IEquals => haystack.eq_ignore_ascii_case(val),
        FilterOp::Contains => haystack.contains(val.as_str()),
        FilterOp::IContains => haystack
            .to_ascii_lowercase()
            .contains(&val.to_ascii_lowercase()),
        FilterOp::StartsWith => haystack.starts_with(val.as_str()),
        FilterOp::IStartsWith => haystack
            .to_ascii_lowercase()
            .starts_with(&val.to_ascii_lowercase()),
        FilterOp::EndsWith => haystack.ends_with(val.as_str()),
        FilterOp::IEndsWith => haystack
            .to_ascii_lowercase()
            .ends_with(&val.to_ascii_lowercase()),
        FilterOp::In => val.split(',').any(|v| v.trim() == haystack),
        FilterOp::IsNull => false,
        FilterOp::NotEquals => haystack != val.as_str(),
        FilterOp::NotIEquals => !haystack.eq_ignore_ascii_case(val),
        FilterOp::NotContains => !haystack.contains(val.as_str()),
        FilterOp::NotIContains => !haystack
            .to_ascii_lowercase()
            .contains(&val.to_ascii_lowercase()),
        FilterOp::NotStartsWith => !haystack.starts_with(val.as_str()),
        FilterOp::NotIStartsWith => !haystack
            .to_ascii_lowercase()
            .starts_with(&val.to_ascii_lowercase()),
        FilterOp::NotEndsWith => !haystack.ends_with(val.as_str()),
        FilterOp::NotIEndsWith => !haystack
            .to_ascii_lowercase()
            .ends_with(&val.to_ascii_lowercase()),
        FilterOp::NotIn => !val.split(',').any(|v| v.trim() == haystack),
        FilterOp::NotIsNull => true,
        FilterOp::Gt => haystack > val.as_str(),
        FilterOp::Gte => haystack >= val.as_str(),
        FilterOp::Lt => haystack < val.as_str(),
        FilterOp::Lte => haystack <= val.as_str(),
        FilterOp::NotGt => haystack <= val.as_str(),
        FilterOp::NotGte => haystack < val.as_str(),
        FilterOp::NotLt => haystack >= val.as_str(),
        FilterOp::NotLte => haystack > val.as_str(),
    }
}

/// Apply a filter on an optional string field. Handles is_null correctly.
pub(super) fn apply_optional_string_filter(haystack: Option<&str>, cond: &FilterCondition) -> bool {
    match cond.op {
        FilterOp::IsNull => haystack.is_none(),
        FilterOp::NotIsNull => haystack.is_some(),
        _ => haystack.is_some_and(|h| apply_string_filter(h, cond)),
    }
}

/// Apply a datetime filter by parsing the condition value as RFC 3339.
pub(super) fn apply_datetime_filter(value: DateTime<Utc>, cond: &FilterCondition) -> bool {
    let Ok(target) = DateTime::parse_from_rfc3339(&cond.value) else {
        return false;
    };
    let target = target.with_timezone(&Utc);
    match cond.op {
        FilterOp::Equals => value == target,
        FilterOp::Gt => value > target,
        FilterOp::Gte => value >= target,
        FilterOp::Lt => value < target,
        FilterOp::Lte => value <= target,
        FilterOp::NotEquals => value != target,
        FilterOp::NotGt => value <= target,
        FilterOp::NotGte => value < target,
        FilterOp::NotLt => value >= target,
        FilterOp::NotLte => value > target,
        _ => false,
    }
}

/// Apply a numeric (u32) filter by parsing the condition value.
pub(super) fn apply_u32_filter(value: u32, cond: &FilterCondition) -> bool {
    let Ok(target) = cond.value.parse::<u32>() else {
        return false;
    };
    match cond.op {
        FilterOp::Equals => value == target,
        FilterOp::Gt => value > target,
        FilterOp::Gte => value >= target,
        FilterOp::Lt => value < target,
        FilterOp::Lte => value <= target,
        FilterOp::In => cond
            .value
            .split(',')
            .filter_map(|v| v.trim().parse::<u32>().ok())
            .any(|v| v == value),
        FilterOp::NotEquals => value != target,
        FilterOp::NotGt => value <= target,
        FilterOp::NotGte => value < target,
        FilterOp::NotLt => value >= target,
        FilterOp::NotLte => value > target,
        FilterOp::NotIn => !cond
            .value
            .split(',')
            .filter_map(|v| v.trim().parse::<u32>().ok())
            .any(|v| v == value),
        _ => false,
    }
}
