use crate::targeting::PhysicalMountState;
use assetiweave_core::{AppKind, AssetKind, SourceKind, SourceOrigin, SourceScannerKind};
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
    pub(crate) scanner_kind: Option<SourceScannerKind>,
    pub(crate) source_origin: Option<SourceOrigin>,
    pub(crate) repo_root: Option<String>,
    pub(crate) scan_root: Option<String>,
    pub(crate) origin_app_kind: Option<AppKind>,
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

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PhysicalMountStateDto {
    Mounted,
    NotMounted,
    Conflict,
    Broken,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AssetMountStatus {
    pub(crate) asset_id: String,
    pub(crate) profile_id: String,
    pub(crate) target_dir: String,
    pub(crate) target_path: String,
    pub(crate) state: PhysicalMountStateDto,
    pub(crate) linked_source: Option<String>,
}

impl From<PhysicalMountState> for PhysicalMountStateDto {
    fn from(value: PhysicalMountState) -> Self {
        match value {
            PhysicalMountState::Mounted => Self::Mounted,
            PhysicalMountState::NotMounted => Self::NotMounted,
            PhysicalMountState::Conflict => Self::Conflict,
            PhysicalMountState::Broken => Self::Broken,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppShortcut {
    pub(crate) profile_id: String,
    pub(crate) profile_name: String,
    pub(crate) app_kind: String,
    pub(crate) display_icon: String,
    pub(crate) accent_color: String,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NavigationModel {
    pub(crate) active_rail_id: String,
    pub(crate) active_header_tab_id: String,
    pub(crate) active_sub_nav_id: String,
    pub(crate) rail_items: Vec<RailMenuItem>,
    pub(crate) header_tabs: Vec<HeaderTabItem>,
    pub(crate) sub_nav_items: std::collections::BTreeMap<String, Vec<SubNavItem>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RailMenuItem {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) icon: String,
    pub(crate) scope: String,
    pub(crate) enabled: bool,
    pub(crate) position: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HeaderTabItem {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) asset_kind: Option<String>,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubNavItem {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) route_key: String,
    pub(crate) enabled: bool,
}
