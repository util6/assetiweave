use super::*;
use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::backend::agents::types::{AgentEnvEntry, AgentId, DeclaredAgentCapabilities};
use crate::backend::ai_execution::{
    AiExecutionCancellation, AiExecutionLimits, AiExecutionProgressSink, AiExecutionPurpose,
};

#[derive(Default)]
struct CaptureSessionEvents {
    events: Mutex<Vec<SessionEvent>>,
}

impl AiExecutionProgressSink for CaptureSessionEvents {
    fn set_phase(&self, _phase: AiExecutionPhase) {}

    fn emit_session_event(&self, event: SessionEvent) {
        self.events.lock().unwrap().push(event);
    }
}

fn definition(record: &Path) -> AgentDefinition {
    definition_with_mode(record, None)
}

fn definition_with_mode(record: &Path, mode: Option<&str>) -> AgentDefinition {
    let mut env = vec![AgentEnvEntry::new(
        "ASSETIWEAVE_FAKE_AGY_RECORD_PATH",
        record.to_string_lossy(),
    )];
    if let Some(mode) = mode {
        env.push(AgentEnvEntry::new("ASSETIWEAVE_FAKE_AGY_MODE", mode));
    }
    AgentDefinition {
        id: AgentId::parse("antigravity").unwrap(),
        installation_id: Some("fixture-installation".to_string()),
        display_name: "Antigravity Fixture".to_string(),
        protocol: AgentProtocol::Native,
        command: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-fixtures/fake-antigravity-agent")
            .to_string_lossy()
            .into_owned(),
        args: Vec::new(),
        env,
        declared_capabilities: DeclaredAgentCapabilities {
            text_prompt: true,
            resume: true,
            live_events: true,
            ..DeclaredAgentCapabilities::default()
        },
        availability_probe: None,
        model_discovery: None,
        session_cleanup: None,
        session_cleanup_not_found_markers: Vec::new(),
    }
}

