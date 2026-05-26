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

#[derive(Debug, Clone, Serialize)]
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
