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
pub struct WatermarkTarget {
    pub target_watermark_utc: DateTime<Utc>,
    pub local_watermark_date: String,
    pub local_watermark_time: String,
    pub timezone_offset_minutes: i64,
    pub window_hours: i64,
    pub window_start_utc: DateTime<Utc>,
    pub window_end_utc: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RecentSnapshotPreparation {
    pub target: WatermarkTarget,
    pub skill_binding: MemorySkillBinding,
    pub target_fingerprint: String,
    pub content_fingerprint: String,
    pub evidence: RecentSnapshotWorkOrderEvidencePack,
    pub skill_text: String,
}

#[derive(Debug, Default)]
pub struct RecentSnapshotFrozenContext {
    pub session_memories: BTreeMap<String, SessionMemory>,
    pub source_references: BTreeMap<String, Vec<SessionMemorySourceReference>>,
    pub recent_events: BTreeMap<String, Vec<crate::backend::domain::RecentMemoryEvent>>,
    pub current_l2_projects: Vec<L2ProjectMemoryView>,
    pub current_l3_items: Vec<L3MemoryItemView>,
}

#[derive(Debug, Serialize)]
pub struct TargetFingerprintSessionRef<'a> {
    pub session_id: &'a str,
    pub source_revision: i64,
}

#[derive(Debug, Serialize)]
pub struct TargetFingerprintItemRef<'a> {
    pub item_id: &'a str,
    pub revision_id: &'a str,
}

#[derive(Debug, Serialize)]
pub struct TargetFingerprintPayload<'a> {
    pub budget_policy_version: &'static str,
    pub candidate_sessions: Vec<TargetFingerprintSessionRef<'a>>,
    pub contract_version: &'static str,
    pub prior_active_items: Vec<TargetFingerprintItemRef<'a>>,
    pub projection_policy_version: &'static str,
    pub skill_asset_id: Option<&'a str>,
    pub skill_content_hash: Option<&'a str>,
    pub skill_revision: i64,
    pub target_watermark_utc: &'a str,
    pub tenant_id: &'a str,
    pub window_hours: i64,
}

#[derive(Debug, Serialize)]
pub struct ContentFingerprintSessionRef<'a> {
    pub last_activity_at: &'a str,
    pub session_id: &'a str,
    pub source_revision: i64,
}

#[derive(Debug, Serialize)]
pub struct ContentFingerprintItemRef<'a> {
    pub item_id: &'a str,
    pub remaining_lifetime_bucket: i64,
    pub revision_id: &'a str,
}

#[derive(Debug, Serialize)]
pub struct ContentFingerprintPayload<'a> {
    pub candidate_sessions: Vec<ContentFingerprintSessionRef<'a>>,
    pub carry_over_items: Vec<ContentFingerprintItemRef<'a>>,
    pub contract_version: &'static str,
    pub current_l2_l3_revisions: Vec<&'a str>,
    pub excluded_session_ids: &'a [String],
    pub excluded_source_ids: &'a [String],
    pub skill_asset_id: Option<&'a str>,
    pub skill_content_hash: Option<&'a str>,
    pub skill_revision: i64,
    pub window_hours: i64,
}