fn request(definition: &AgentDefinition, execution_id: &str) -> AiExecutionRequest {
    AiExecutionRequest {
        execution_id: execution_id.to_string(),
        agent_id: definition.id.clone(),
        purpose: AiExecutionPurpose::TeamTask,
        session_mode: crate::backend::ai_execution::AgentSessionMode::Persistent,
        prompt: "fixture prompt".to_string(),
        model: Some("fixture-model".to_string()),
        limits: AiExecutionLimits::default(),
        cancellation: AiExecutionCancellation::default(),
        progress: None,
        tenant_id: Some("tenant-fixture".to_string()),
        execution_context_key: Some("member-context".to_string()),
        binding: None,
        replay: false,
        restore_only: false,
        team_tools: None,
        recall_tools: None,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn real_conversation_id_is_captured_and_reused_for_each_turn() {
    let root = std::env::temp_dir().join(format!("assetiweave-agy-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let record = root.join("argv.ndjson");
    let definition = definition(&record);
    let backend = crate::backend::ai_execution::backends::native::NativeExecutionBackend::new(
        root.join("workspaces"),
    );

    let first = backend
        .execute(&definition, request(&definition, "agy-first"))
        .await
        .expect("first Antigravity turn");
    let binding = first.persistent_binding.clone().expect("real binding");
    assert_eq!(binding.provider_session_id, "REAL_CONVERSATION_ID");
    assert!(!binding.provider_session_id.starts_with("native-session-"));

    let mut second_request = request(&definition, "agy-second");
    second_request.binding = Some(binding.clone());
    let second = backend
        .execute(&definition, second_request)
        .await
        .expect("resumed Antigravity turn");
    assert_eq!(
        second.persistent_binding.unwrap().provider_session_id,
        "REAL_CONVERSATION_ID"
    );
    let contents = fs::read_to_string(&record).unwrap();
    let records = contents
        .split("--END--\n")
        .filter(|record| !record.is_empty())
        .map(|record| record.lines().map(str::to_string).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 2);
    assert!(records[0]
        .windows(2)
        .any(|pair| pair == ["--new-project", "--add-dir"]));
    assert!(records[1]
        .windows(2)
        .any(|pair| pair == ["--conversation", "REAL_CONVERSATION_ID"]));
    assert!(!records[1]
        .iter()
        .any(|arg| arg.starts_with("native-session-")));
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn stream_json_is_projected_without_tool_payload_or_hidden_thought() {
    let root =
        std::env::temp_dir().join(format!("assetiweave-agy-events-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let record = root.join("argv.ndjson");
    let definition = definition(&record);
    let capture = Arc::new(CaptureSessionEvents::default());
    let mut execution_request = request(&definition, "agy-events");
    execution_request.progress = Some(capture.clone());
    execution_request.session_mode = crate::backend::ai_execution::AgentSessionMode::OneShot;
    execution_request.tenant_id = None;
    execution_request.execution_context_key = None;

    let backend = crate::backend::ai_execution::backends::native::NativeExecutionBackend::new(
        root.join("workspaces"),
    );
    backend
        .execute(&definition, execution_request)
        .await
        .expect("stream-json event execution");

    let events = capture.events.lock().unwrap();
    let shapes = events
        .iter()
        .map(|event| match &event.kind {
            SessionEventKind::Processing {
                state: SessionProcessingState::Started,
            } => "processing_started",
            SessionEventKind::AssistantTextDelta { .. } => "text",
            SessionEventKind::ToolStart { .. } => "tool_start",
            SessionEventKind::ToolUpdate {
                state: SessionToolState::Running,
                ..
            } => "tool_running",
            SessionEventKind::ToolResult { success: true, .. } => "tool_result",
            SessionEventKind::Processing {
                state: SessionProcessingState::Completed,
            } => "processing_completed",
            SessionEventKind::TerminalResult { .. } => "terminal",
            _ => "other",
        })
        .collect::<Vec<_>>();
    assert_eq!(
        shapes,
        vec![
            "processing_started",
            "text",
            "tool_start",
            "tool_running",
            "tool_result",
            "processing_completed",
            "terminal",
        ]
    );
    let tool_item_ids = events
        .iter()
        .filter_map(|event| match event.kind {
            SessionEventKind::ToolStart { .. }
            | SessionEventKind::ToolUpdate { .. }
            | SessionEventKind::ToolResult { .. } => Some(event.identity.item_id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        tool_item_ids,
        vec![
            "agy:REAL_CONVERSATION_ID:step:3".to_string(),
            "agy:REAL_CONVERSATION_ID:step:3".to_string(),
            "agy:REAL_CONVERSATION_ID:step:3".to_string(),
        ]
    );
    assert!(!format!("{events:?}").contains("RAW_TOOL_SECRET"));
    assert!(!format!("{events:?}").contains("thinking_tokens"));
    assert!(events.iter().all(|event| {
        event.identity.execution_id == "agy-events"
            && event.identity.member_id == "execution:agy-events"
            && matches!(event.delivery, SessionEventDelivery::Live)
    }));
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn result_only_conversation_id_becomes_the_persistent_anchor() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-agy-result-id-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).unwrap();
    let record = root.join("argv.ndjson");
    let definition = definition_with_mode(&record, Some("result-id-only"));
    let backend = crate::backend::ai_execution::backends::native::NativeExecutionBackend::new(
        root.join("workspaces"),
    );

    let result = backend
        .execute(&definition, request(&definition, "agy-result-id"))
        .await
        .expect("result Conversation ID");
    assert_eq!(
        result.persistent_binding.unwrap().provider_session_id,
        "RESULT_ONLY_CONVERSATION_ID"
    );
    assert!(!fs::read_to_string(&record)
        .unwrap()
        .contains("native-session-"));
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn authentication_failure_is_controlled_and_creates_no_binding() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-agy-auth-failure-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).unwrap();
    let record = root.join("argv.ndjson");
    let definition = definition_with_mode(&record, Some("auth-failure"));
    let backend = crate::backend::ai_execution::backends::native::NativeExecutionBackend::new(
        root.join("workspaces"),
    );

    let error = backend
        .execute(&definition, request(&definition, "agy-auth-failure"))
        .await
        .expect_err("authentication failure");
    assert!(
        matches!(error, AiExecutionError::Output { message } if message.contains("authentication failed") && !message.contains("or timed out"))
    );
    assert!(!fs::read_to_string(&record)
        .unwrap()
        .contains("native-session-"));
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn failed_empty_id_turn_keeps_the_existing_real_anchor() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-agy-anchor-retain-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).unwrap();
    let record = root.join("argv.ndjson");
    let success_definition = definition(&record);
    let backend = crate::backend::ai_execution::backends::native::NativeExecutionBackend::new(
        root.join("workspaces"),
    );
    let existing_binding = backend
        .execute(
            &success_definition,
            request(&success_definition, "agy-anchor-first"),
        )
        .await
        .expect("initial real anchor")
        .persistent_binding
        .expect("initial binding");

    let failure_definition = definition_with_mode(&record, Some("auth-failure"));
    let mut failure_request = request(&failure_definition, "agy-anchor-failure");
    failure_request.binding = Some(existing_binding.clone());
    let error = backend
        .execute(&failure_definition, failure_request)
        .await
        .expect_err("failed turn with empty provider id");
    assert!(matches!(error, AiExecutionError::Output { .. }));
    assert_eq!(existing_binding.provider_session_id, "REAL_CONVERSATION_ID");
    assert!(!existing_binding
        .provider_session_id
        .starts_with("native-session-"));
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn executor_does_not_overwrite_store_after_failed_empty_id_turn() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-agy-store-retain-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).unwrap();
    let record = root.join("argv.ndjson");
    let database_path = root.join("bindings.sqlite");
    let database =
        tokio::task::spawn_blocking(move || crate::backend::store::Database::open(&database_path))
            .await
            .expect("database open task")
            .expect("temporary provider store");
    let store = Arc::new(crate::backend::ai_execution::PersistentBindingStore::new(
        database.pool().clone(),
    ));
    let workspace = root.join("persisted-workspace");
    fs::create_dir_all(&workspace).unwrap();
    let existing_binding = crate::backend::ai_execution::PersistentExecutionBinding {
        tenant_id: "tenant-fixture".to_string(),
        execution_context_key: "member-context".to_string(),
        provider_session_id: "REAL_CONVERSATION_ID".to_string(),
        agent_id: "antigravity".to_string(),
        installation_id: Some("fixture-installation".to_string()),
        model: Some("fixture-model".to_string()),
        workspace_path: workspace.to_string_lossy().into_owned(),
        binding_version: 1,
        provider_metadata_json: "{\"protocol\":\"native\",\"adapter\":\"antigravity-direct-cli\"}"
            .to_string(),
    };
    store.save(&existing_binding).await.expect("seed binding");

    let definition = definition_with_mode(&record, Some("auth-failure"));
    let registry =
        crate::backend::agents::registry::AgentRegistry::from_definitions([definition.clone()])
            .expect("fixture registry");
    let registry =
        crate::backend::agents::registry::AgentRegistryHandle::from_registry(Arc::new(registry));
    let backend = crate::backend::ai_execution::backends::native::NativeExecutionBackend::new(
        root.join("workspaces"),
    );
    let executor =
        crate::backend::ai_execution::executor::AgentExecutor::with_registry_handle_and_bindings(
            registry,
            Arc::new(
                crate::backend::ai_execution::backends::acp::AcpExecutionBackend::new(
                    root.join("workspaces"),
                ),
            ),
            Arc::new(backend),
            1,
            store.clone(),
        );

    let error = executor
        .execute(request(&definition, "agy-store-failure"))
        .await
        .expect_err("failed turn");
    assert!(matches!(error, AiExecutionError::Output { .. }));
    let persisted = store
        .load("tenant-fixture", "member-context")
        .await
        .expect("load persisted binding")
        .expect("binding remains present");
    assert_eq!(
        persisted.provider_session_id, "REAL_CONVERSATION_ID",
        "failed empty-id turn must not replace the previous anchor"
    );
    drop(executor);
    drop(store);
    tokio::task::spawn_blocking(move || drop(database))
        .await
        .expect("database close task");
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_and_malformed_provider_events_have_controlled_outcomes() {
    let unknown_root =
        std::env::temp_dir().join(format!("assetiweave-agy-unknown-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&unknown_root).unwrap();
    let unknown_record = unknown_root.join("argv.ndjson");
    let unknown_definition = definition_with_mode(&unknown_record, Some("unknown"));
    let capture = Arc::new(CaptureSessionEvents::default());
    let mut unknown_request = request(&unknown_definition, "agy-unknown");
    unknown_request.progress = Some(capture.clone());
    unknown_request.session_mode = crate::backend::ai_execution::AgentSessionMode::OneShot;
    crate::backend::ai_execution::backends::native::NativeExecutionBackend::new(
        unknown_root.join("workspaces"),
    )
    .execute(&unknown_definition, unknown_request)
    .await
    .expect("unknown events are non-fatal");
    assert!(capture.events.lock().unwrap().iter().any(|event| {
        matches!(
            &event.kind,
            SessionEventKind::Notice { code, .. }
                if code == UNKNOWN_EVENT_CODE
        )
    }));
    let _ = fs::remove_dir_all(unknown_root);

    let malformed_root = std::env::temp_dir().join(format!(
        "assetiweave-agy-malformed-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&malformed_root).unwrap();
    let malformed_record = malformed_root.join("argv.ndjson");
    let malformed_definition = definition_with_mode(&malformed_record, Some("malformed"));
    let malformed_capture = Arc::new(CaptureSessionEvents::default());
    let mut malformed_request = request(&malformed_definition, "agy-malformed");
    malformed_request.progress = Some(malformed_capture.clone());
    malformed_request.session_mode = crate::backend::ai_execution::AgentSessionMode::OneShot;
    let error = crate::backend::ai_execution::backends::native::NativeExecutionBackend::new(
        malformed_root.join("workspaces"),
    )
    .execute(&malformed_definition, malformed_request)
    .await
    .expect_err("malformed event");
    assert!(matches!(error, AiExecutionError::Output { .. }));
    assert!(malformed_capture
        .events
        .lock()
        .unwrap()
        .iter()
        .any(|event| {
            matches!(
                &event.kind,
                SessionEventKind::Error { code, .. }
                    if code == MALFORMED_EVENT_CODE
            )
        }));
    let _ = fs::remove_dir_all(malformed_root);
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_and_timeout_reap_the_one_turn_process() {
    let cancel_root =
        std::env::temp_dir().join(format!("assetiweave-agy-cancel-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&cancel_root).unwrap();
    let cancel_record = cancel_root.join("argv.ndjson");
    let cancel_definition = definition_with_mode(&cancel_record, Some("hang"));
    let cancellation = AiExecutionCancellation::default();
    let mut cancel_request = request(&cancel_definition, "agy-cancel");
    cancel_request.cancellation = cancellation.clone();
    let cancel_backend =
        crate::backend::ai_execution::backends::native::NativeExecutionBackend::new(
            cancel_root.join("workspaces"),
        );
    let cancel_execution = cancel_backend.execute(&cancel_definition, cancel_request);
    tokio::pin!(cancel_execution);
    let mut process_started = false;
    for _ in 0..100 {
        if cancel_record.exists() {
            process_started = true;
            break;
        }
        tokio::select! {
            result = &mut cancel_execution => panic!("cancel fixture exited before cancellation: {result:?}"),
            _ = tokio::time::sleep(Duration::from_millis(5)) => {}
        }
    }
    assert!(process_started, "fixture process did not start");
    cancellation.cancel();
    assert!(matches!(
        cancel_execution.await.expect_err("cancelled turn"),
        AiExecutionError::Cancelled { .. }
    ));
    let _ = fs::remove_dir_all(cancel_root);

    let timeout_root =
        std::env::temp_dir().join(format!("assetiweave-agy-timeout-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&timeout_root).unwrap();
    let timeout_record = timeout_root.join("argv.ndjson");
    let timeout_definition = definition_with_mode(&timeout_record, Some("hang"));
    let mut timeout_request = request(&timeout_definition, "agy-timeout");
    timeout_request.limits.total_timeout = Duration::from_millis(100);
    let error = crate::backend::ai_execution::backends::native::NativeExecutionBackend::new(
        timeout_root.join("workspaces"),
    )
    .execute(&timeout_definition, timeout_request)
    .await
    .expect_err("timed out turn");
    assert!(matches!(error, AiExecutionError::Timeout { .. }));
    let _ = fs::remove_dir_all(timeout_root);
}

#[test]
fn empty_and_synthetic_anchors_are_not_valid_resume_inputs() {
    assert!(!is_valid_resume_anchor(""));
    assert!(!is_valid_resume_anchor("native-session-legacy"));
    assert!(is_valid_resume_anchor("REAL_CONVERSATION_ID"));
}

#[test]
fn malformed_and_unknown_lines_are_controlled() {
    assert!(parse_line(br#"{not-json"#).is_err());
    assert!(matches!(
        parse_line(br#"{"event":"future_event"}"#).unwrap(),
        Some(AgyEvent::Unknown)
    ));
    assert!(parse_line(b"provider notice").unwrap().is_none());
}
