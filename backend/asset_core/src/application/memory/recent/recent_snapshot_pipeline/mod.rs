pub mod candidates;
pub mod commit;
pub mod commit_helpers;
pub mod conflict;
pub mod execution;
pub mod fingerprint;
pub mod generation;
pub mod json_repair;
pub mod preparation;
pub mod types;
pub mod validation;

pub use candidates::*;
pub use commit::*;
pub use commit_helpers::*;
pub use conflict::*;
pub use execution::*;
pub use fingerprint::*;
pub use generation::*;
pub use json_repair::*;
pub use preparation::*;
pub use types::*;
pub use validation::*;

use crate::backend::application::memory::recent::recent::resolve_project_directory;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::{
    application::{AppError, AppResult},
    domain::{
        CandidateSession, CandidateSessionSummary, ContinuableMemoryItemView, L2ProjectMemoryView,
        L3MemoryItemView, MemoryGenerationResult, MemoryItemCategory, MemoryItemStatus,
        MemoryJobPurpose, MemoryPromotionNomination, MemorySkillBinding, MemoryWindow,
        MemoryWorkOrder, MemoryWorkOrderScope, RecentSnapshotSessionEvidence,
        RecentSnapshotWorkOrderEvidencePack, RecentSnapshotWorkOrderPayload, ResolvedEvidenceRef,
        SessionMemory, SessionMemorySourceReference, ALLOWED_MEMORY_GENERATION_TOOLS,
    },
    infrastructure::agent_execution::{
        execute_agent, AgentSessionMode, AiExecutionCancellation, AiExecutionLimits,
        AiExecutionProgressSink, AiExecutionPurpose, AiExecutionRequest,
    },
    store,
};
use chrono::{DateTime, Duration, Offset, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{QueryBuilder, Row, Sqlite};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

#[cfg(test)]
#[path = "../recent_snapshot_pipeline_tests.rs"]
mod tests;
