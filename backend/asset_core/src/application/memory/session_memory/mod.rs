pub mod json_repair;
pub mod prompt;
pub mod reconciler;
pub mod scheduler;
pub mod types;
pub mod validation;
pub mod worker;

pub use json_repair::*;
pub use prompt::*;
pub use reconciler::*;
pub use scheduler::*;
pub use types::*;
pub use validation::*;
pub use worker::*;

use crate::backend::application::service::AppService;
use crate::backend::{
    application::memory::memory_agent_session::{
        ActiveMemoryAgentSession, MemoryAgentSessionParams,
    },
    application::{AppError, AppResult},
    domain::memory::evidence::{
        build_bounded_evidence_initial_pack, BoundedEvidenceInitialPack, BoundedEvidenceNode,
        EvidenceNodeKind, EvidenceReadStatus, ShortEvidenceRef,
    },
    domain::{
        AgentSessionRef, BoundedMemoryBudgetPolicy, ConversationContentNodeLocator,
        ConversationPartRole, ConversationSessionDetail, MemoryExecutionWorkOrder, MemoryRecipe,
        NormalizedConversationPart, NormalizedConversationSession, NormalizedConversationTurn,
        RecentMemoryEventCategory, SessionMemory, SessionMemoryJob, SessionMemoryJobStatus,
    },
    infrastructure::agent_execution::{
        execute_agent, AgentSessionMode, AiExecutionCancellation, AiExecutionLimits,
        AiExecutionProgressSink, AiExecutionPurpose, AiExecutionRequest, SessionCleanupStatus,
    },
    infrastructure::tasks::tasks::{
        StageStatus, TaskCapabilities, TaskContext, TaskFailure, TaskOutcome,
    },
    store::{
        self, RecentMemoryEventInput, SessionMemoryPersistInput, SessionMemoryReferenceInput,
        SESSION_MEMORY_CONTRACT_VERSION, SESSION_MEMORY_PROMPT_VERSION,
    },
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    time::Duration as StdDuration,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[cfg(test)]
#[path = "../session_memory_tests.rs"]
mod tests;
