use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AccountProvider {
    Codex,
    Antigravity,
    Claude,
    Gemini,
    OpenAi,
    Custom(String),
}

impl AccountProvider {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Codex => "codex",
            Self::Antigravity => "antigravity",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::OpenAi => "openai",
            Self::Custom(name) => name.as_str(),
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "codex" => Self::Codex,
            "antigravity" => Self::Antigravity,
            "claude" => Self::Claude,
            "gemini" => Self::Gemini,
            "openai" => Self::OpenAi,
            other => Self::Custom(other.to_string()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AuthType {
    ApiKey,
    OAuthToken,
    ConfigDirectory,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentAccount {
    pub id: String,
    pub name: String,
    pub provider: AccountProvider,
    pub auth_type: AuthType,
    pub credential: Option<String>,
    pub custom_config_dir: Option<String>,
    pub env_overrides: HashMap<String, String>,
}

impl AgentAccount {
    pub fn new_api_key(
        id: impl Into<String>,
        name: impl Into<String>,
        provider: AccountProvider,
        key: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            provider,
            auth_type: AuthType::ApiKey,
            credential: Some(key.into()),
            custom_config_dir: None,
            env_overrides: HashMap::new(),
        }
    }

    pub fn new_oauth_profile(
        id: impl Into<String>,
        name: impl Into<String>,
        provider: AccountProvider,
        config_dir: Option<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            provider,
            auth_type: AuthType::ConfigDirectory,
            credential: None,
            custom_config_dir: config_dir,
            env_overrides: HashMap::new(),
        }
    }
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
