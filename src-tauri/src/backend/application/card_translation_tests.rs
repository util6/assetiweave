use super::*;
use crate::backend::{
    agents::types::{AgentId, AgentProtocol},
    ai_execution::{
        executor::BackendFuture, AgentExecutionRuntime, AiExecutionRequest, AiExecutionResult,
    },
    card_translation::{
        ConversationTranslationCli, ConversationTranslationProvider, ConversationTranslationRequest,
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
                persistent_binding: None,
                replay_text: None,
                session_cleanup: crate::backend::ai_execution::SessionCleanupStatus::Deleted,
            })
        })
    }
}

#[tokio::test]
async fn app_service_translation_uses_the_injected_runtime() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-translation-service-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let runtime = Arc::new(FakeRuntime {
        requests: Mutex::new(Vec::new()),
    });
    let service = AppService::open_with_db_path_and_runtime(root.join("app.db"), runtime.clone())
        .await
        .unwrap();

    let result = service
        .translate_conversation_card(ConversationTranslationRequest {
            agent_id: None,
            provider: ConversationTranslationProvider::Cli,
            cli: ConversationTranslationCli::Opencode,
            model: "model/a".to_string(),
            prompt: "translate".to_string(),
        })
        .await
        .unwrap();

    assert_eq!(result.translated_text, "译文");
    let requests = runtime.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].agent_id.as_str(), "opencode");
    assert_eq!(requests[0].model.as_deref(), Some("model/a"));

    drop(service);
    let _ = std::fs::remove_dir_all(root);
}
