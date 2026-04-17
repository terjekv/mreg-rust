use crate::errors::AppError;

// ─── Filter operator types ──────────────────────────────────────────

/// Supported filter operators.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilterOp {
    Equals,
    IEquals,
    Contains,
    IContains,
    StartsWith,
    IStartsWith,
    EndsWith,
    IEndsWith,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    IsNull,
    // Negated
    NotEquals,
    NotIEquals,
    NotContains,
    NotIContains,
    NotStartsWith,
    NotIStartsWith,
    NotEndsWith,
    NotIEndsWith,
    NotGt,
    NotGte,
    NotLt,
    NotLte,
    NotIn,
    NotIsNull,
}

/// A single filter condition: operator + raw string value.
#[derive(Clone, Debug)]
pub struct FilterCondition {
    pub op: FilterOp,
    pub value: String,
}

// ─── Parse helpers ──────────────────────────────────────────────────

/// Parse a query parameter key like "name__contains" into ("name", FilterOp::Contains).
/// A bare "name" defaults to FilterOp::Equals.
pub fn parse_filter_key(key: &str) -> Result<(String, FilterOp), AppError> {
    if let Some((field, op_str)) = key.rsplit_once("__") {
        let op = parse_filter_op(op_str)?;
        Ok((field.to_string(), op))
    } else {
        Ok((key.to_string(), FilterOp::Equals))
    }
}

pub(super) fn parse_filter_op(s: &str) -> Result<FilterOp, AppError> {
    if let Some(inner) = s.strip_prefix("not_") {
        let base = parse_base_op(inner)?;
        return Ok(negate_op(base));
    }
    parse_base_op(s)
}

pub(super) fn parse_base_op(s: &str) -> Result<FilterOp, AppError> {
    match s {
        "equals" => Ok(FilterOp::Equals),
        "iequals" => Ok(FilterOp::IEquals),
        "contains" => Ok(FilterOp::Contains),
        "icontains" => Ok(FilterOp::IContains),
        "startswith" => Ok(FilterOp::StartsWith),
        "istartswith" => Ok(FilterOp::IStartsWith),
        "endswith" => Ok(FilterOp::EndsWith),
        "iendswith" => Ok(FilterOp::IEndsWith),
        "gt" => Ok(FilterOp::Gt),
        "gte" => Ok(FilterOp::Gte),
        "lt" => Ok(FilterOp::Lt),
        "lte" => Ok(FilterOp::Lte),
        "in" => Ok(FilterOp::In),
        "is_null" => Ok(FilterOp::IsNull),
        _ => Err(AppError::validation(format!(
            "unknown filter operator: {s}"
        ))),
    }
}

pub(super) fn negate_op(op: FilterOp) -> FilterOp {
    match op {
        FilterOp::Equals => FilterOp::NotEquals,
        FilterOp::IEquals => FilterOp::NotIEquals,
        FilterOp::Contains => FilterOp::NotContains,
        FilterOp::IContains => FilterOp::NotIContains,
        FilterOp::StartsWith => FilterOp::NotStartsWith,
        FilterOp::IStartsWith => FilterOp::NotIStartsWith,
        FilterOp::EndsWith => FilterOp::NotEndsWith,
        FilterOp::IEndsWith => FilterOp::NotIEndsWith,
        FilterOp::Gt => FilterOp::NotGt,
        FilterOp::Gte => FilterOp::NotGte,
        FilterOp::Lt => FilterOp::NotLt,
        FilterOp::Lte => FilterOp::NotLte,
        FilterOp::In => FilterOp::NotIn,
        FilterOp::IsNull => FilterOp::NotIsNull,
        // Already-negated variants: double-negate back
        FilterOp::NotEquals => FilterOp::Equals,
        FilterOp::NotIEquals => FilterOp::IEquals,
        FilterOp::NotContains => FilterOp::Contains,
        FilterOp::NotIContains => FilterOp::IContains,
        FilterOp::NotStartsWith => FilterOp::StartsWith,
        FilterOp::NotIStartsWith => FilterOp::IStartsWith,
        FilterOp::NotEndsWith => FilterOp::EndsWith,
        FilterOp::NotIEndsWith => FilterOp::IEndsWith,
        FilterOp::NotGt => FilterOp::Gt,
        FilterOp::NotGte => FilterOp::Gte,
        FilterOp::NotLt => FilterOp::Lt,
        FilterOp::NotLte => FilterOp::Lte,
        FilterOp::NotIn => FilterOp::In,
        FilterOp::NotIsNull => FilterOp::IsNull,
    }
}

