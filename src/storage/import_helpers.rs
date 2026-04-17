//! Shared helper functions for import attribute resolution.
//!
//! These pure functions extract values from a `serde_json::Value` attribute map,
//! optionally resolving references via a `BTreeMap<String, String>` refs map.
//! Used by both the memory and postgres import backends.

use std::collections::BTreeMap;

use serde_json::Value;
use uuid::Uuid;

use crate::errors::AppError;

/// Extract a required string attribute, resolving `{key}_ref` references if present.
pub fn resolve_string(
    attributes: &Value,
    key: &str,
    refs: &BTreeMap<String, String>,
) -> Result<String, AppError> {
    resolve_optional_string(attributes, key, refs)?
        .ok_or_else(|| AppError::validation(format!("missing required import attribute '{}'", key)))
}

/// Extract an optional string attribute, resolving `{key}_ref` references if present.
pub fn resolve_optional_string(
    attributes: &Value,
    key: &str,
    refs: &BTreeMap<String, String>,
) -> Result<Option<String>, AppError> {
    let object = attributes
        .as_object()
        .ok_or_else(|| AppError::validation("import item attributes must be a JSON object"))?;
    if let Some(value) = object.get(key) {
        return value
            .as_str()
            .map(|value| Some(value.to_string()))
            .ok_or_else(|| {
                AppError::validation(format!("import attribute '{}' must be a string", key))
            });
    }
    let ref_key = format!("{}_ref", key);
    if let Some(value) = object.get(&ref_key) {
        let reference = value.as_str().ok_or_else(|| {
            AppError::validation(format!("import attribute '{}' must be a string", ref_key))
        })?;
        return refs
            .get(reference)
            .cloned()
            .map(Some)
            .ok_or_else(|| AppError::validation(format!("unknown import ref '{}'", reference)));
    }
    Ok(None)
}

/// Try each key in order, returning the first resolved string value.
pub fn resolve_one_of_string(
    attributes: &Value,
    keys: &[&str],
    refs: &BTreeMap<String, String>,
) -> Result<Option<String>, AppError> {
    for key in keys {
        if let Some(value) = resolve_optional_string(attributes, key, refs)? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

/// Like `resolve_one_of_string`, but returns an error if none of the keys are present.
pub fn resolve_required_one_of_string(
    attributes: &Value,
    keys: &[&str],
    refs: &BTreeMap<String, String>,
) -> Result<String, AppError> {
    resolve_one_of_string(attributes, keys, refs)?.ok_or_else(|| {
        AppError::validation(format!(
            "missing required import attribute '{}'",
            keys.join("' or '")
        ))
    })
}

/// Extract an optional boolean attribute.
pub fn resolve_bool(attributes: &Value, key: &str) -> Result<Option<bool>, AppError> {
    let object = attributes
        .as_object()
        .ok_or_else(|| AppError::validation("import item attributes must be a JSON object"))?;
    match object.get(key) {
        Some(value) => value.as_bool().map(Some).ok_or_else(|| {
            AppError::validation(format!("import attribute '{}' must be a boolean", key))
        }),
        None => Ok(None),
    }
}

/// Extract a string array attribute, resolving each element via refs.
pub fn resolve_string_vec(
    attributes: &Value,
    key: &str,
    refs: &BTreeMap<String, String>,
) -> Result<Vec<String>, AppError> {
    let object = attributes
        .as_object()
        .ok_or_else(|| AppError::validation("import item attributes must be a JSON object"))?;

    let values = match object.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                let value = item.as_str().ok_or_else(|| {
                    AppError::validation(format!(
                        "import attribute '{}' must be an array of strings",
                        key
                    ))
                })?;
                Ok::<String, AppError>(
                    refs.get(value)
                        .cloned()
                        .unwrap_or_else(|| value.to_string()),
                )
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => {
            return Err(AppError::validation(format!(
                "import attribute '{}' must be an array of strings",
                key
            )));
        }
        None => Vec::new(),
    };

    Ok(values
        .into_iter()
        .map(|value| refs.get(&value).cloned().unwrap_or(value))
        .collect())
}

/// Extract an optional unsigned integer attribute.
pub fn resolve_u64(attributes: &Value, key: &str) -> Result<Option<u64>, AppError> {
    let object = attributes
        .as_object()
        .ok_or_else(|| AppError::validation("import item attributes must be a JSON object"))?;
    match object.get(key) {
        Some(value) => value.as_u64().map(Some).ok_or_else(|| {
            AppError::validation(format!("import attribute '{}' must be an integer", key))
        }),
        None => Ok(None),
    }
}

/// Extract an optional UUID attribute, resolving refs.
pub fn resolve_uuid(
    attributes: &Value,
    key: &str,
    refs: &BTreeMap<String, String>,
) -> Result<Option<Uuid>, AppError> {
    resolve_optional_string(attributes, key, refs)?
        .map(|raw| {
            Uuid::parse_str(&raw)
                .map_err(|error| AppError::validation(format!("invalid {key}: {error}")))
        })
        .transpose()
}

/// Like `resolve_u64` but converts to u32, returning a validation error on overflow.
pub fn resolve_u32(attributes: &Value, key: &str) -> Result<Option<u32>, AppError> {
    resolve_u64(attributes, key)?
        .map(|v| {
            u32::try_from(v).map_err(|_| AppError::validation(format!("'{key}' exceeds u32 range")))
        })
        .transpose()
}

/// Like `resolve_u64` but converts to i32, returning a validation error on overflow.
pub fn resolve_i32(attributes: &Value, key: &str) -> Result<Option<i32>, AppError> {
    resolve_u64(attributes, key)?
        .map(|v| {
            i32::try_from(v).map_err(|_| AppError::validation(format!("'{key}' exceeds i32 range")))
        })
        .transpose()
}

/// Convert a JSON value to a string suitable for storing as a ref.
pub fn stringify_ref_value(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{resolve_i32, resolve_u32};
    use crate::errors::AppError;

    #[test]
    fn resolve_u32_rejects_values_above_u32_max() {
        let attributes = json!({
            "reserved": u64::from(u32::MAX) + 1,
        });

        let err = resolve_u32(&attributes, "reserved").expect_err("overflow should fail");

        match err {
            AppError::Validation(message) => assert_eq!(message, "'reserved' exceeds u32 range"),
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn resolve_i32_rejects_values_above_i32_max() {
        let attributes = json!({
            "max_communities": i64::from(i32::MAX) + 1,
        });

        let err = resolve_i32(&attributes, "max_communities").expect_err("overflow should fail");

        match err {
            AppError::Validation(message) => {
                assert_eq!(message, "'max_communities' exceeds i32 range")
            }
            other => panic!("expected validation error, got {other:?}"),
        }
    }
}
