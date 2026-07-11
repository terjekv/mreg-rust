use super::operators::{FilterCondition, FilterOp};

#[derive(Clone, Copy)]
pub(super) enum SqlBindType {
    Text,
    Integer,
    Timestamp,
}

// ─── SQL generation helpers ─────────────────────────────────────────

/// Build SQL WHERE clauses from field-to-column mappings, with optional full-text search.
///
/// Each entry in `field_mappings` maps a slice of filter conditions to a SQL column expression.
/// The `search` parameter adds an ILIKE search across the given `search_columns`.
///
/// Returns (clauses, bind_values) ready for a WHERE clause joined with AND.
pub(super) fn build_sql_conditions(
    field_mappings: &[(&[FilterCondition], &str, SqlBindType)],
    search: &Option<String>,
    search_columns: &[&str],
) -> (Vec<String>, Vec<String>) {
    let mut clauses = Vec::new();
    let mut values = Vec::new();
    let mut idx = 1usize;

    for (conditions, column, bind_type) in field_mappings {
        for cond in *conditions {
            let (sql, val, consumed) =
                op_to_sql_typed(&cond.op, column, &cond.value, idx, *bind_type);
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
    op_to_sql_typed(op, column, value, param_idx, SqlBindType::Text)
}

fn op_to_sql_typed(
    op: &FilterOp,
    column: &str,
    value: &str,
    param_idx: usize,
    bind_type: SqlBindType,
) -> (String, Option<String>, bool) {
    let raw = format!("${param_idx}");
    let p = match bind_type {
        SqlBindType::Text => raw.clone(),
        SqlBindType::Integer => format!("{raw}::integer"),
        SqlBindType::Timestamp => format!("{raw}::timestamptz"),
    };
    let array = match bind_type {
        SqlBindType::Text => format!("regexp_split_to_array({raw}, '\\s*,\\s*')"),
        SqlBindType::Integer => {
            format!("regexp_split_to_array({raw}, '\\s*,\\s*')::integer[]")
        }
        SqlBindType::Timestamp => {
            format!("regexp_split_to_array({raw}, '\\s*,\\s*')::timestamptz[]")
        }
    };
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
            format!("{column} = ANY({array})"),
            Some(value.to_string()),
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
            format!("{column} != ALL({array})"),
            Some(value.to_string()),
            true,
        ),
        FilterOp::NotIsNull => (format!("{column} IS NOT NULL"), None, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_bind_parameters_are_cast_for_postgres() {
        let (integer, _, _) =
            op_to_sql_typed(&FilterOp::Gt, "n.vlan", "42", 1, SqlBindType::Integer);
        let (timestamp, _, _) = op_to_sql_typed(
            &FilterOp::Gte,
            "h.updated_at",
            "2026-01-01T00:00:00Z",
            2,
            SqlBindType::Timestamp,
        );
        assert_eq!(integer, "n.vlan > $1::integer");
        assert_eq!(timestamp, "h.updated_at >= $2::timestamptz");
    }

    #[test]
    fn in_filter_uses_split_array_instead_of_array_literal() {
        let (sql, value, _) = op_to_sql_typed(&FilterOp::In, "h.name", "a,b", 1, SqlBindType::Text);
        assert_eq!(sql, "h.name = ANY(regexp_split_to_array($1, '\\s*,\\s*'))");
        assert_eq!(value.as_deref(), Some("a,b"));
    }
}
