use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
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
    Legacy(String),
}

pub(crate) type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct AppErrorView {
    pub(crate) code: String,
    pub(crate) message: String,
}

impl AppError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "validation",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::Io(_) => "io",
            Self::Db(_) => "database",
            Self::Extension(_) => "extension",
            Self::Canceled(_) => "canceled",
            Self::Legacy(_) => "legacy",
        }
    }

    pub(crate) fn view(&self) -> AppErrorView {
        AppErrorView {
            code: self.code().to_string(),
            message: self.to_string(),
        }
    }
}

impl From<AppError> for String {
    fn from(error: AppError) -> Self {
        error.to_string()
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
            Self::Legacy(format!("后台任务异常退出: {error}"))
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
