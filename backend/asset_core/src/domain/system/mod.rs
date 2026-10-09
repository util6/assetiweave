use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod error;
pub mod navigation;

pub use error::*;
pub use navigation::*;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppShortcut {
    pub profile_id: String,
    pub profile_name: String,
    pub app_kind: String,
    pub display_icon: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_svg: Option<AppShortcutIconSvg>,
    pub accent_color: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppShortcutIconSvg {
    pub paths: Vec<AppShortcutIconPath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view_box: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppShortcutIconPath {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_rule: Option<String>,
    pub d: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill_rule: Option<String>,
}
