use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionViewModel {
    pub cli_version: String,
    pub engine_version: String,
    pub compatible: bool,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewViewModel {
    pub source_count: usize,
    pub asset_count: usize,
    pub profile_count: usize,
    pub last_scan_status: String,
}
