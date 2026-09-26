use super::NavigationModel;
use crate::backend::domain::AppShortcut;
use crate::backend::infrastructure::app_settings::AppLocale;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct BackgroundTaskGetParams {
    #[serde(alias = "taskId")]
    pub(crate) task_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct TenantCreateParams {
    pub(crate) name: String,
    pub(crate) slug: Option<String>,
    #[serde(default, alias = "setActive")]
    pub(crate) set_active: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateNavigationModelParams {
    pub(crate) model: NavigationModel,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateAppShortcutsParams {
    pub(crate) shortcuts: Vec<AppShortcut>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct LogsGetSnapshotParams {
    #[serde(alias = "fileName")]
    pub(crate) file_name: Option<String>,
    #[serde(alias = "lineLimit")]
    pub(crate) line_limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct LogsWriteOperationParams {
    pub(crate) level: String,
    pub(crate) operation: String,
    pub(crate) message: String,
    pub(crate) fields: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SaveAppSettingsParams {
    pub(crate) settings: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct InitializeAppLocaleParams {
    pub(crate) locale: AppLocale,
}