// ─── Field type validation ─────────────────────────────────────────

/// Field types determine which operators are valid.
#[derive(Clone, Copy, Debug)]
pub enum FieldType {
    /// Free-form text: all string operators allowed.
    String,
    /// Date/time: equals, gt, gte, lt, lte, is_null, and their negations.
    DateTime,
    /// Small integer: equals, gt, gte, lt, lte, in, is_null, and their negations.
    Numeric,
    /// Enum-like value (e.g., IP family 4/6): only equals, not_equals, in, is_null.
    Enum,
    /// Network CIDR string: equals, contains, startswith, in, is_null, and negations.
    Cidr,
}

/// Validate that an operator is valid for a field type. Returns an error if not.
pub fn validate_op(field_name: &str, op: &FilterOp, field_type: FieldType) -> Result<(), AppError> {
    let base = base_of(op);
    let allowed = match field_type {
        FieldType::String => return Ok(()), // all ops allowed on strings
        FieldType::DateTime => matches!(
            base,
            FilterOp::Equals
                | FilterOp::Gt
                | FilterOp::Gte
                | FilterOp::Lt
                | FilterOp::Lte
                | FilterOp::IsNull
        ),
        FieldType::Numeric => matches!(
            base,
            FilterOp::Equals
                | FilterOp::Gt
                | FilterOp::Gte
                | FilterOp::Lt
                | FilterOp::Lte
                | FilterOp::In
                | FilterOp::IsNull
        ),
        FieldType::Enum => matches!(base, FilterOp::Equals | FilterOp::In | FilterOp::IsNull),
        FieldType::Cidr => matches!(
            base,
            FilterOp::Equals
                | FilterOp::Contains
                | FilterOp::StartsWith
                | FilterOp::In
                | FilterOp::IsNull
        ),
    };
    if allowed {
        Ok(())
    } else {
        Err(AppError::validation(format!(
            "operator '{}' is not valid for field '{field_name}' (type: {field_type:?})",
            op_name(op)
        )))
    }
}

/// Get the base (non-negated) form of an operator.
pub(super) fn base_of(op: &FilterOp) -> FilterOp {
    match op {
        FilterOp::NotEquals => FilterOp::Equals,
        FilterOp::NotIEquals => FilterOp::IEquals,
        FilterOp::NotContains => FilterOp::Contains,
        FilterOp::NotIContains => FilterOp::IContains,
        FilterOp::NotStartsWith => FilterOp::StartsWith,
        FilterOp::NotIStartsWith => FilterOp::IStartsWith,
        FilterOp::NotEndsWith => FilterOp::EndsWith,
        FilterOp::NotIEndsWith => FilterOp::IEndsWith,
        FilterOp::NotGt => FilterOp::Gt,
        FilterOp::NotGte => FilterOp::Gte,
        FilterOp::NotLt => FilterOp::Lt,
        FilterOp::NotLte => FilterOp::Lte,
        FilterOp::NotIn => FilterOp::In,
        FilterOp::NotIsNull => FilterOp::IsNull,
        other => other.clone(),
    }
}

pub(super) fn op_name(op: &FilterOp) -> &'static str {
    match op {
        FilterOp::Equals => "equals",
        FilterOp::IEquals => "iequals",
        FilterOp::Contains => "contains",
        FilterOp::IContains => "icontains",
        FilterOp::StartsWith => "startswith",
        FilterOp::IStartsWith => "istartswith",
        FilterOp::EndsWith => "endswith",
        FilterOp::IEndsWith => "iendswith",
        FilterOp::Gt => "gt",
        FilterOp::Gte => "gte",
        FilterOp::Lt => "lt",
        FilterOp::Lte => "lte",
        FilterOp::In => "in",
        FilterOp::IsNull => "is_null",
        FilterOp::NotEquals => "not_equals",
        FilterOp::NotIEquals => "not_iequals",
        FilterOp::NotContains => "not_contains",
        FilterOp::NotIContains => "not_icontains",
        FilterOp::NotStartsWith => "not_startswith",
        FilterOp::NotIStartsWith => "not_istartswith",
        FilterOp::NotEndsWith => "not_endswith",
        FilterOp::NotIEndsWith => "not_iendswith",
        FilterOp::NotGt => "not_gt",
        FilterOp::NotGte => "not_gte",
        FilterOp::NotLt => "not_lt",
        FilterOp::NotLte => "not_lte",
        FilterOp::NotIn => "not_in",
        FilterOp::NotIsNull => "not_is_null",
    }
}
