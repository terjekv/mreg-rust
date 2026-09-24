use serde::{Deserialize, Deserializer, Serialize};

use crate::errors::AppError;

/// Canonical identifier used to select a community naming template.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct CommunityTemplatePattern(String);

impl CommunityTemplatePattern {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        let value = value.as_ref().trim();
        if value.is_empty()
            || value.len() > 100
            || !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(AppError::validation(
                "community template pattern must contain 1 to 100 ASCII letters, digits, or underscores",
            ));
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for CommunityTemplatePattern {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::CommunityTemplatePattern;

    #[rstest]
    #[case("")]
    #[case(" ")]
    #[case("bad-pattern")]
    #[case("bad pattern")]
    #[case("ø")]
    #[case(&"a".repeat(101))]
    fn rejects_invalid_patterns(#[case] value: &str) {
        assert!(
            serde_json::from_value::<CommunityTemplatePattern>(serde_json::json!(value)).is_err()
        );
    }

    #[test]
    fn normalizes_before_comparison() {
        assert_eq!(
            CommunityTemplatePattern::new("  campus  ").unwrap(),
            CommunityTemplatePattern::new("campus").unwrap()
        );
    }
}
