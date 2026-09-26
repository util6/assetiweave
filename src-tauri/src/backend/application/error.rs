use crate::backend::domain::AppErrorView;
use crate::backend::infrastructure::InfraError;
use crate::backend::store::StoreError;
use serde::{Serialize, Serializer};
use serde_json::Value;
use std::{fmt, io};

#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    AgentDefinition(
        #[from]
        #[source]
        crate::backend::domain::agents::definition::AgentDefinitionError,
    ),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Db(#[from] sqlx::Error),
    #[error("{0}")]
    Codec(#[from] crate::backend::store::CodecError),
    #[error("{0}")]
    Store(StoreError),
    #[error("{0}")]
    Infra(InfraError),
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
    #[allow(dead_code)]
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

impl From<AppError> for AppErrorView {
    fn from(error: AppError) -> Self {
        error.view()
    }
}

pub(crate) type AppResult<T> = Result<T, AppError>;

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
            Self::Validation(_) | Self::AgentDefinition(_) => "validation_error".to_string(),
            Self::NotFound(_) => "not_found".to_string(),
            Self::Conflict(_) => "conflict".to_string(),
            Self::Io(_) | Self::Db(_) | Self::Codec(_) | Self::Storage(_) => {
                "storage_error".to_string()
            }
            Self::Store(store_err) => match store_err {
                StoreError::NotFound(_) => "not_found".to_string(),
                StoreError::Conflict(_) => "conflict".to_string(),
                StoreError::Validation(_) => "validation_error".to_string(),
                StoreError::Cancelled(_) => "cancelled".to_string(),
                StoreError::External(_) => "external_error".to_string(),
                StoreError::Db(_) | StoreError::Codec(_) | StoreError::Storage(_) => {
                    "storage_error".to_string()
                }
                StoreError::Projection(_) => "validation_error".to_string(),
            },
            Self::Infra(infra_err) => match infra_err {
                InfraError::Extension(e) => e.code().to_string(),
                InfraError::NotFound(_) => "not_found".to_string(),
                InfraError::Conflict(_) => "conflict".to_string(),
                InfraError::Validation(_) => "validation_error".to_string(),
                InfraError::Cancelled(_) => "cancelled".to_string(),
                InfraError::Timeout(_) => "timeout".to_string(),
                InfraError::Process(_) => "process_error".to_string(),
                InfraError::External(_) => "external_error".to_string(),
                InfraError::Io(_) | InfraError::Db(_) | InfraError::Storage(_) => {
                    "storage_error".to_string()
                }
            },
            Self::Cancelled(_) => "cancelled".to_string(),
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
            details: self
                .details()
                .as_ref()
                .and_then(crate::backend::infrastructure::validation::sanitize_details),
        }
    }

    fn public_message(&self) -> String {
        use crate::backend::infrastructure::validation::sanitize_public_message;
        match self {
            Self::Io(_) | Self::Db(_) | Self::Codec(_) | Self::Storage(_) => {
                "The application could not access local storage.".to_string()
            }
            Self::Store(store_err) => match store_err {
                StoreError::Db(_) | StoreError::Codec(_) | StoreError::Storage(_) => {
                    "The application could not access local storage.".to_string()
                }
                StoreError::NotFound(message)
                | StoreError::Conflict(message)
                | StoreError::Validation(message) => sanitize_public_message(message),
                StoreError::Cancelled(_) => "The operation was cancelled.".to_string(),
                StoreError::External(_) => "An external operation failed.".to_string(),
                StoreError::Projection(err) => sanitize_public_message(&err.to_string()),
            },
            Self::Infra(infra_err) => match infra_err {
                InfraError::Extension(e) => e.public_message(),
                InfraError::Io(_) | InfraError::Db(_) | InfraError::Storage(_) => {
                    "The application could not access local storage.".to_string()
                }
                InfraError::Process(_) => "The external process failed.".to_string(),
                InfraError::External(_) => "An external operation failed.".to_string(),
                InfraError::Cancelled(_) => "The operation was cancelled.".to_string(),
                InfraError::Timeout(_) => "The operation timed out.".to_string(),
                InfraError::NotFound(message)
                | InfraError::Conflict(message)
                | InfraError::Validation(message) => sanitize_public_message(message),
            },
            Self::Process(_) => "The external process failed.".to_string(),
            Self::External(_) => "An external operation failed.".to_string(),
            Self::Extension(_) => "An extension operation failed.".to_string(),
            Self::Cancelled(_) => "The operation was cancelled.".to_string(),
            Self::Timeout(_) => "The operation timed out.".to_string(),
            Self::Validation(message)
            | Self::NotFound(message)
            | Self::Conflict(message)
            | Self::Domain { message, .. } => sanitize_public_message(message),
            Self::AgentDefinition(error) => sanitize_public_message(&error.to_string()),
        }
    }

    pub(crate) fn retryable(&self) -> bool {
        match self {
            Self::Validation(_) | Self::AgentDefinition(_) | Self::NotFound(_) => false,
            Self::Store(store_err) => match store_err {
                StoreError::NotFound(_) | StoreError::Validation(_) | StoreError::Projection(_) => {
                    false
                }
                _ => true,
            },
            Self::Infra(infra_err) => match infra_err {
                InfraError::Extension(e) => e.retryable(),
                InfraError::NotFound(_) | InfraError::Validation(_) => false,
                _ => true,
            },
            Self::Conflict(_)
            | Self::Io(_)
            | Self::Db(_)
            | Self::Codec(_)
            | Self::Cancelled(_)
            | Self::Timeout(_)
            | Self::Storage(_)
            | Self::Process(_)
            | Self::External(_)
            | Self::Extension(_) => true,
            Self::Domain { retryable, .. } => *retryable,
        }
    }

    fn details(&self) -> Option<Value> {
        match self {
            Self::Domain { details, .. } => details.clone(),
            Self::Infra(InfraError::Extension(e)) => e.details(),
            _ => None,
        }
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
            Self::Cancelled("后台任务已取消".to_string())
        } else {
            Self::External(format!("后台任务异常退出: {error}"))
        }
    }
}

