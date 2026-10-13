use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::memory::evidence::{BoundedEvidenceInitialPack, ShortEvidenceRef};
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

pub const SESSION_MEMORY_ACTION: &str = "memory.extraction";
pub const MAX_EVIDENCE_ITEMS: usize = 512;
pub const MAX_OUTPUT_ITEMS: usize = 64;
pub const MAX_ITEM_LENGTH: usize = 4000;
pub const MAX_AGENT_OUTPUT_LENGTH: usize = 200_000;
pub const MAX_SESSION_MEMORY_CONCURRENCY: usize = 4;

pub fn is_error_retryable(error: &AppError) -> bool {
    match error {
        AppError::Validation(_) => false,
        AppError::Domain {
            retryable, code, ..
        } => {
            if !*retryable {
                return false;
            }
            !matches!(
                code.as_str(),
                "agent_not_found"
                    | "tool_use_denied"
                    | "model_not_found"
                    | "protocol_unsupported"
                    | "config_invalid"
            )
        }
        AppError::Cancelled(_) => false,
        _ => true,
    }
}

pub struct SessionMemoryAgentExecutionResult {
    pub raw_text: String,
    pub short_refs: std::collections::HashMap<String, ShortEvidenceRef>,
    pub pack: BoundedEvidenceInitialPack,
    pub session_cleanup: SessionCleanupStatus,
    pub is_empty: bool,
}

pub fn sanitize_memory_failure(
    code: &str,
    raw_message: &str,
    stage: &str,
    retryable: bool,
) -> TaskFailure {
    let sanitized_code = if code.is_empty() {
        "memory_execution_failed".to_string()
    } else {
        code.chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .take(64)
            .collect::<String>()
    };
    let safe_message = if raw_message.contains("prompt")
        || raw_message.contains("bearer")
        || raw_message.contains("token")
        || raw_message.contains("secret")
    {
        "执行过程中发生受控错误".to_string()
    } else {
        raw_message.chars().take(200).collect::<String>()
    };
    TaskFailure {
        code: sanitized_code,
        message: safe_message,
        stage: stage.to_string(),
        identity: None,
        retryable,
        path: None,
        timestamp: Utc::now().to_rfc3339(),
    }
}

use crate::backend::application::memory::memory_agent_session::{
    ActiveMemoryAgentSession, MemoryAgentSessionParams,
};

pub struct SessionMemoryLeaseGuard;

impl SessionMemoryLeaseGuard {
    pub fn start(
        _database: crate::backend::store::Database,
        _tenant_id: String,
        _job_id: String,
        _ownership_token: String,
        _task_cancellation: CancellationToken,
    ) -> Self {
        Self
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SessionMemoryAgentOutput {
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub result: String,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub verification: Vec<String>,
    #[serde(default)]
    pub blockers: Vec<String>,
    #[serde(default, alias = "followUp")]
    pub follow_up: Vec<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default, alias = "sourceReferences")]
    pub source_references: Vec<AgentSourceReference>,
    #[serde(default, alias = "recentEvents")]
    pub events: Vec<AgentRecentEvent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentSourceReference {
    #[serde(alias = "referenceKey", alias = "sourceReference")]
    pub reference_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentRecentEvent {
    pub category: String,
    pub title: String,
    pub summary: String,
    #[serde(default, alias = "occurredAt")]
    pub occurred_at: Option<String>,
    #[serde(default, alias = "sourceReference")]
    pub source_reference: Option<String>,
    #[serde(default)]
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EvidenceReference {
    pub key: String,
    pub locator: ConversationContentNodeLocator,
    pub node_id: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptEvidence<'a> {
    pub reference_key: &'a str,
    pub locator: &'a ConversationContentNodeLocator,
    pub content: &'a str,
}
