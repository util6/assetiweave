use assetiweave_core::{AssetKind, SourceKind};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Mutex};

pub(crate) type AppResult<T> = Result<T, String>;

pub(crate) struct AppState {
    pub(crate) db_path: PathBuf,
    pub(crate) lock: Mutex<()>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AppOverview {
    pub(crate) source_count: usize,
    pub(crate) asset_count: usize,
    pub(crate) profile_count: usize,
    pub(crate) last_scan_status: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SourceInput {
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    pub(crate) kind: SourceKind,
    pub(crate) root_path: String,
    pub(crate) include_globs: Vec<String>,
    pub(crate) exclude_globs: Vec<String>,
    pub(crate) default_kind: Option<AssetKind>,
    pub(crate) enabled: bool,
    pub(crate) priority: i32,
}

#[derive(Debug, Serialize)]
pub(crate) struct ExecutionResult {
    pub(crate) executed_count: usize,
    pub(crate) skipped_count: usize,
    pub(crate) conflict_count: usize,
    pub(crate) errors: Vec<String>,
}