impl From<StoreError> for AppError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::NotFound(m) => Self::NotFound(m),
            StoreError::Conflict(m) => Self::Conflict(m),
            StoreError::Validation(m) => Self::Validation(m),
            StoreError::Storage(m) => Self::Storage(m),
            StoreError::Db(e) => Self::Db(e),
            StoreError::Codec(e) => Self::Codec(e),
            StoreError::External(m) => Self::External(m),
            StoreError::Cancelled(m) => Self::Cancelled(m),
            StoreError::Projection(e) => Self::from(e),
        }
    }
}

impl From<InfraError> for AppError {
    fn from(error: InfraError) -> Self {
        match error {
            InfraError::Io(e) => Self::Io(e),
            InfraError::Db(e) => Self::Db(e),
            InfraError::Process(m) => Self::Process(m),
            InfraError::NotFound(m) => Self::NotFound(m),
            InfraError::Validation(m) => Self::Validation(m),
            InfraError::Storage(m) => Self::Storage(m),
            InfraError::Cancelled(m) => Self::Cancelled(m),
            InfraError::Conflict(m) => Self::Conflict(m),
            InfraError::Timeout(m) => Self::Timeout(m),
            InfraError::External(m) => Self::External(m),
            InfraError::Extension(e) => Self::Infra(InfraError::Extension(e)),
        }
    }
}

impl From<crate::backend::infrastructure::host_process::HostProcessError> for AppError {
    fn from(error: crate::backend::infrastructure::host_process::HostProcessError) -> Self {
        use crate::backend::infrastructure::host_process::HostProcessError;
        match error {
            HostProcessError::MissingProgram { program } => {
                Self::NotFound(format!("executable not found: {}", program.display()))
            }
            HostProcessError::Spawn(reason) | HostProcessError::Output(reason) => {
                Self::Process(reason)
            }
            HostProcessError::Timeout { .. } => {
                Self::Timeout("process execution timed out".to_string())
            }
            HostProcessError::Cancelled => {
                Self::Cancelled("process execution was cancelled".to_string())
            }
            HostProcessError::OutputLimitExceeded { stdout, stderr } => Self::Process(format!(
                "process output limit exceeded (stdout={stdout}, stderr={stderr})"
            )),
            HostProcessError::Cleanup(reason) => Self::Process(reason),
        }
    }
}

impl From<AppErrorView> for AppError {
    fn from(error: AppErrorView) -> Self {
        Self::Domain {
            code: error.code,
            message: error.message,
            retryable: error.retryable,
            details: error.details,
        }
    }
}

impl From<&AppError> for AppErrorView {
    fn from(error: &AppError) -> Self {
        error.view()
    }
}

impl From<crate::backend::domain::conversations::projection::ProjectionError> for AppError {
    fn from(error: crate::backend::domain::conversations::projection::ProjectionError) -> Self {
        use crate::backend::domain::conversations::projection::ProjectionError;
        match error {
            ProjectionError::InvalidPersistedCardJson(source) => {
                Self::Codec(crate::backend::store::CodecError::Decode(source))
            }
            ProjectionError::UnsupportedSchemaVersion { .. }
            | ProjectionError::MissingCardKind
            | ProjectionError::InvalidCardKind { .. }
            | ProjectionError::UndeclaredCardKind { .. }
            | ProjectionError::UnsupportedRenderer { .. }
            | ProjectionError::RendererNotAllowed { .. }
            | ProjectionError::MissingContractVersion { .. }
            | ProjectionError::AmbiguousLegacySemanticRole { .. }
            | ProjectionError::LegacyConflict { .. }
            | ProjectionError::ManifestValidation(_) => Self::Validation(error.to_string()),
            ProjectionError::Other(message) => Self::Validation(message),
        }
    }
}

impl From<crate::backend::infrastructure::logs::LogAccessError> for AppError {
    fn from(error: crate::backend::infrastructure::logs::LogAccessError) -> Self {
        use crate::backend::infrastructure::logs::LogAccessError;
        match error {
            LogAccessError::Io { source, .. } => Self::Io(source),
            LogAccessError::OpenDirectory(source) => Self::Io(source),
            LogAccessError::PathEscape(path) => {
                Self::Validation(format!("非法日志路径访问: {path}"))
            }
            LogAccessError::InvalidLogLevel(level) => {
                Self::Validation(format!("不支持的日志级别: {level}"))
            }
            LogAccessError::FileNotFound(file_name) => {
                Self::NotFound(format!("未找到指定日志文件: {file_name}"))
            }
            LogAccessError::NoAvailableLogFiles => Self::NotFound("未找到可用日志文件".to_string()),
            LogAccessError::RuntimeConfig(message) => Self::External(message),
            LogAccessError::PanicLogFailed(message) => Self::Storage(message),
            LogAccessError::Other(message) => Self::External(message),
        }
    }
}

pub(crate) fn validation_error(errors: validator::ValidationErrors) -> AppError {
    AppError::Validation(
        crate::backend::infrastructure::validation::format_validation_errors(&errors),
    )
}

impl From<crate::backend::infrastructure::extensions::ExtensionError> for AppError {
    fn from(error: crate::backend::infrastructure::extensions::ExtensionError) -> Self {
        Self::Domain {
            code: error.code().to_string(),
            message: error.public_message(),
            retryable: error.retryable(),
            details: error.details(),
        }
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
