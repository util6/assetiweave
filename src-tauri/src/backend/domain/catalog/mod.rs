use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::mounting::AppKind;

pub mod read_models;
pub use read_models::*;

mod classifier_rules;
pub use classifier_rules::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Prompt,
    Rule,
    Memory,
    Skill,
    Mcp,
    Agent,
    Command,
    Workflow,
    Profile,
    Custom,
    Unclassified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetFormat {
    Markdown,
    Json,
    Yaml,
    Toml,
    Directory,
    Script,
    Sqlite,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Local,
    GitCheckout,
    Import,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceScannerKind {
    Skill,
    Mcp,
    Prompt,
    Rule,
    Mixed,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceOrigin {
    GitRepo,
    LocalFolder,
    AppTarget,
    AppLocal,
    AssetiweaveLibrary,
    AssetiweaveSystem,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub kind: SourceKind,
    pub root_path: String,
    pub scanner_kind: SourceScannerKind,
    pub source_origin: SourceOrigin,
    pub repo_root: Option<String>,
    pub scan_root: String,
    pub origin_app_kind: Option<AppKind>,
    #[serde(default)]
    pub origin_provider_id: Option<String>,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub default_kind: Option<AssetKind>,
    pub enabled: bool,
    pub priority: i32,
    pub last_scanned_at: Option<String>,
    pub last_scan_status: Option<String>,
}

pub const SYSTEM_SKILL_SOURCE_ID: &str = "assetiweave-system-skills";

pub(crate) fn system_skill_source(root_path: String) -> Source {
    Source {
        id: SYSTEM_SKILL_SOURCE_ID.to_string(),
        name: "AssetIWeave System Skills".to_string(),
        kind: SourceKind::Local,
        root_path,
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::AssetiweaveSystem,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        origin_provider_id: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: vec![
            "**/.git/**".to_string(),
            "**/node_modules/**".to_string(),
            "**/target/**".to_string(),
            "**/dist/**".to_string(),
        ],
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: -200,
        last_scanned_at: None,
        last_scan_status: Some("pending".to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Asset {
    pub id: String,
    pub source_id: String,
    pub name: String,
    pub kind: AssetKind,
    #[serde(default = "default_asset_detector_id")]
    pub detector_id: String,
    #[serde(default = "default_asset_detector_version")]
    pub detector_version: u32,
    pub format: AssetFormat,
    pub relative_path: String,
    pub absolute_path: String,
    pub entry_file: Option<String>,
    pub description: Option<String>,
    pub content_hash: Option<String>,
    pub discovered_at: String,
    pub updated_at: String,
}

fn default_asset_detector_id() -> String {
    "legacy.classifier".to_string()
}

fn default_asset_detector_version() -> u32 {
    1
}

pub fn stable_asset_id(source_id: &str, relative_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_id.as_bytes());
    hasher.update(b":");
    hasher.update(relative_path.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
