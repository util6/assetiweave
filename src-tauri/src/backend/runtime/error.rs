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
    Extension(String),
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

    pub(crate) fn code(&self) -> String {
        match self {
            Self::Validation(_) => "validation_error".to_string(),
            Self::NotFound(_) => "not_found".to_string(),
            Self::Conflict(_) => "conflict".to_string(),
            Self::Io(_) | Self::Db(_) | Self::Storage(_) => "storage_error".to_string(),
            Self::Extension(_) => "extension_error".to_string(),
            Self::Canceled(_) | Self::Cancelled(_) => "cancelled".to_string(),
            Self::Timeout(_) => "timeout".to_string(),
            Self::Process(_) => "process_error".to_string(),
            Self::External(_) => "external_error".to_string(),
            Self::Domain { code, .. } => code.clone(),
        }
    }

    pub(crate) fn view(&self) -> AppErrorView {
        AppErrorView {
            code: self.code(),
            message: self.to_string(),
            retryable: self.retryable(),
            details: self.details().cloned(),
        }
    }

    pub(crate) fn retryable(&self) -> bool {
        match self {
            Self::Validation(_) | Self::NotFound(_) => false,
            Self::Conflict(_)
            | Self::Io(_)
            | Self::Db(_)
            | Self::Extension(_)
            | Self::Canceled(_)
            | Self::Cancelled(_)
            | Self::Timeout(_)
            | Self::Storage(_)
            | Self::Process(_)
            | Self::External(_) => true,
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

impl From<AppError> for String {
    fn from(error: AppError) -> Self {
        error.to_string()
    }
}

/// Compatibility conversion for infrastructure helpers that still expose a
/// plain message. New internal APIs should return `AppError` directly.
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

impl From<AppError> for AppErrorView {
    fn from(error: AppError) -> Self {
        error.view()
    }
}

impl From<&AppError> for AppErrorView {
    fn from(error: &AppError) -> Self {
        error.view()
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
    fn external_helper_preserves_explicit_boundary_mapping() {
        let error = AppError::external("plain failure");

        assert_eq!(error.code(), "external_error");
        assert!(error.retryable());
    }
}
