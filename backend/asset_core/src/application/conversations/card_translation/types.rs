use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::backend::application::{AppError, AppResult};

#[derive(Debug, Serialize)]
pub struct OpencodeTranslationAvailability {
    pub available: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ActionAvailability {
    pub available: bool,
    pub agent_id: Option<String>,
    pub installed: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OpencodeTranslationRequest {
    pub prompt: String,
}

#[derive(Debug, Serialize)]
pub struct OpencodeTranslationResult {
    pub translated_text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PromptOptimizationRequest {
    pub provider: ConversationTranslationProvider,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default = "default_translation_cli")]
    pub cli: ConversationTranslationCli,
    pub model: String,
    pub prompt: String,
}

#[derive(Debug, Serialize)]
pub struct PromptOptimizationResult {
    pub optimized_text: String,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationTranslationProvider {
    Cli,
    Google,
    Apple,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationTranslationCli {
    Opencode,
    Gemini,
}

pub fn default_translation_cli() -> ConversationTranslationCli {
    ConversationTranslationCli::Opencode
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationTranslationRequest {
    pub provider: ConversationTranslationProvider,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default = "default_translation_cli")]
    pub cli: ConversationTranslationCli,
    pub model: String,
    pub prompt: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationTranslationConnectionRequest {
    pub provider: ConversationTranslationProvider,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default = "default_translation_cli")]
    pub cli: ConversationTranslationCli,
    pub model: String,
    pub prompt: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationTranslationModelsRequest {
    pub provider: ConversationTranslationProvider,
    pub cli: ConversationTranslationCli,
}

#[derive(Debug, Serialize)]
pub struct ConversationTranslationModelsResult {
    pub models: Vec<String>,
    pub error: Option<String>,
}

pub fn validate_translation_prompt(prompt: &str) -> AppResult<()> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err(AppError::Validation(
            "translation prompt is empty".to_string(),
        ));
    }
    if prompt.len() > 200_000 {
        return Err(AppError::Validation(
            "translation prompt is too large".to_string(),
        ));
    }
    Ok(())
}

pub fn normalize_model(model: &str) -> AppResult<Option<String>> {
    let model = model.trim();
    if model.is_empty() {
        return Ok(None);
    }
    if model.len() > 120 || model.contains(['\n', '\r', '\0']) {
        return Err(AppError::Validation(
            "translation model is invalid".to_string(),
        ));
    }
    Ok(Some(model.to_string()))
}
