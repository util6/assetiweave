use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::backend::application::{AppError, AppResult};

#[derive(Debug, Serialize)]
pub(crate) struct OpencodeTranslationAvailability {
    pub(crate) available: bool,
    pub(crate) version: Option<String>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ActionAvailability {
    pub(crate) available: bool,
    pub(crate) agent_id: Option<String>,
    pub(crate) installed: bool,
    pub(crate) version: Option<String>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct OpencodeTranslationRequest {
    pub(crate) prompt: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct OpencodeTranslationResult {
    pub(crate) translated_text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct PromptOptimizationRequest {
    pub(crate) provider: ConversationTranslationProvider,
    #[serde(default)]
    pub(crate) agent_id: Option<String>,
    #[serde(default = "default_translation_cli")]
    pub(crate) cli: ConversationTranslationCli,
    pub(crate) model: String,
    pub(crate) prompt: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct PromptOptimizationResult {
    pub(crate) optimized_text: String,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConversationTranslationProvider {
    Cli,
    Google,
    Apple,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConversationTranslationCli {
    Opencode,
    Gemini,
}

pub(crate) fn default_translation_cli() -> ConversationTranslationCli {
    ConversationTranslationCli::Opencode
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationTranslationRequest {
    pub(crate) provider: ConversationTranslationProvider,
    #[serde(default)]
    pub(crate) agent_id: Option<String>,
    #[serde(default = "default_translation_cli")]
    pub(crate) cli: ConversationTranslationCli,
    pub(crate) model: String,
    pub(crate) prompt: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationTranslationConnectionRequest {
    pub(crate) provider: ConversationTranslationProvider,
    #[serde(default)]
    pub(crate) agent_id: Option<String>,
    #[serde(default = "default_translation_cli")]
    pub(crate) cli: ConversationTranslationCli,
    pub(crate) model: String,
    pub(crate) prompt: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationTranslationModelsRequest {
    pub(crate) provider: ConversationTranslationProvider,
    pub(crate) cli: ConversationTranslationCli,
}

#[derive(Debug, Serialize)]
pub(crate) struct ConversationTranslationModelsResult {
    pub(crate) models: Vec<String>,
    pub(crate) error: Option<String>,
}

pub(crate) fn validate_translation_prompt(prompt: &str) -> AppResult<()> {
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

pub(crate) fn normalize_model(model: &str) -> AppResult<Option<String>> {
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
