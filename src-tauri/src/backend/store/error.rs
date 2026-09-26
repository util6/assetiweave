use crate::backend::store::CodecError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum StoreError {
    #[error("Database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("Codec error: {0}")]
    Codec(#[from] CodecError),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Projection error: {0}")]
    Projection(#[from] crate::backend::domain::conversations::projection::ProjectionError),
    #[error("Operation cancelled: {0}")]
    Cancelled(String),
    #[error("External error: {0}")]
    External(String),
}

impl StoreError {
    pub(crate) fn external(error: impl std::fmt::Display) -> Self {
        Self::External(error.to_string())
    }

    pub(crate) fn code(&self) -> String {
        self.view().code
    }

    pub(crate) fn retryable(&self) -> bool {
        self.view().retryable
    }

    pub(crate) fn contains(&self, pat: &str) -> bool {
        self.to_string().contains(pat)
    }

    pub(crate) fn view(&self) -> crate::backend::domain::AppErrorView {
        let (code, message) = match self {
            Self::Db(_) | Self::Storage(_) | Self::Codec(_) | Self::Projection(_) => (
                "storage_error",
                "The application could not access local storage.".to_string(),
            ),
            Self::NotFound(msg) => ("not_found", msg.clone()),
            Self::Conflict(msg) => ("conflict", msg.clone()),
            Self::Validation(msg) => ("validation_error", msg.clone()),
            Self::Cancelled(_) => ("cancelled", "The operation was cancelled.".to_string()),
            Self::External(_) => (
                "external_error",
                "An external operation failed.".to_string(),
            ),
        };
        crate::backend::domain::AppErrorView {
            code: code.to_string(),
            message,
            retryable: !matches!(self, Self::Validation(_) | Self::NotFound(_)),
            details: None,
        }
    }
}

impl From<StoreError> for crate::backend::domain::AppErrorView {
    fn from(error: StoreError) -> Self {
        error.view()
    }
}

pub(crate) type StoreResult<T> = Result<T, StoreError>;
