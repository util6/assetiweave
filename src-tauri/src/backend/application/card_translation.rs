use super::prelude::*;
use crate::backend::runtime::AppResult as RuntimeAppResult;

impl AppService {
    pub(crate) fn check_opencode_translation_availability(
        &self,
    ) -> RuntimeAppResult<crate::backend::card_translation::OpencodeTranslationAvailability> {
        let settings = self.app_settings_value();
        Ok(
            crate::backend::card_translation::check_opencode_translation_availability_with_settings(
                self.agent_runtime()?.as_ref(),
                &settings,
            ),
        )
    }

    pub(crate) fn check_prompt_optimization_availability(
        &self,
    ) -> RuntimeAppResult<crate::backend::card_translation::ActionAvailability> {
        let settings = self.app_settings_value();
        Ok(
            crate::backend::card_translation::check_prompt_optimization_availability_with_settings(
                self.agent_runtime()?.as_ref(),
                &settings,
            ),
        )
    }

    pub(crate) async fn translate_conversation_card_with_opencode(
        &self,
        params: crate::backend::card_translation::OpencodeTranslationRequest,
    ) -> RuntimeAppResult<crate::backend::card_translation::OpencodeTranslationResult> {
        Ok(
            crate::backend::card_translation::translate_conversation_card_with_opencode(
                self.agent_runtime()?,
                params,
            )
            .await?,
        )
    }

    pub(crate) async fn translate_conversation_card(
        &self,
        params: crate::backend::card_translation::ConversationTranslationRequest,
    ) -> RuntimeAppResult<crate::backend::card_translation::OpencodeTranslationResult> {
        Ok(
            crate::backend::card_translation::translate_conversation_card(
                self.agent_runtime()?,
                params,
            )
            .await?,
        )
    }

    pub(crate) async fn optimize_prompt(
        &self,
        params: crate::backend::card_translation::PromptOptimizationRequest,
    ) -> RuntimeAppResult<crate::backend::card_translation::PromptOptimizationResult> {
        Ok(
            crate::backend::card_translation::optimize_prompt(self.agent_runtime()?, params)
                .await?,
        )
    }

    pub(crate) async fn test_conversation_translation_connection(
        &self,
        params: crate::backend::card_translation::ConversationTranslationConnectionRequest,
    ) -> RuntimeAppResult<crate::backend::card_translation::OpencodeTranslationAvailability> {
        Ok(
            crate::backend::card_translation::test_conversation_translation_connection(
                self.agent_runtime()?,
                params,
            )
            .await,
        )
    }

    pub(crate) fn list_conversation_translation_models(
        &self,
        params: crate::backend::card_translation::ConversationTranslationModelsRequest,
    ) -> RuntimeAppResult<crate::backend::card_translation::ConversationTranslationModelsResult>
    {
        Ok(
            crate::backend::card_translation::list_conversation_translation_models(
                self.agent_runtime()?.as_ref(),
                params,
            ),
        )
    }

    fn agent_runtime(
        &self,
    ) -> RuntimeAppResult<std::sync::Arc<dyn crate::backend::ai_execution::AgentExecutionRuntime>>
    {
        Ok(self.agent_runtime.clone())
    }
}

#[cfg(test)]
#[path = "card_translation_tests.rs"]
mod tests;
