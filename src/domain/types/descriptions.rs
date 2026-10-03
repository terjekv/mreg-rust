use serde::{Deserialize, Deserializer, Serialize};

use crate::errors::AppError;

/// A normalized description that cannot be empty or whitespace-only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct RequiredDescription(String);

impl RequiredDescription {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(AppError::validation("description cannot be empty"));
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for RequiredDescription {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::RequiredDescription;

    #[rstest]
    #[case("")]
    #[case(" \t\n")]
    fn rejects_blank_descriptions(#[case] value: &str) {
        assert!(RequiredDescription::new(value).is_err());
    }

    #[rstest]
    #[case("")]
    #[case(" \t\n")]
    fn deserialization_rejects_blank_descriptions(#[case] value: &str) {
        assert!(serde_json::from_value::<RequiredDescription>(serde_json::json!(value)).is_err());
    }

    #[test]
    fn normalizes_descriptions() {
        assert_eq!(
            RequiredDescription::new("  Guest hosts  ")
                .unwrap()
                .as_str(),
            "Guest hosts"
        );
    }
}
