use schemars::JsonSchema;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use std::{fmt, io};

#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Db(#[from] sqlx::Error),
    #[error("{0}")]
    Canceled(String),
    #[error("{0}")]
    Cancelled(String),
    #[allow(dead_code)]
    #[error("{0}")]
    Timeout(String),
    #[error("{0}")]
    Storage(String),
    #[error("{0}")]
    Process(String),
    #[error("{0}")]
    External(String),
    #[error("{0}")]
    Extension(String),
    #[error("{message}")]
    Domain {
        code: String,
        message: String,
        retryable: bool,
        details: Option<Value>,
    },
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.view().serialize(serializer)
    }
}

pub(crate) type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireError {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) retryable: bool,
    pub(crate) details: Option<Value>,
}

pub(crate) type AppErrorView = WireError;

impl AppError {
    pub(crate) fn external(error: impl fmt::Display) -> Self {
        Self::External(error.to_string())
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }

    pub(crate) fn code(&self) -> String {
        match self {
            Self::Validation(_) => "validation_error".to_string(),
            Self::NotFound(_) => "not_found".to_string(),
            Self::Conflict(_) => "conflict".to_string(),
            Self::Io(_) | Self::Db(_) | Self::Storage(_) => "storage_error".to_string(),
            Self::Canceled(_) | Self::Cancelled(_) => "cancelled".to_string(),
            Self::Timeout(_) => "timeout".to_string(),
            Self::Process(_) => "process_error".to_string(),
            Self::External(_) => "external_error".to_string(),
            Self::Extension(_) => "extension_error".to_string(),
            Self::Domain { code, .. } => code.clone(),
        }
    }

    pub(crate) fn view(&self) -> AppErrorView {
        AppErrorView {
            code: self.code(),
            message: self.public_message(),
            retryable: self.retryable(),
            details: self.details().and_then(sanitize_details),
        }
    }

    fn public_message(&self) -> String {
        match self {
            Self::Io(_) | Self::Db(_) | Self::Storage(_) => {
                "The application could not access local storage.".to_string()
            }
            Self::Process(_) => "The external process failed.".to_string(),
            Self::External(_) => "An external operation failed.".to_string(),
            Self::Extension(_) => "An extension operation failed.".to_string(),
            Self::Canceled(_) | Self::Cancelled(_) => "The operation was cancelled.".to_string(),
            Self::Timeout(_) => "The operation timed out.".to_string(),
            Self::Validation(message)
            | Self::NotFound(message)
            | Self::Conflict(message)
            | Self::Domain { message, .. } => sanitize_public_message(message),
        }
    }

    pub(crate) fn retryable(&self) -> bool {
        match self {
            Self::Validation(_) | Self::NotFound(_) => false,
            Self::Conflict(_)
            | Self::Io(_)
            | Self::Db(_)
            | Self::Canceled(_)
            | Self::Cancelled(_)
            | Self::Timeout(_)
            | Self::Storage(_)
            | Self::Process(_)
            | Self::External(_)
            | Self::Extension(_) => true,
            Self::Domain { retryable, .. } => *retryable,
        }
    }

    fn details(&self) -> Option<&Value> {
        match self {
            Self::Domain { details, .. } => details.as_ref(),
            _ => None,
        }
    }
}

pub(crate) fn sanitize_public_message(message: &str) -> String {
    let normalized = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "The operation failed.".to_string();
    }
    let lower = normalized.to_ascii_lowercase();
    let contains_absolute_path = normalized.split_whitespace().any(|word| {
        word.starts_with('/')
            || word.starts_with("~/")
            || word.get(1..3).is_some_and(|drive| {
                drive.starts_with(':') && word.as_bytes().get(2) == Some(&b'\\')
            })
    });
    if contains_absolute_path
        || lower.contains("sql")
        || lower.contains("token")
        || lower.contains("authorization")
        || lower.contains("password")
        || lower.contains("prompt=")
        || lower.contains("prompt:")
        || lower.contains("environment")
    {
        return "The operation failed.".to_string();
    }
    normalized.chars().take(500).collect()
}

