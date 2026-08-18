use super::prelude::*;

impl AppService {
    pub(crate) fn check_action_availability(
        &self,
        action: &crate::backend::ai_execution::composition::ActionId,
    ) -> AppResult<crate::backend::card_translation::ActionAvailability> {
        Ok(crate::backend::card_translation::check_action_availability(
            self.agent_runtime()?.as_ref(),
            action,
        ))
    }

    pub(crate) fn check_opencode_translation_availability(
        &self,
    ) -> AppResult<crate::backend::card_translation::OpencodeTranslationAvailability> {
        Ok(
            crate::backend::card_translation::check_opencode_translation_availability(
                self.agent_runtime()?.as_ref(),
            ),
        )
    }

    pub(crate) fn translate_conversation_card_with_opencode(
        &self,
        params: crate::backend::card_translation::OpencodeTranslationRequest,
    ) -> AppResult<crate::backend::card_translation::OpencodeTranslationResult> {
        crate::backend::card_translation::translate_conversation_card_with_opencode(
            self.agent_runtime()?,
            params,
        )
    }

    pub(crate) fn translate_conversation_card(
        &self,
        params: crate::backend::card_translation::ConversationTranslationRequest,
    ) -> AppResult<crate::backend::card_translation::OpencodeTranslationResult> {
        crate::backend::card_translation::translate_conversation_card(self.agent_runtime()?, params)
    }

    pub(crate) fn test_conversation_translation_connection(
        &self,
        params: crate::backend::card_translation::ConversationTranslationConnectionRequest,
    ) -> AppResult<crate::backend::card_translation::OpencodeTranslationAvailability> {
        Ok(
            crate::backend::card_translation::test_conversation_translation_connection(
                self.agent_runtime()?,
                params,
            ),
        )
    }

    pub(crate) fn list_conversation_translation_models(
        &self,
        params: crate::backend::card_translation::ConversationTranslationModelsRequest,
    ) -> AppResult<crate::backend::card_translation::ConversationTranslationModelsResult> {
        Ok(
            crate::backend::card_translation::list_conversation_translation_models(
                self.agent_runtime()?.as_ref(),
                params,
            ),
        )
    }

    fn agent_runtime(
        &self,
    ) -> AppResult<std::sync::Arc<dyn crate::backend::ai_execution::AgentExecutionRuntime>> {
        Ok(self.agent_runtime.clone())
    }
}

impl super::service::AgentAppService {
    pub(crate) fn check_opencode_translation_availability(
        &self,
    ) -> AppResult<crate::backend::card_translation::OpencodeTranslationAvailability> {
        Ok(
            crate::backend::card_translation::check_opencode_translation_availability(
                self.agent_runtime.as_ref(),
            ),
        )
    }

    pub(crate) fn translate_conversation_card_with_opencode(
        &self,
        params: crate::backend::card_translation::OpencodeTranslationRequest,
    ) -> AppResult<crate::backend::card_translation::OpencodeTranslationResult> {
        crate::backend::card_translation::translate_conversation_card_with_opencode(
            self.agent_runtime.clone(),
            params,
        )
    }

    pub(crate) fn translate_conversation_card(
        &self,
        params: crate::backend::card_translation::ConversationTranslationRequest,
    ) -> AppResult<crate::backend::card_translation::OpencodeTranslationResult> {
        crate::backend::card_translation::translate_conversation_card(
            self.agent_runtime.clone(),
            params,
        )
    }

    pub(crate) fn test_conversation_translation_connection(
        &self,
        params: crate::backend::card_translation::ConversationTranslationConnectionRequest,
    ) -> AppResult<crate::backend::card_translation::OpencodeTranslationAvailability> {
        Ok(
            crate::backend::card_translation::test_conversation_translation_connection(
                self.agent_runtime.clone(),
                params,
            ),
        )
    }

    pub(crate) fn list_conversation_translation_models(
        &self,
        params: crate::backend::card_translation::ConversationTranslationModelsRequest,
    ) -> AppResult<crate::backend::card_translation::ConversationTranslationModelsResult> {
        Ok(
            crate::backend::card_translation::list_conversation_translation_models(
                self.agent_runtime.as_ref(),
                params,
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{
        agents::types::{AgentId, AgentProtocol},
        ai_execution::{
            executor::BackendFuture, AgentExecutionRuntime, AiExecutionRequest, AiExecutionResult,
        },
        card_translation::{
            ConversationTranslationCli, ConversationTranslationProvider,
            ConversationTranslationRequest,
        },
    };
    use std::sync::{Arc, Mutex};

    struct FakeRuntime {
        requests: Mutex<Vec<AiExecutionRequest>>,
    }

    impl AgentExecutionRuntime for FakeRuntime {
        fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
            Box::pin(async move {
                self.requests.lock().unwrap().push(request.clone());
                Ok(AiExecutionResult {
                    text: "译文".to_string(),
                    agent_id: AgentId::parse("opencode").unwrap(),
                    protocol: AgentProtocol::Acp,
                    requested_model: request.model,
                    elapsed_ms: 1,
                })
            })
        }
    }

    #[test]
    fn app_service_translation_uses_the_injected_runtime() {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-translation-service-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let runtime = Arc::new(FakeRuntime {
            requests: Mutex::new(Vec::new()),
        });
        let service =
            AppService::open_with_db_path_and_runtime(root.join("app.db"), runtime.clone())
                .unwrap();

        let result = service
            .translate_conversation_card(ConversationTranslationRequest {
                agent_id: None,
                provider: ConversationTranslationProvider::Cli,
                cli: ConversationTranslationCli::Opencode,
                model: "model/a".to_string(),
                prompt: "translate".to_string(),
            })
            .unwrap();

        assert_eq!(result.translated_text, "译文");
        let requests = runtime.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].agent_id.as_str(), "opencode");
        assert_eq!(requests[0].model.as_deref(), Some("model/a"));

        drop(service);
        let _ = std::fs::remove_dir_all(root);
    }
}
