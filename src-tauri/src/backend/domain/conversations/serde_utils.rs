use serde::{Deserialize, Deserializer};

pub(crate) fn deserialize_optional_metadata_json<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(text))
            }
        }
        other => serde_json::to_string(&other)
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}
