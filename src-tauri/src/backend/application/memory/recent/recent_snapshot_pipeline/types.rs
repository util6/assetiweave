use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::memory::work_order::RecentSnapshotWorkOrderEvidencePack;
use crate::backend::domain::memory::{
    L2ProjectMemoryView, L3MemoryItemView, SessionMemory, SessionMemorySourceReference,
};
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row, Sqlite};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub(crate) struct WatermarkTarget {
    pub(crate) target_watermark_utc: DateTime<Utc>,
    pub(crate) local_watermark_date: String,
    pub(crate) local_watermark_time: String,
    pub(crate) timezone_offset_minutes: i64,
    pub(crate) window_hours: i64,
    pub(crate) window_start_utc: DateTime<Utc>,
    pub(crate) window_end_utc: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct RecentSnapshotPreparation {
    pub(crate) target: WatermarkTarget,
    pub(crate) skill_binding: MemorySkillBinding,
    pub(crate) target_fingerprint: String,
    pub(crate) content_fingerprint: String,
    pub(crate) evidence: RecentSnapshotWorkOrderEvidencePack,
    pub(crate) skill_text: String,
}

#[derive(Debug, Default)]
pub(crate) struct RecentSnapshotFrozenContext {
    pub(crate) session_memories: BTreeMap<String, SessionMemory>,
    pub(crate) source_references: BTreeMap<String, Vec<SessionMemorySourceReference>>,
    pub(crate) recent_events: BTreeMap<String, Vec<crate::backend::domain::RecentMemoryEvent>>,
    pub(crate) current_l2_projects: Vec<L2ProjectMemoryView>,
    pub(crate) current_l3_items: Vec<L3MemoryItemView>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TargetFingerprintSessionRef<'a> {
    pub(crate) session_id: &'a str,
    pub(crate) source_revision: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct TargetFingerprintItemRef<'a> {
    pub(crate) item_id: &'a str,
    pub(crate) revision_id: &'a str,
}

#[derive(Debug, Serialize)]
pub(crate) struct TargetFingerprintPayload<'a> {
    pub(crate) budget_policy_version: &'static str,
    pub(crate) candidate_sessions: Vec<TargetFingerprintSessionRef<'a>>,
    pub(crate) contract_version: &'static str,
    pub(crate) prior_active_items: Vec<TargetFingerprintItemRef<'a>>,
    pub(crate) projection_policy_version: &'static str,
    pub(crate) skill_asset_id: Option<&'a str>,
    pub(crate) skill_content_hash: Option<&'a str>,
    pub(crate) skill_revision: i64,
    pub(crate) target_watermark_utc: &'a str,
    pub(crate) tenant_id: &'a str,
    pub(crate) window_hours: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct ContentFingerprintSessionRef<'a> {
    pub(crate) last_activity_at: &'a str,
    pub(crate) session_id: &'a str,
    pub(crate) source_revision: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct ContentFingerprintItemRef<'a> {
    pub(crate) item_id: &'a str,
    pub(crate) remaining_lifetime_bucket: i64,
    pub(crate) revision_id: &'a str,
}

#[derive(Debug, Serialize)]
pub(crate) struct ContentFingerprintPayload<'a> {
    pub(crate) candidate_sessions: Vec<ContentFingerprintSessionRef<'a>>,
    pub(crate) carry_over_items: Vec<ContentFingerprintItemRef<'a>>,
    pub(crate) contract_version: &'static str,
    pub(crate) current_l2_l3_revisions: Vec<&'a str>,
    pub(crate) excluded_session_ids: &'a [String],
    pub(crate) excluded_source_ids: &'a [String],
    pub(crate) skill_asset_id: Option<&'a str>,
    pub(crate) skill_content_hash: Option<&'a str>,
    pub(crate) skill_revision: i64,
    pub(crate) window_hours: i64,
}
