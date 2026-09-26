#[derive(Debug, thiserror::Error)]
pub(crate) enum InfraError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("Process error: {0}")]
    Process(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Cancelled: {0}")]
    Cancelled(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("External error: {0}")]
    External(String),
    #[error("Extension error: {0}")]
    Extension(#[from] crate::backend::infrastructure::extensions::ExtensionError),
}

impl InfraError {
    pub(crate) fn external(error: impl std::fmt::Display) -> Self {
        Self::External(error.to_string())
    }

    pub(crate) fn contains(&self, pat: &str) -> bool {
        self.to_string().contains(pat)
    }

    pub(crate) fn code(&self) -> String {
        self.view().code
    }

    pub(crate) fn retryable(&self) -> bool {
        self.view().retryable
    }

    pub(crate) fn view(&self) -> crate::backend::domain::AppErrorView {
        match self {
            Self::Extension(err) => crate::backend::domain::AppErrorView {
                code: err.code().to_string(),
                message: err.public_message(),
                retryable: err.retryable(),
                details: err.details(),
            },
            _ => {
                let (code, message) = match self {
                    Self::Io(_) | Self::Db(_) | Self::Storage(_) => (
                        "storage_error",
                        "The application could not access local storage.".to_string(),
                    ),
                    Self::Process(_) => {
                        ("process_error", "The external process failed.".to_string())
                    }
                    Self::NotFound(msg) => ("not_found", msg.clone()),
                    Self::Validation(msg) => ("validation_error", msg.clone()),
                    Self::Cancelled(_) => ("cancelled", "The operation was cancelled.".to_string()),
                    Self::Conflict(msg) => ("conflict", msg.clone()),
                    Self::Timeout(_) => ("timeout", "The operation timed out.".to_string()),
                    Self::External(_) => (
                        "external_error",
                        "An external operation failed.".to_string(),
                    ),
                    Self::Extension(_) => unreachable!(),
                };
                crate::backend::domain::AppErrorView {
                    code: code.to_string(),
                    message,
                    retryable: !matches!(self, Self::Validation(_) | Self::NotFound(_)),
                    details: None,
                }
            }
        }
    }
}

impl From<InfraError> for crate::backend::domain::AppErrorView {
    fn from(error: InfraError) -> Self {
        error.view()
    }
}

impl From<crate::backend::store::error::StoreError> for InfraError {
    fn from(error: crate::backend::store::error::StoreError) -> Self {
        match error {
            crate::backend::store::error::StoreError::Db(err) => InfraError::Db(err),
            crate::backend::store::error::StoreError::NotFound(msg) => InfraError::NotFound(msg),
            crate::backend::store::error::StoreError::Conflict(msg) => InfraError::Conflict(msg),
            crate::backend::store::error::StoreError::Validation(msg) => {
                InfraError::Validation(msg)
            }
            crate::backend::store::error::StoreError::Storage(msg) => InfraError::Storage(msg),
            crate::backend::store::error::StoreError::Cancelled(msg) => InfraError::Cancelled(msg),
            crate::backend::store::error::StoreError::External(msg) => InfraError::External(msg),
            crate::backend::store::error::StoreError::Codec(err) => {
                InfraError::Storage(err.to_string())
            }
            crate::backend::store::error::StoreError::Projection(err) => {
                InfraError::Storage(err.to_string())
            }
        }
    }
}

pub(crate) type InfraResult<T> = Result<T, InfraError>;
