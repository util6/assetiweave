use crate::backend::runtime::AppResult;
use serde::{de::DeserializeOwned, Serialize};

#[derive(Debug, thiserror::Error)]
pub(crate) enum CodecError {
    #[error("failed to encode json: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("failed to decode json: {0}")]
    Decode(#[source] serde_json::Error),
    #[error("schema validation failed: {0}")]
    Validation(String),
}

pub(crate) fn encode_json<T: Serialize>(value: &T) -> Result<String, CodecError> {
    serde_json::to_string(value).map_err(CodecError::Encode)
}

pub(crate) fn decode_json<T: DeserializeOwned>(value: impl AsRef<str>) -> Result<T, CodecError> {
    serde_json::from_str(value.as_ref()).map_err(CodecError::Decode)
}

pub(crate) fn encode_enum<T: Serialize>(value: T) -> Result<String, CodecError> {
    match serde_json::to_value(value).map_err(CodecError::Encode)? {
        serde_json::Value::String(value) => Ok(value),
        _ => Err(CodecError::Validation(
            "enum did not serialize to string".to_string(),
        )),
    }
}

pub(crate) fn encode_optional_enum<T: Serialize>(
    value: Option<T>,
) -> Result<Option<String>, CodecError> {
    value.map(encode_enum).transpose()
}

pub(crate) fn decode_enum<T: DeserializeOwned>(value: impl AsRef<str>) -> Result<T, CodecError> {
    serde_json::from_value(serde_json::Value::String(value.as_ref().to_string()))
        .map_err(CodecError::Decode)
}

pub(crate) fn decode_optional_enum<T: DeserializeOwned>(
    value: Option<String>,
) -> Result<Option<T>, CodecError> {
    value.map(decode_enum).transpose()
}

pub(crate) fn encode_json_app<T: Serialize>(value: &T) -> AppResult<String> {
    Ok(encode_json(value)?)
}

pub(crate) fn decode_json_app<T: DeserializeOwned>(value: impl AsRef<str>) -> AppResult<T> {
    Ok(decode_json(value)?)
}

pub(crate) fn encode_enum_app<T: Serialize>(value: T) -> AppResult<String> {
    Ok(encode_enum(value)?)
}

pub(crate) fn encode_optional_enum_app<T: Serialize>(
    value: Option<T>,
) -> AppResult<Option<String>> {
    Ok(encode_optional_enum(value)?)
}

pub(crate) fn decode_enum_app<T: DeserializeOwned>(value: impl AsRef<str>) -> AppResult<T> {
    Ok(decode_enum(value)?)
}

pub(crate) fn decode_optional_enum_app<T: DeserializeOwned>(
    value: Option<String>,
) -> AppResult<Option<T>> {
    Ok(decode_optional_enum(value)?)
}

#[cfg(test)]
#[path = "codec_tests.rs"]
mod tests;
