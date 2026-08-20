use crate::backend::compat::LegacyResult;
use serde::{de::DeserializeOwned, Serialize};

pub(super) fn encode_json<T: Serialize>(value: &T) -> LegacyResult<String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

pub(super) fn decode_json<T: DeserializeOwned>(value: String) -> LegacyResult<T> {
    serde_json::from_str(&value).map_err(|error| error.to_string())
}

pub(super) fn encode_enum<T: Serialize>(value: T) -> LegacyResult<String> {
    match serde_json::to_value(value).map_err(|error| error.to_string())? {
        serde_json::Value::String(value) => Ok(value),
        _ => Err("enum did not serialize to string".to_string()),
    }
}

pub(super) fn encode_optional_enum<T: Serialize>(value: Option<T>) -> LegacyResult<Option<String>> {
    value.map(encode_enum).transpose()
}

pub(super) fn decode_enum<T: DeserializeOwned>(value: String) -> LegacyResult<T> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(|error| error.to_string())
}

pub(super) fn decode_optional_enum<T: DeserializeOwned>(
    value: Option<String>,
) -> LegacyResult<Option<T>> {
    value.map(decode_enum).transpose()
}
