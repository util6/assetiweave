use super::*;
use crate::backend::ai_execution::{
    executor::BackendFuture, AgentExecutionRuntime, AiExecutionRequest,
};
use crate::backend::conversations::{
    ConversationAdapterRuntimeKind, ConversationAdapterRuntimeStatus,
};
use std::sync::Arc;

struct FakeAgentRuntime;

impl AgentExecutionRuntime for FakeAgentRuntime {
    fn execute<'a>(&'a self, _request: AiExecutionRequest) -> BackendFuture<'a> {
        Box::pin(async { panic!("runtime execution is outside this constructor test") })
    }
}

#[tokio::test]
async fn app_service_accepts_an_injected_agent_runtime() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-runtime-injection-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let runtime: Arc<dyn AgentExecutionRuntime> = Arc::new(FakeAgentRuntime);

    let service = AppService::open_with_db_path_and_runtime(db_path.clone(), runtime.clone())
        .await
        .expect("open service with fake runtime");

    assert!(Arc::ptr_eq(&service.agent_runtime, &runtime));
    drop(service);
    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn default_app_services_use_independent_runtime_snapshots() {
    let first_path = std::env::temp_dir().join(format!(
        "assetiweave-runtime-shared-first-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let second_path = std::env::temp_dir().join(format!(
        "assetiweave-runtime-shared-second-{}.sqlite",
        uuid::Uuid::new_v4()
    ));

    let first = AppService::open_with_db_path(first_path.clone())
        .await
        .expect("first service");
    let second = AppService::open_with_db_path(second_path.clone())
        .await
        .expect("second service");

    let first_runtime = first.agent_runtime.clone();
    let second_runtime = second.agent_runtime.clone();
    assert!(!Arc::ptr_eq(&first_runtime, &second_runtime));
    drop(first);
    drop(second);
    let _ = std::fs::remove_file(first_path);
    let _ = std::fs::remove_file(second_path);
}

#[test]
fn runtime_doctor_ignores_unavailable_unrequired_runtimes() {
    let statuses = vec![
        runtime_status(ConversationAdapterRuntimeKind::Node, false, None),
        runtime_status(ConversationAdapterRuntimeKind::Python, true, Some(">=3.10")),
        runtime_status(ConversationAdapterRuntimeKind::Bash, true, None),
    ];

    let (status, message) = conversation_runtime_doctor_summary(&statuses);

    assert_eq!(status, "pass");
    assert!(message.contains("all required conversation plugin runtimes available"));
    assert!(!message.contains("node runtime missing"));
}

#[test]
fn runtime_doctor_warns_for_unavailable_required_runtimes() {
    let statuses = vec![
        runtime_status(ConversationAdapterRuntimeKind::Node, false, Some(">=20")),
        runtime_status(ConversationAdapterRuntimeKind::Python, true, None),
        runtime_status(ConversationAdapterRuntimeKind::Bash, true, None),
    ];

    let (status, message) = conversation_runtime_doctor_summary(&statuses);

    assert_eq!(status, "warn");
    assert!(message.contains("missing required conversation plugin runtimes"));
    assert!(message.contains("node >=20"));
}

fn runtime_status(
    kind: ConversationAdapterRuntimeKind,
    available: bool,
    required_version: Option<&str>,
) -> ConversationAdapterRuntimeStatus {
    ConversationAdapterRuntimeStatus {
        kind,
        program: "runtime".to_string(),
        available,
        version: None,
        required_version: required_version.map(str::to_string),
        error: None,
        hint: None,
    }
}