pub(crate) fn sanitize_details(value: &Value) -> Option<Value> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => Some(value.clone()),
        Value::String(message) => Some(Value::String(
            if sanitize_public_message(message) == "The operation failed."
                && !message.trim().is_empty()
            {
                "<redacted>".to_string()
            } else {
                message.chars().take(500).collect()
            },
        )),
        Value::Array(values) => Some(Value::Array(
            values.iter().filter_map(sanitize_details).collect(),
        )),
        Value::Object(values) => Some(Value::Object(
            values
                .iter()
                .filter_map(|(key, value)| {
                    let lower_key = key.to_ascii_lowercase();
                    if lower_key.contains("token")
                        || lower_key.contains("secret")
                        || lower_key.contains("password")
                        || lower_key.contains("prompt")
                        || lower_key.contains("environment")
                    {
                        return None;
                    }
                    sanitize_details(value).map(|value| (key.clone(), value))
                })
                .collect(),
        )),
    }
}

impl From<io::ErrorKind> for AppError {
    fn from(kind: io::ErrorKind) -> Self {
        Self::Io(io::Error::from(kind))
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(error: tokio::task::JoinError) -> Self {
        if error.is_cancelled() {
            Self::Canceled("后台任务已取消".to_string())
        } else {
            Self::External(format!("后台任务异常退出: {error}"))
        }
    }
}

impl From<WireError> for AppError {
    fn from(error: WireError) -> Self {
        Self::Domain {
            code: error.code,
            message: error.message,
            retryable: error.retryable,
            details: error.details,
        }
    }
}

impl fmt::Display for AppErrorView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_error_preserves_validation_code_across_tauri_view() {
        let view = AppError::Validation("bad input".to_string()).view();

        assert_eq!(view.code, "validation_error");
        assert!(!view.retryable);
        assert_eq!(view.message, "bad input");
    }

    #[test]
    fn external_error_never_exposes_debug_payload() {
        let value = serde_json::to_value(AppError::External("plain failure".to_string()))
            .expect("wire error serializes");
        assert_eq!(value["code"], "external_error");
        assert!(value.get("AppError").is_none());
        assert!(value.get("Legacy").is_none());
    }

    #[test]
    fn infrastructure_wire_error_hides_internal_diagnostics() {
        let view =
            AppError::External("SQL /Users/util6/private.db token=secret".to_string()).view();

        assert_eq!(view.code, "external_error");
        assert_eq!(view.message, "An external operation failed.");
        assert!(!view.message.contains("/Users/util6"));
        assert!(!view.message.contains("secret"));
    }

    #[test]
    fn external_helper_preserves_explicit_boundary_mapping() {
        let error = AppError::external("plain failure");

        assert_eq!(error.code(), "external_error");
        assert!(error.retryable());
    }

    #[test]
    fn domain_wire_error_redacts_sensitive_message_and_details() {
        let error = AppError::Domain {
            code: "fixture_failed".to_string(),
            message: "failed at /Users/util6/private.db token=secret".to_string(),
            retryable: true,
            details: Some(serde_json::json!({
                "path": "/Users/util6/private.db",
                "token": "secret",
                "phase": "prompting",
            })),
        };

        let view = error.view();

        assert_eq!(view.message, "The operation failed.");
        let details = view.details.as_ref().expect("safe details");
        assert!(details.get("token").is_none());
        assert_eq!(details["path"], "<redacted>");
        assert_eq!(details["phase"], "prompting");
        assert!(!serde_json::to_string(&view).unwrap().contains("secret"));
        assert!(!serde_json::to_string(&view)
            .unwrap()
            .contains("/Users/util6"));
    }
}


impl From<String> for AppError {
    fn from(message: String) -> Self {
        Self::External(message)
    }
}

impl From<AppError> for String {
    fn from(error: AppError) -> Self {
        error.to_string()
    }
}
