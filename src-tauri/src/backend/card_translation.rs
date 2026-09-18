use crate::backend::{
    agents::types::AgentId,
    ai_execution::{
        execute_agent, AgentExecutionRuntime, AgentSessionMode, AiExecutionCancellation,
        AiExecutionError, AiExecutionLimits, AiExecutionPurpose, AiExecutionRequest,
    },
    runtime::{AppError, AppResult},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

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

#[derive(Debug, Deserialize, JsonSchema)]
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

pub(crate) fn check_action_availability_with_settings(
    runtime: &dyn AgentExecutionRuntime,
    action: &crate::backend::ai_execution::composition::ActionId,
    settings: &serde_json::Value,
) -> ActionAvailability {
    let Ok((agent_id, _model)) =
        crate::backend::ai_execution::composition::resolve_agent_for(action, settings)
    else {
        return ActionAvailability {
            available: false,
            agent_id: None,
            installed: false,
            version: None,
            error: Some(format!(
                "unknown or unavailable action: {}",
                action.as_str()
            )),
        };
    };
    let availability = runtime.check_availability(&agent_id);
    ActionAvailability {
        available: availability.available,
        agent_id: Some(agent_id.to_string()),
        installed: availability.installed,
        version: availability.version,
        error: availability.error.map(|error| error.to_string()),
    }
}

pub(crate) fn check_opencode_translation_availability_with_settings(
    runtime: &dyn AgentExecutionRuntime,
    settings: &serde_json::Value,
) -> OpencodeTranslationAvailability {
    let availability = check_action_availability_with_settings(
        runtime,
        &crate::backend::ai_execution::composition::ActionId::new("translation.card"),
        settings,
    );
    OpencodeTranslationAvailability {
        available: availability.available,
        version: availability.version,
        error: availability.error,
    }
}

pub(crate) fn check_prompt_optimization_availability_with_settings(
    runtime: &dyn AgentExecutionRuntime,
    settings: &serde_json::Value,
) -> ActionAvailability {
    check_action_availability_with_settings(
        runtime,
        &crate::backend::ai_execution::composition::ActionId::new("prompt.optimization"),
        settings,
    )
}

pub(crate) async fn test_conversation_translation_connection(
    runtime: Arc<dyn AgentExecutionRuntime>,
    params: ConversationTranslationConnectionRequest,
) -> OpencodeTranslationAvailability {
    let result = match params.provider {
        ConversationTranslationProvider::Cli => {
            let model = normalize_model(&params.model);
            match model {
                Ok(model) => {
                    if let Some(agent_id) = params.agent_id.as_deref() {
                        let id = match resolve_agent_id(Some(agent_id), params.cli) {
                            Ok(id) => id,
                            Err(e) => {
                                return OpencodeTranslationAvailability {
                                    available: false,
                                    version: None,
                                    error: Some(e.to_string()),
                                }
                            }
                        };
                        execute_agent_translation(
                            runtime,
                            id,
                            params.prompt,
                            model,
                            AiExecutionPurpose::ConnectionTest,
                            connection_test_limits(),
                        )
                        .await
                    } else {
                        match params.cli {
                            ConversationTranslationCli::Opencode => {
                                execute_opencode_translation(
                                    runtime,
                                    params.prompt,
                                    model,
                                    AiExecutionPurpose::ConnectionTest,
                                    connection_test_limits(),
                                )
                                .await
                            }
                            cli => {
                                let id = match resolve_agent_id(None, cli) {
                                    Ok(id) => id,
                                    Err(e) => {
                                        return OpencodeTranslationAvailability {
                                            available: false,
                                            version: None,
                                            error: Some(e.to_string()),
                                        }
                                    }
                                };
                                execute_agent_translation(
                                    runtime,
                                    id,
                                    params.prompt,
                                    model,
                                    AiExecutionPurpose::ConnectionTest,
                                    connection_test_limits(),
                                )
                                .await
                            }
                        }
                    }
                }
                Err(e) => Err(e),
            }
        }
        provider => {
            translate_conversation_card(
                runtime,
                ConversationTranslationRequest {
                    provider,
                    agent_id: params.agent_id,
                    cli: params.cli,
                    model: params.model,
                    prompt: params.prompt,
                },
            )
            .await
        }
    };

    match result {
        Ok(_) => OpencodeTranslationAvailability {
            available: true,
            version: None,
            error: None,
        },
        Err(error) => OpencodeTranslationAvailability {
            available: false,
            version: None,
            error: Some(error.to_string()),
        },
    }
}

pub(crate) fn list_conversation_translation_models(
    runtime: &dyn AgentExecutionRuntime,
    params: ConversationTranslationModelsRequest,
) -> ConversationTranslationModelsResult {
    let ConversationTranslationProvider::Cli = params.provider else {
        return ConversationTranslationModelsResult {
            models: Vec::new(),
            error: Some(
                "model listing is only available for CLI translation providers".to_string(),
            ),
        };
    };

    match params.cli {
        ConversationTranslationCli::Opencode => {
            match runtime.discover_models(&opencode_agent_id(), Duration::from_secs(20)) {
                Ok(output) => ConversationTranslationModelsResult {
                    models: parse_model_lines(&output),
                    error: None,
                },
                Err(error) => ConversationTranslationModelsResult {
                    models: Vec::new(),
                    error: Some(error.to_string()),
                },
            }
        }
        ConversationTranslationCli::Gemini => ConversationTranslationModelsResult {
            models: Vec::new(),
            error: Some(
                "Gemini CLI does not expose a model listing command; enter a model manually"
                    .to_string(),
            ),
        },
    }
}

pub(crate) async fn translate_conversation_card(
    runtime: Arc<dyn AgentExecutionRuntime>,
    params: ConversationTranslationRequest,
) -> AppResult<OpencodeTranslationResult> {
    validate_translation_prompt(&params.prompt)?;
    let model = normalize_model(&params.model)?;

    match params.provider {
        ConversationTranslationProvider::Cli => {
            if let Some(agent_id) = params.agent_id.as_deref() {
                execute_agent_translation(
                    runtime,
                    resolve_agent_id(Some(agent_id), params.cli)?,
                    params.prompt,
                    model,
                    AiExecutionPurpose::Translation,
                    AiExecutionLimits::default(),
                )
                .await
            } else {
                translate_with_cli(runtime, params.cli, model, params.prompt).await
            }
        }
        ConversationTranslationProvider::Google => Err(AppError::Validation(
            "Google Translate provider is reserved but not implemented yet".to_string(),
        )),
        ConversationTranslationProvider::Apple => Err(AppError::Validation(
            "Apple Translate provider is reserved but not implemented yet".to_string(),
        )),
    }
}

pub(crate) async fn optimize_prompt(
    runtime: Arc<dyn AgentExecutionRuntime>,
    params: PromptOptimizationRequest,
) -> AppResult<PromptOptimizationResult> {
    validate_translation_prompt(&params.prompt)?;
    let model = normalize_model(&params.model)?;
    let ConversationTranslationProvider::Cli = params.provider else {
        return Err(AppError::Validation(
            "prompt optimization currently requires a CLI provider".to_string(),
        ));
    };
    let result = execute_agent_translation(
        runtime,
        resolve_agent_id(params.agent_id.as_deref(), params.cli)?,
        params.prompt,
        model,
        AiExecutionPurpose::PromptOptimization,
        AiExecutionLimits::default(),
    )
    .await?;
    Ok(PromptOptimizationResult {
        optimized_text: result.translated_text,
    })
}

pub(crate) fn prepare_opencode_agent_translation(
    params: ConversationTranslationRequest,
) -> AppResult<(AgentId, String, Option<String>)> {
    let ConversationTranslationProvider::Cli = params.provider else {
        return Err(AppError::Validation(
            "AI execution tasks require a CLI translation provider".to_string(),
        ));
    };
    validate_translation_prompt(&params.prompt)?;
    let model = normalize_model(&params.model)?;
    let agent_id = resolve_agent_id(params.agent_id.as_deref(), params.cli)?;
    Ok((agent_id, params.prompt.trim().to_string(), model))
}

pub(crate) async fn translate_conversation_card_with_opencode(
    runtime: Arc<dyn AgentExecutionRuntime>,
    params: OpencodeTranslationRequest,
) -> AppResult<OpencodeTranslationResult> {
    validate_translation_prompt(&params.prompt)?;
    execute_opencode_translation(
        runtime,
        params.prompt,
        None,
        AiExecutionPurpose::Translation,
        AiExecutionLimits::default(),
    )
    .await
}

async fn translate_with_cli(
    runtime: Arc<dyn AgentExecutionRuntime>,
    cli: ConversationTranslationCli,
    model: Option<String>,
    prompt: String,
) -> AppResult<OpencodeTranslationResult> {
    match cli {
        ConversationTranslationCli::Opencode => {
            execute_opencode_translation(
                runtime,
                prompt,
                model,
                AiExecutionPurpose::Translation,
                AiExecutionLimits::default(),
            )
            .await
        }
        cli => {
            execute_agent_translation(
                runtime,
                resolve_agent_id(None, cli)?,
                prompt,
                model,
                AiExecutionPurpose::Translation,
                AiExecutionLimits::default(),
            )
            .await
        }
    }
}

fn resolve_agent_id(agent_id: Option<&str>, cli: ConversationTranslationCli) -> AppResult<AgentId> {
    if let Some(agent_id) = agent_id.map(str::trim).filter(|value| !value.is_empty()) {
        return AgentId::parse(agent_id).map_err(|error| AppError::Validation(error.to_string()));
    }
    let legacy_id = match cli {
        ConversationTranslationCli::Opencode => "opencode",
        ConversationTranslationCli::Gemini => "gemini",
    };
    AgentId::parse(legacy_id).map_err(|error| AppError::Validation(error.to_string()))
}

fn default_translation_cli() -> ConversationTranslationCli {
    ConversationTranslationCli::Opencode
}

async fn execute_agent_translation(
    runtime: Arc<dyn AgentExecutionRuntime>,
    agent_id: AgentId,
    prompt: String,
    model: Option<String>,
    purpose: AiExecutionPurpose,
    limits: AiExecutionLimits,
) -> AppResult<OpencodeTranslationResult> {
    validate_translation_prompt(&prompt)?;
    let request = AiExecutionRequest {
        execution_id: uuid::Uuid::new_v4().to_string(),
        agent_id,
        purpose,
        session_mode: AgentSessionMode::OneShot,
        prompt: prompt.trim().to_string(),
        model,
        limits,
        cancellation: AiExecutionCancellation::default(),
        progress: None,
        tenant_id: None,
        execution_context_key: None,
        binding: None,
        replay: false,
        restore_only: false,
        team_tools: None,
        recall_tools: None,
        memory_generation_tools: None,
    };
    request.validate().map_err(app_error_from_ai)?;
    let result = execute_agent(runtime, request)
        .await
        .map_err(app_error_from_ai)?;
    Ok(OpencodeTranslationResult {
        translated_text: result.text,
    })
}

async fn execute_opencode_translation(
    runtime: Arc<dyn AgentExecutionRuntime>,
    prompt: String,
    model: Option<String>,
    purpose: AiExecutionPurpose,
    limits: AiExecutionLimits,
) -> AppResult<OpencodeTranslationResult> {
    validate_translation_prompt(&prompt)?;
    let prompt = prompt.trim().to_string();
    execute_agent_translation(runtime, opencode_agent_id(), prompt, model, purpose, limits).await
}

fn opencode_agent_id() -> AgentId {
    AgentId::parse("opencode").expect("builtin OpenCode agent id must be valid")
}

fn connection_test_limits() -> AiExecutionLimits {
    AiExecutionLimits {
        total_timeout: Duration::from_secs(30),
        ..AiExecutionLimits::default()
    }
}

fn validate_translation_prompt(prompt: &str) -> AppResult<()> {
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

fn normalize_model(model: &str) -> AppResult<Option<String>> {
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

fn app_error_from_ai(error: AiExecutionError) -> AppError {
    let view = error.to_view();
    match view.code.as_str() {
        "invalid_request" => AppError::Validation(view.message),
        "cancelled" => AppError::Cancelled(view.message),
        _ => AppError::Domain {
            code: view.code,
            message: view.message,
            retryable: view.retryable,
            details: None,
        },
    }
}

fn parse_model_lines(bytes: &[u8]) -> Vec<String> {
    let mut models = String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("opencode models"))
        .map(str::to_string)
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    models.truncate(500);
    models
}

#[cfg(test)]
#[path = "card_translation_tests.rs"]
mod tests;
