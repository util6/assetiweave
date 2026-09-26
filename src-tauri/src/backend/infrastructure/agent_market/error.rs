use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::backend::domain::agents::{market::catalog::CatalogError, DistributionType, Ownership};
use crate::backend::infrastructure::validation::{sanitize_details, sanitize_public_message};

#[derive(Debug, thiserror::Error)]
pub(crate) enum AgentMarketError {
    #[error("Database error in agent market repository: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Agent catalog error: {0}")]
    Catalog(#[from] CatalogError),

    #[error("Agent catalog validation failed: {message}")]
    CatalogValidation {
        message: String,
        agent_id: Option<String>,
        field: Option<String>,
        details: Option<Value>,
    },

    #[error("Agent installation '{agent_id}' not found")]
    InstallationNotFound { agent_id: String },

    #[error("Agent distribution error [{code}]: {message}")]
    Distribution {
        code: String,
        message: String,
        agent_id: Option<String>,
        distribution_id: Option<String>,
        details: Option<Value>,
    },

    #[error("Agent host process error: {0}")]
    Process(#[from] crate::backend::infrastructure::host_process::HostProcessError),

    #[error("Agent runtime timeout: {message}")]
    Timeout {
        message: String,
        agent_id: Option<String>,
    },

    #[error("Agent lifecycle error [{code}]: {message}")]
    Lifecycle {
        code: String,
        message: String,
        agent_id: Option<String>,
        phase: Option<String>,
        retryable: bool,
        action: Option<String>,
        details: Option<Value>,
    },

    #[error("Agent market IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Agent market serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl From<crate::backend::store::error::StoreError> for AgentMarketError {
    fn from(error: crate::backend::store::error::StoreError) -> Self {
        match error {
            crate::backend::store::error::StoreError::Db(err) => AgentMarketError::Database(err),
            crate::backend::store::error::StoreError::NotFound(agent_id) => {
                AgentMarketError::InstallationNotFound { agent_id }
            }
            other => AgentMarketError::new("storage_error", &other.to_string(), true),
        }
    }
}

impl AgentMarketError {
    pub(crate) fn new(code: &str, message: &str, retryable: bool) -> Self {
        Self::Lifecycle {
            code: code.to_string(),
            message: message.to_string(),
            agent_id: None,
            phase: None,
            retryable,
            action: None,
            details: None,
        }
    }

    pub(crate) fn contains(&self, pat: &str) -> bool {
        self.to_string().contains(pat)
    }

    pub(crate) fn with_details(mut self, details: Option<Value>) -> Self {
        match &mut self {
            Self::Lifecycle { details: d, .. }
            | Self::CatalogValidation { details: d, .. }
            | Self::Distribution { details: d, .. } => {
                *d = details;
            }
            _ => {}
        }
        self
    }

    pub(crate) fn with_agent_id(mut self, agent_id: impl Into<String>) -> Self {
        let id = agent_id.into();
        match &mut self {
            Self::Lifecycle { agent_id: a, .. }
            | Self::CatalogValidation { agent_id: a, .. }
            | Self::Distribution { agent_id: a, .. }
            | Self::Timeout { agent_id: a, .. } => {
                *a = Some(id);
            }
            Self::InstallationNotFound { agent_id: a } => {
                *a = id;
            }
            _ => {}
        }
        self
    }

    pub(crate) fn code(&self) -> String {
        match self {
            Self::Database(_) => "storage_error".to_string(),
            Self::Catalog(_) | Self::CatalogValidation { .. } => "validation_error".to_string(),
            Self::InstallationNotFound { .. } => "agent_not_installed".to_string(),
            Self::Distribution { code, .. } => code.clone(),
            Self::Process(err) => match err {
                crate::backend::infrastructure::host_process::HostProcessError::MissingProgram { .. } => {
                    "not_found".to_string()
                }
                crate::backend::infrastructure::host_process::HostProcessError::Timeout { .. } => {
                    "timeout".to_string()
                }
                crate::backend::infrastructure::host_process::HostProcessError::Cancelled => {
                    "cancelled".to_string()
                }
                crate::backend::infrastructure::host_process::HostProcessError::OutputLimitExceeded { .. } => {
                    "output_limit_exceeded".to_string()
                }
                _ => "process_error".to_string(),
            },
            Self::Timeout { .. } => "timeout".to_string(),
            Self::Lifecycle { code, .. } => code.clone(),
            Self::Io(_) => "storage_error".to_string(),
            Self::Serialization(_) => "storage_error".to_string(),
        }
    }

    pub(crate) fn message(&self) -> String {
        match self {
            Self::Database(_) => "The application could not access local storage.".to_string(),
            Self::Catalog(err) => err.to_string(),
            Self::CatalogValidation { message, .. } => message.clone(),
            Self::InstallationNotFound { agent_id } => {
                format!("Agent installation '{agent_id}' not found")
            }
            Self::Distribution { message, .. } => message.clone(),
            Self::Process(err) => err.to_string(),
            Self::Timeout { message, .. } => message.clone(),
            Self::Lifecycle { message, .. } => message.clone(),
            Self::Io(_) => "The application could not access local storage.".to_string(),
            Self::Serialization(_) => "The application could not access local storage.".to_string(),
        }
    }

    pub(crate) fn retryable(&self) -> bool {
        match self {
            Self::Database(_) | Self::Io(_) | Self::Serialization(_) => true,
            Self::Catalog(_) | Self::CatalogValidation { .. } => false,
            Self::InstallationNotFound { .. } => false,
            Self::Distribution { code, .. } => {
                matches!(
                    code.as_str(),
                    "runtime_missing" | "system_version_incompatible"
                )
            }
            Self::Process(err) => match err {
                crate::backend::infrastructure::host_process::HostProcessError::MissingProgram { .. }
                | crate::backend::infrastructure::host_process::HostProcessError::OutputLimitExceeded { .. } => {
                    false
                }
                _ => true,
            },
            Self::Timeout { .. } => true,
            Self::Lifecycle { retryable, .. } => *retryable,
        }
    }

    pub(crate) fn details(&self) -> Option<&Value> {
        match self {
            Self::CatalogValidation { details, .. } => details.as_ref(),
            Self::Distribution { details, .. } => details.as_ref(),
            Self::Lifecycle { details, .. } => details.as_ref(),
            _ => None,
        }
    }

    pub(crate) fn agent_id(&self) -> Option<&str> {
        match self {
            Self::CatalogValidation { agent_id, .. } => agent_id.as_deref(),
            Self::InstallationNotFound { agent_id } => Some(agent_id.as_str()),
            Self::Distribution { agent_id, .. } => agent_id.as_deref(),
            Self::Timeout { agent_id, .. } => agent_id.as_deref(),
            Self::Lifecycle { agent_id, .. } => agent_id.as_deref(),
            _ => None,
        }
    }

    pub(crate) fn phase(&self) -> Option<&str> {
        match self {
            Self::Lifecycle { phase, .. } => phase.as_deref(),
            _ => None,
        }
    }

    pub(crate) fn action(&self) -> Option<&str> {
        match self {
            Self::Lifecycle { action, .. } => action.as_deref(),
            _ => None,
        }
    }
}

impl PartialEq<String> for AgentMarketError {
    fn eq(&self, other: &String) -> bool {
        self.to_string() == *other || self.code() == *other || self.message() == *other
    }
}

impl PartialEq<&str> for AgentMarketError {
    fn eq(&self, other: &&str) -> bool {
        self.to_string() == *other || self.code() == *other || self.message() == *other
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LifecycleTaskState {
    Queued,
    Running,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
}

impl LifecycleTaskState {
    pub(crate) fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LifecycleTaskPhase {
    Queued,
    Preparing,
    ProbingRuntime,
    Downloading,
    Installing,
    ValidatingIntegrity,
    ValidatingLayout,
    ProbingProtocol,
    ActivatingDatabase,
    ReloadingRegistry,
    CleaningUp,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProgressSnapshot {
    pub(crate) completed_units: u64,
    pub(crate) total_units: Option<u64>,
    pub(crate) downloaded_bytes: Option<u64>,
    pub(crate) total_bytes: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentLifecycleTaskSnapshot {
    pub(crate) id: String,
    pub(crate) agent_id: String,
    pub(crate) action: String,
    pub(crate) state: LifecycleTaskState,
    pub(crate) phase: LifecycleTaskPhase,
    pub(crate) catalog_version: Option<String>,
    pub(crate) agent_version: Option<String>,
    pub(crate) distribution_id: Option<String>,
    pub(crate) distribution_type: Option<DistributionType>,
    pub(crate) ownership: Option<Ownership>,
    pub(crate) progress: ProgressSnapshot,
    pub(crate) cancellable: bool,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<serde_json::Value>,
    pub(crate) error: Option<AgentMarketErrorView>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentMarketErrorView {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) agent_id: Option<String>,
    pub(crate) phase: Option<String>,
    pub(crate) retryable: bool,
    pub(crate) action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) details: Option<Value>,
}

impl From<&AgentMarketError> for AgentMarketErrorView {
    fn from(value: &AgentMarketError) -> Self {
        Self {
            code: value.code(),
            message: sanitize_public_message(&value.message()),
            agent_id: value.agent_id().map(str::to_string),
            phase: value.phase().map(str::to_string),
            retryable: value.retryable(),
            action: value.action().map(str::to_string),
            details: value.details().cloned().as_ref().and_then(sanitize_details),
        }
    }
}
