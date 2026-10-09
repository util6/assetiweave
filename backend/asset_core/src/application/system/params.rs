use super::NavigationModel;
use crate::backend::domain::AppShortcut;
use crate::backend::infrastructure::app_settings::AppLocale;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BackgroundTaskGetParams {
    #[serde(alias = "taskId")]
    pub task_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TenantCreateParams {
    pub name: String,
    pub slug: Option<String>,
    #[serde(default, alias = "setActive")]
    pub set_active: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateNavigationModelParams {
    pub model: NavigationModel,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateAppShortcutsParams {
    pub shortcuts: Vec<AppShortcut>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LogsGetSnapshotParams {
    #[serde(alias = "fileName")]
    pub file_name: Option<String>,
    #[serde(alias = "lineLimit")]
    pub line_limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LogsWriteOperationParams {
    pub level: String,
    pub operation: String,
    pub message: String,
    pub fields: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SaveAppSettingsParams {
    pub settings: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InitializeAppLocaleParams {
    pub locale: AppLocale,
}
