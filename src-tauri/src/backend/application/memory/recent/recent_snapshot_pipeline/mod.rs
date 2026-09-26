pub(crate) mod candidates;
pub(crate) mod commit;
pub(crate) mod commit_helpers;
pub(crate) mod conflict;
pub(crate) mod execution;
pub(crate) mod fingerprint;
pub(crate) mod generation;
pub(crate) mod json_repair;
pub(crate) mod preparation;
pub(crate) mod types;
pub(crate) mod validation;

pub(crate) use candidates::*;
pub(crate) use commit::*;
pub(crate) use commit_helpers::*;
pub(crate) use conflict::*;
pub(crate) use execution::*;
pub(crate) use fingerprint::*;
pub(crate) use generation::*;
pub(crate) use json_repair::*;
pub(crate) use preparation::*;
pub(crate) use types::*;
pub(crate) use validation::*;

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
