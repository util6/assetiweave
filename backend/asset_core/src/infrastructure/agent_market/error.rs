use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::backend::domain::agents::{market::catalog::CatalogError, DistributionType, Ownership};
use crate::backend::infrastructure::validation::{sanitize_details, sanitize_public_message};

#[derive(Debug, thiserror::Error)]
pub enum AgentMarketError {
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
    pub fn new(code: &str, message: &str, retryable: bool) -> Self {
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

    pub fn contains(&self, pat: &str) -> bool {
        self.to_string().contains(pat)
    }

    pub fn with_details(mut self, details: Option<Value>) -> Self {
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

    pub fn with_agent_id(mut self, agent_id: impl Into<String>) -> Self {
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

    pub fn code(&self) -> String {
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

    pub fn message(&self) -> String {
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

    pub fn retryable(&self) -> bool {
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

    pub fn details(&self) -> Option<&Value> {
        match self {
            Self::CatalogValidation { details, .. } => details.as_ref(),
            Self::Distribution { details, .. } => details.as_ref(),
            Self::Lifecycle { details, .. } => details.as_ref(),
            _ => None,
        }
    }

    pub fn agent_id(&self) -> Option<&str> {
        match self {
            Self::CatalogValidation { agent_id, .. } => agent_id.as_deref(),
            Self::InstallationNotFound { agent_id } => Some(agent_id.as_str()),
            Self::Distribution { agent_id, .. } => agent_id.as_deref(),
            Self::Timeout { agent_id, .. } => agent_id.as_deref(),
            Self::Lifecycle { agent_id, .. } => agent_id.as_deref(),
            _ => None,
        }
    }

    pub fn phase(&self) -> Option<&str> {
        match self {
            Self::Lifecycle { phase, .. } => phase.as_deref(),
            _ => None,
        }
    }

    pub fn action(&self) -> Option<&str> {
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
pub enum LifecycleTaskState {
    Queued,
    Running,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
}

impl LifecycleTaskState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleTaskPhase {
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
pub struct ProgressSnapshot {
    pub completed_units: u64,
    pub total_units: Option<u64>,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLifecycleTaskSnapshot {
    pub id: String,
    pub agent_id: String,
    pub action: String,
    pub state: LifecycleTaskState,
    pub phase: LifecycleTaskPhase,
    pub catalog_version: Option<String>,
    pub agent_version: Option<String>,
    pub distribution_id: Option<String>,
    pub distribution_type: Option<DistributionType>,
    pub ownership: Option<Ownership>,
    pub progress: ProgressSnapshot,
    pub cancellable: bool,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    pub result: Option<serde_json::Value>,
    pub error: Option<AgentMarketErrorView>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMarketErrorView {
    pub code: String,
    pub message: String,
    pub agent_id: Option<String>,
    pub phase: Option<String>,
    pub retryable: bool,
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
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
