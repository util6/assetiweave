use std::sync::Arc;

use crate::backend::{
    application::prelude::*,
    application::AppResult as RuntimeAppResult,
    domain::agents::AgentId,
    infrastructure::agent_execution::{
        AgentExecutionRuntime, AgentSessionMode, AiExecutionCancellation, AiExecutionError,
        AiExecutionLimits, AiExecutionProgressSink, AiExecutionPurpose, AiExecutionRequest,
        AiExecutionResult,
    },
};

mod types;
mod workflow;

pub(crate) use types::*;
pub(crate) use workflow::*;

pub(crate) type PreparedConversationCardTranslation = (AgentId, String, Option<String>);

impl AppService {
    pub(crate) fn prepare_conversation_card_translation(
        params: ConversationTranslationRequest,
    ) -> RuntimeAppResult<PreparedConversationCardTranslation> {
        prepare_opencode_agent_translation(params)
    }

    pub(crate) async fn execute_prepared_conversation_card_translation(
        &self,
        prepared: PreparedConversationCardTranslation,
        execution_id: String,
        cancellation: AiExecutionCancellation,
        progress: Option<Arc<dyn AiExecutionProgressSink>>,
    ) -> Result<AiExecutionResult, AiExecutionError> {
        let (agent_id, prompt, model) = prepared;
        self.agent_runtime
            .execute(AiExecutionRequest {
                execution_id,
                agent_id,
                purpose: AiExecutionPurpose::Translation,
                session_mode: AgentSessionMode::OneShot,
                prompt,
                model,
                limits: AiExecutionLimits::default(),
                cancellation,
                progress,
                tenant_id: None,
                execution_context_key: None,
                binding: None,
                replay: false,
                restore_only: false,
                recall_tools: None,
                memory_generation_tools: None,
            })
            .await
    }

    pub(crate) fn check_opencode_translation_availability(
        &self,
    ) -> RuntimeAppResult<OpencodeTranslationAvailability> {
        let settings = self.app_settings_value();
        Ok(check_opencode_translation_availability_with_settings(
            self.agent_runtime()?.as_ref(),
            &settings,
        ))
    }

    pub(crate) fn check_prompt_optimization_availability(
        &self,
    ) -> RuntimeAppResult<ActionAvailability> {
        let settings = self.app_settings_value();
        Ok(check_prompt_optimization_availability_with_settings(
            self.agent_runtime()?.as_ref(),
            &settings,
        ))
    }

    pub(crate) async fn translate_conversation_card_with_opencode(
        &self,
        params: OpencodeTranslationRequest,
    ) -> RuntimeAppResult<OpencodeTranslationResult> {
        Ok(translate_conversation_card_with_opencode(self.agent_runtime()?, params).await?)
    }

    pub(crate) async fn translate_conversation_card(
        &self,
        params: ConversationTranslationRequest,
    ) -> RuntimeAppResult<OpencodeTranslationResult> {
        Ok(translate_conversation_card(self.agent_runtime()?, params).await?)
    }

    pub(crate) async fn optimize_prompt(
        &self,
        params: PromptOptimizationRequest,
    ) -> RuntimeAppResult<PromptOptimizationResult> {
        Ok(optimize_prompt(self.agent_runtime()?, params).await?)
    }

    pub(crate) async fn test_conversation_translation_connection(
        &self,
        params: ConversationTranslationConnectionRequest,
    ) -> RuntimeAppResult<OpencodeTranslationAvailability> {
        Ok(test_conversation_translation_connection(self.agent_runtime()?, params).await)
    }

    pub(crate) fn list_conversation_translation_models(
        &self,
        params: ConversationTranslationModelsRequest,
    ) -> RuntimeAppResult<ConversationTranslationModelsResult> {
        Ok(list_conversation_translation_models(
            self.agent_runtime()?.as_ref(),
            params,
        ))
    }

    fn agent_runtime(&self) -> RuntimeAppResult<Arc<dyn AgentExecutionRuntime>> {
        Ok(self.agent_runtime.clone())
    }
}

#[cfg(test)]
#[path = "card_translation_tests.rs"]
mod tests;
