use super::operators::{FilterCondition, FilterOp};

// ─── SQL generation helpers ─────────────────────────────────────────

/// Build SQL WHERE clauses from field-to-column mappings, with optional full-text search.
///
/// Each entry in `field_mappings` maps a slice of filter conditions to a SQL column expression.
/// The `search` parameter adds an ILIKE search across the given `search_columns`.
///
/// Returns (clauses, bind_values) ready for a WHERE clause joined with AND.
pub(super) fn build_sql_conditions(
    field_mappings: &[(&[FilterCondition], &str)],
    search: &Option<String>,
    search_columns: &[&str],
) -> (Vec<String>, Vec<String>) {
    let mut clauses = Vec::new();
    let mut values = Vec::new();
    let mut idx = 1usize;

    for (conditions, column) in field_mappings {
        for cond in *conditions {
            let (sql, val, consumed) = op_to_sql(&cond.op, column, &cond.value, idx);
            clauses.push(sql);
            if let Some(v) = val {
                values.push(v);
            }
            if consumed {
                idx += 1;
            }
        }
    }

    if let Some(needle) = search {
        let p = format!("${idx}");
        let search_clause = search_columns
            .iter()
            .map(|col| format!("{col} ILIKE '%' || {p} || '%'"))
            .collect::<Vec<_>>()
            .join(" OR ");
        clauses.push(format!("({search_clause})"));
        values.push(needle.clone());
    }

    (clauses, values)
}

/// Generate a SQL WHERE clause fragment and optional bind value for a filter condition.
/// `column` is the SQL column expression (e.g., "h.name", "fz.name").
/// `param_idx` is the next $N parameter index.
/// Returns (sql_fragment, optional_bind_value, whether a param was consumed).
pub(super) fn op_to_sql(
    op: &FilterOp,
    column: &str,
    value: &str,
    param_idx: usize,
) -> (String, Option<String>, bool) {
    let p = format!("${param_idx}");
    match op {
        FilterOp::Equals => (format!("{column} = {p}"), Some(value.to_string()), true),
        FilterOp::IEquals => (
            format!("LOWER({column}) = LOWER({p})"),
            Some(value.to_string()),
            true,
        ),
        FilterOp::Contains => (
            format!("{column} LIKE '%' || {p} || '%'"),
            Some(value.to_string()),
            true,
        ),
        FilterOp::IContains => (
            format!("{column} ILIKE '%' || {p} || '%'"),
            Some(value.to_string()),
            true,
        ),
        FilterOp::StartsWith => (
            format!("{column} LIKE {p} || '%'"),
            Some(value.to_string()),
            true,
        ),
        FilterOp::IStartsWith => (
            format!("{column} ILIKE {p} || '%'"),
            Some(value.to_string()),
            true,
        ),
        FilterOp::EndsWith => (
            format!("{column} LIKE '%' || {p}"),
            Some(value.to_string()),
            true,
        ),
        FilterOp::IEndsWith => (
            format!("{column} ILIKE '%' || {p}"),
            Some(value.to_string()),
            true,
        ),
        FilterOp::Gt => (format!("{column} > {p}"), Some(value.to_string()), true),
        FilterOp::Gte => (format!("{column} >= {p}"), Some(value.to_string()), true),
        FilterOp::Lt => (format!("{column} < {p}"), Some(value.to_string()), true),
        FilterOp::Lte => (format!("{column} <= {p}"), Some(value.to_string()), true),
        FilterOp::In => (
            format!("{column} = ANY({p}::text[])"),
            Some(format!("{{{}}}", value)),
            true,
        ),
        FilterOp::IsNull => (format!("{column} IS NULL"), None, false),
        FilterOp::NotEquals => (format!("{column} != {p}"), Some(value.to_string()), true),
        FilterOp::NotIEquals => (
            format!("LOWER({column}) != LOWER({p})"),
            Some(value.to_string()),
            true,
        ),
        FilterOp::NotContains => (
            format!("{column} NOT LIKE '%' || {p} || '%'"),
            Some(value.to_string()),
            true,
        ),
        FilterOp::NotIContains => (
            format!("{column} NOT ILIKE '%' || {p} || '%'"),
            Some(value.to_string()),
            true,
        ),
        FilterOp::NotStartsWith => (
            format!("{column} NOT LIKE {p} || '%'"),
            Some(value.to_string()),
            true,
        ),
        FilterOp::NotIStartsWith => (
            format!("{column} NOT ILIKE {p} || '%'"),
            Some(value.to_string()),
            true,
        ),
        FilterOp::NotEndsWith => (
            format!("{column} NOT LIKE '%' || {p}"),
            Some(value.to_string()),
            true,
        ),
        FilterOp::NotIEndsWith => (
            format!("{column} NOT ILIKE '%' || {p}"),
            Some(value.to_string()),
            true,
        ),
        FilterOp::NotGt => (format!("{column} <= {p}"), Some(value.to_string()), true),
        FilterOp::NotGte => (format!("{column} < {p}"), Some(value.to_string()), true),
        FilterOp::NotLt => (format!("{column} >= {p}"), Some(value.to_string()), true),
        FilterOp::NotLte => (format!("{column} > {p}"), Some(value.to_string()), true),
        FilterOp::NotIn => (
            format!("{column} != ALL({p}::text[])"),
            Some(format!("{{{}}}", value)),
            true,
        ),
        FilterOp::NotIsNull => (format!("{column} IS NOT NULL"), None, false),
    }
}
