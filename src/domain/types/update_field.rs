use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Represents a field in an update command that can be left unchanged, cleared, or set to a value.
///
/// This replaces `Option<Option<T>>` throughout the codebase with explicit semantics:
/// - `Unchanged` — the field was not present in the request; keep the existing value
/// - `Clear` — the field was explicitly set to `null`; remove/clear the value
/// - `Set(T)` — the field was set to a new value
///
/// JSON mapping (compatible with serde `Option<Option<T>>` wire format):
/// - absent key → `Unchanged` (via `#[serde(default)]`)
/// - `"field": null` → `Clear`
/// - `"field": value` → `Set(value)`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum UpdateField<T> {
    #[default]
    Unchanged,
    Clear,
    Set(T),
}

impl<T> UpdateField<T> {
    /// Returns `true` if the field should be modified (cleared or set).
    pub fn is_changed(&self) -> bool {
        !matches!(self, UpdateField::Unchanged)
    }

    /// Returns `true` if the field was not included in the update.
    pub fn is_unchanged(&self) -> bool {
        matches!(self, UpdateField::Unchanged)
    }

    /// Resolve the update against an existing value.
    ///
    /// - `Unchanged` → returns the fallback value
    /// - `Clear` → returns `None`
    /// - `Set(v)` → returns `Some(v)` (consuming the value)
    pub fn resolve(self, existing: Option<T>) -> Option<T> {
        match self {
            UpdateField::Unchanged => existing,
            UpdateField::Clear => None,
            UpdateField::Set(v) => Some(v),
        }
    }

    /// Like `resolve`, but maps the inner value through a function before returning.
    pub fn resolve_with<U>(self, existing: Option<U>, f: impl FnOnce(T) -> U) -> Option<U> {
        match self {
            UpdateField::Unchanged => existing,
            UpdateField::Clear => None,
            UpdateField::Set(v) => Some(f(v)),
        }
    }

    /// Map the inner `Set` value, preserving `Unchanged` and `Clear`.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> UpdateField<U> {
        match self {
            UpdateField::Unchanged => UpdateField::Unchanged,
            UpdateField::Clear => UpdateField::Clear,
            UpdateField::Set(v) => UpdateField::Set(f(v)),
        }
    }

    /// Map the inner `Set` value with a fallible function.
    pub fn try_map<U, E>(self, f: impl FnOnce(T) -> Result<U, E>) -> Result<UpdateField<U>, E> {
        match self {
            UpdateField::Unchanged => Ok(UpdateField::Unchanged),
            UpdateField::Clear => Ok(UpdateField::Clear),
            UpdateField::Set(v) => f(v).map(UpdateField::Set),
        }
    }

    /// Extract the `Set` value, if present.
    pub fn into_set(self) -> Option<T> {
        match self {
            UpdateField::Set(v) => Some(v),
            _ => None,
        }
    }

    /// Convert to the legacy `Option<Option<T>>` representation.
    pub fn into_option_option(self) -> Option<Option<T>> {
        match self {
            UpdateField::Unchanged => None,
            UpdateField::Clear => Some(None),
            UpdateField::Set(v) => Some(Some(v)),
        }
    }

    /// Convert from the legacy `Option<Option<T>>` representation.
    pub fn from_option_option(value: Option<Option<T>>) -> Self {
        match value {
            None => UpdateField::Unchanged,
            Some(None) => UpdateField::Clear,
            Some(Some(v)) => UpdateField::Set(v),
        }
    }
}

impl<T: Copy> Copy for UpdateField<T> {}

// Serialize: Unchanged should never appear in output (handled by
// `#[serde(skip_serializing_if = "UpdateField::is_unchanged")]`),
// Clear serializes as null, Set serializes as the value.
impl<T: Serialize> Serialize for UpdateField<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            UpdateField::Unchanged => serializer.serialize_none(),
            UpdateField::Clear => serializer.serialize_none(),
            UpdateField::Set(v) => v.serialize(serializer),
        }
    }
}

// Deserialize: null → Clear, value → Set(value).
// `Unchanged` is produced by `#[serde(default)]` when the key is absent.
impl<'de, T: Deserialize<'de>> Deserialize<'de> for UpdateField<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Deserialize as Option<T>: null → None, value → Some(value)
        let opt = Option::<T>::deserialize(deserializer)?;
        match opt {
            None => Ok(UpdateField::Clear),
            Some(v) => Ok(UpdateField::Set(v)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UpdateField;

    #[test]
    fn default_is_unchanged() {
        let field: UpdateField<i32> = UpdateField::default();
        assert!(field.is_unchanged());
        assert!(!field.is_changed());
    }

    #[test]
    fn resolve_unchanged_returns_existing() {
        let field: UpdateField<i32> = UpdateField::Unchanged;
        assert_eq!(field.resolve(Some(42)), Some(42));
    }

    #[test]
    fn resolve_clear_returns_none() {
        let field: UpdateField<i32> = UpdateField::Clear;
        assert_eq!(field.resolve(Some(42)), None);
    }

    #[test]
    fn resolve_set_returns_new_value() {
        let field = UpdateField::Set(99);
        assert_eq!(field.resolve(Some(42)), Some(99));
    }

    #[test]
    fn json_roundtrip() {
        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
        struct Patch {
            #[serde(default)]
            name: UpdateField<String>,
            #[serde(default)]
            ttl: UpdateField<u32>,
        }

        // Absent keys → Unchanged
        let json = r#"{}"#;
        let patch: Patch = serde_json::from_str(json).unwrap();
        assert_eq!(patch.name, UpdateField::Unchanged);
        assert_eq!(patch.ttl, UpdateField::Unchanged);

        // Null → Clear
        let json = r#"{"name": null, "ttl": null}"#;
        let patch: Patch = serde_json::from_str(json).unwrap();
        assert_eq!(patch.name, UpdateField::Clear);
        assert_eq!(patch.ttl, UpdateField::Clear);

        // Values → Set
        let json = r#"{"name": "hello", "ttl": 300}"#;
        let patch: Patch = serde_json::from_str(json).unwrap();
        assert_eq!(patch.name, UpdateField::Set("hello".to_string()));
        assert_eq!(patch.ttl, UpdateField::Set(300));
    }

    #[test]
    fn option_option_roundtrip() {
        assert_eq!(
            UpdateField::<i32>::from_option_option(None),
            UpdateField::Unchanged
        );
        assert_eq!(
            UpdateField::<i32>::from_option_option(Some(None)),
            UpdateField::Clear
        );
        assert_eq!(
            UpdateField::from_option_option(Some(Some(42))),
            UpdateField::Set(42)
        );

        assert_eq!(UpdateField::<i32>::Unchanged.into_option_option(), None);
        assert_eq!(UpdateField::<i32>::Clear.into_option_option(), Some(None));
        assert_eq!(UpdateField::Set(42).into_option_option(), Some(Some(42)));
    }

    #[test]
    fn try_map_propagates_error() {
        let field = UpdateField::Set("bad");
        let result: Result<UpdateField<i32>, _> = field.try_map(|s| s.parse::<i32>());
        assert!(result.is_err());

        let field = UpdateField::Set("42");
        let result: Result<UpdateField<i32>, _> = field.try_map(|s| s.parse::<i32>());
        assert_eq!(result.unwrap(), UpdateField::Set(42));

        let field: UpdateField<&str> = UpdateField::Unchanged;
        let result: Result<UpdateField<i32>, std::num::ParseIntError> =
            field.try_map(|s| s.parse::<i32>());
        assert_eq!(result.unwrap(), UpdateField::Unchanged);
    }
}
