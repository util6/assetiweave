use super::*;
use crate::backend::{
    agents::types::{AgentEnvEntry, AgentId, DeclaredAgentCapabilities},
    ai_execution::{
        AgentSessionMode, AiExecutionCancellation, AiExecutionLimits, AiExecutionProgressSink,
        AiExecutionPurpose, SessionEvent, SessionEventProjection, SessionItemKind,
        SessionItemState,
    },
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

#[test]
fn provider_routing_failure_is_reported_as_an_actionable_model_error() {
    let error = map_acp_error(
            "prompt",
            AcpError::RequestFailed {
                operation: crate::backend::agents::protocol::acp::AcpOperation::Prompt,
                message: "Internal error: Error from provider (Console): Upstream request failed: [404] No allowed providers are available for the selected model.".to_string(),
            },
        );

    let view = error.to_view();
    assert_eq!(view.code, "model_unavailable");
    assert!(view
        .message
        .contains("Choose another model in Agent settings"));
    assert!(view.message.contains("Upstream request failed: [404]"));
    assert!(!view.message.contains("Internal error"));
    assert!(!view.message.contains("Error from provider (Console)"));
}

#[test]
fn memory_generation_mcp_is_injected_with_the_job_lease_binding() {
    let servers = memory_generation_mcp_servers(Some(
        &crate::backend::ai_execution::AiMemoryGenerationTools {
            tenant_id: "tenant-fixture".to_string(),
            job_id: "job-fixture".to_string(),
            ownership_token: "lease-fixture".to_string(),
            database_path: "/tmp/fixture.db".to_string(),
        },
    ))
    .unwrap();
    let serialized = serde_json::to_value(&servers).unwrap();
    let rendered = serialized.to_string();

    assert_eq!(servers.len(), 1);
    assert!(rendered.contains("assetiweave-memory-generation"));
    assert!(rendered.contains("--memory-generation-mcp-stdio"));
    assert!(rendered.contains("ASSETIWEAVE_MEMORY_GENERATION_JOB_ID"));
    assert!(rendered.contains("job-fixture"));
    assert!(rendered.contains("ASSETIWEAVE_MEMORY_GENERATION_OWNERSHIP_TOKEN"));
    assert!(rendered.contains("lease-fixture"));
}

fn definition(mode: &str, record_path: &Path) -> AgentDefinition {
    AgentDefinition {
        id: AgentId::parse("fake-acp").unwrap(),
        installation_id: None,
        display_name: "Fake ACP".to_owned(),
        protocol: AgentProtocol::Acp,
        command: "node".to_owned(),
        args: vec![Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-fixtures/fake-acp-agent.mjs")
            .to_string_lossy()
            .into_owned()],
        env: vec![
            AgentEnvEntry::new("ASSETIWEAVE_FAKE_ACP_MODE", mode),
            AgentEnvEntry::new(
                "ASSETIWEAVE_FAKE_ACP_RECORD_PATH",
                record_path.to_string_lossy(),
            ),
        ],
        declared_capabilities: DeclaredAgentCapabilities::acp_text(),
        availability_probe: None,
        model_discovery: None,
        session_cleanup: None,
        session_cleanup_not_found_markers: Vec::new(),
    }
}

fn definition_with_fallback(mode: &str, record_path: &Path, cleanup_mode: &str) -> AgentDefinition {
    let mut definition = definition(mode, record_path);
    definition.env.push(AgentEnvEntry::new(
        "ASSETIWEAVE_FAKE_SESSION_CLEANUP_MODE",
        cleanup_mode,
    ));
    definition.session_cleanup = Some(crate::backend::agents::types::AgentCommandDefinition::new(
        [
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("test-fixtures/fake-session-cleanup.mjs")
                .to_string_lossy()
                .into_owned(),
            "{session_id}".to_string(),
        ],
    ));
    definition
}

fn request(model: Option<&str>) -> AiExecutionRequest {
    AiExecutionRequest {
        execution_id: Uuid::new_v4().to_string(),
        agent_id: AgentId::parse("fake-acp").unwrap(),
        purpose: AiExecutionPurpose::Translation,
        session_mode: AgentSessionMode::OneShot,
        prompt: "translate fixture".to_owned(),
        model: model.map(str::to_owned),
        limits: AiExecutionLimits {
            initialize_timeout: Duration::from_secs(5),
            config_rpc_timeout: Duration::from_secs(2),
            cancel_grace: Duration::from_millis(500),
            close_timeout: Duration::from_millis(500),
            cleanup_timeout: Duration::from_secs(2),
            text_bytes: 1024,
            stderr_bytes: 1024,
            ..AiExecutionLimits::default()
        },
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
    }
}

fn test_paths(name: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("assetiweave-acp-{name}-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let record = root.join("record.ndjson");
    (root, record)
}

fn records(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

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

#[tokio::test(flavor = "current_thread")]
async fn t03_fake_acp_events_reach_the_typed_session_sink() {
    let (root, record) = test_paths("session-events");
    let backend = AcpExecutionBackend::new(root.join("workspaces"));
    let capture = Arc::new(CaptureSessionEvents::default());
    let mut execution_request = request(None);
    execution_request.purpose = AiExecutionPurpose::Recall;
    execution_request.recall_tools = Some(crate::backend::ai_execution::AiRecallTools {
        tenant_id: "tenant-fixture".to_string(),
        recall_session_id: "recall-fixture".to_string(),
        database_path: root.join("fixture.db").to_string_lossy().into_owned(),
    });
    execution_request.execution_context_key = Some("member-fixture".to_string());
    execution_request.progress = Some(capture.clone());

    backend
        .execute(&definition("session_events", &record), execution_request)
        .await
        .expect("ACP event fixture execution");

    let events = capture.events.lock().unwrap();
    assert!(events.iter().any(|event| matches!(
        event.kind,
        crate::backend::ai_execution::SessionEventKind::AssistantTextDelta { ref text }
            if text == "visible "
    )));
    assert!(events.iter().any(|event| matches!(
        event.kind,
        crate::backend::ai_execution::SessionEventKind::AssistantTextDelta { ref text }
            if text == "answer"
    )));
    assert!(events.iter().any(|event| matches!(
        event.kind,
        crate::backend::ai_execution::SessionEventKind::ThinkingDelta { ref text }
            if text == "provider thought"
    )));
    assert!(events.iter().any(|event| matches!(
        event.kind,
        crate::backend::ai_execution::SessionEventKind::Processing {
            state: SessionProcessingState::Active
        }
    )));
    assert!(!events.iter().any(|event| matches!(
        event.kind,
        crate::backend::ai_execution::SessionEventKind::ThinkingDelta { ref text }
            if text.contains("thought-metadata")
    )));
    assert!(events.iter().any(|event| matches!(
        event.kind,
        crate::backend::ai_execution::SessionEventKind::ToolStart { ref name, .. }
            if name.as_deref() == Some("memory_recall_search")
    )));
    assert!(events.iter().any(|event| matches!(
        event.kind,
        crate::backend::ai_execution::SessionEventKind::ToolResult { success: true, .. }
    )));
    assert!(events.iter().any(|event| matches!(
        event.kind,
        crate::backend::ai_execution::SessionEventKind::ToolUpdate {
            state: crate::backend::ai_execution::SessionToolState::Running,
            ..
        }
    )));
    assert!(events.iter().any(|event| matches!(
        event.kind,
        crate::backend::ai_execution::SessionEventKind::TerminalResult { .. }
    )));
    let event_shapes = events
        .iter()
        .map(|event| match &event.kind {
            crate::backend::ai_execution::SessionEventKind::Processing { state } => match state {
                SessionProcessingState::Started => "processing_started",
                SessionProcessingState::Active => "processing_active",
                SessionProcessingState::Completed => "processing_completed",
            },
            crate::backend::ai_execution::SessionEventKind::AssistantTextDelta { .. } => "text",
            crate::backend::ai_execution::SessionEventKind::ThinkingDelta { .. } => "thinking",
            crate::backend::ai_execution::SessionEventKind::ToolStart { .. } => "tool_start",
            crate::backend::ai_execution::SessionEventKind::ToolUpdate { .. } => "tool_update",
            crate::backend::ai_execution::SessionEventKind::ToolResult { .. } => "tool_result",
            crate::backend::ai_execution::SessionEventKind::TerminalResult { .. } => "terminal",
            _ => "other",
        })
        .collect::<Vec<_>>();
    assert_eq!(
        event_shapes,
        vec![
            "processing_started",
            "text",
            "text",
            "thinking",
            "processing_active",
            "tool_start",
            "tool_update",
            "tool_result",
            "processing_completed",
            "terminal",
        ]
    );
    let execution_id = events[0].identity.execution_id.clone();
    assert!(events
        .iter()
        .all(|event| event.identity.execution_id == execution_id));
    assert!(events
        .windows(2)
        .all(|pair| pair[0].sequence < pair[1].sequence));
    assert!(events.iter().all(|event| {
        event.identity.session_id == "member-fixture"
            && event.identity.member_id == "member-fixture"
            && event.identity.execution_id != ""
            && event.identity.turn_id != ""
            && event.identity.item_id != ""
            && event.identity.event_id != ""
            && matches!(
                event.delivery,
                crate::backend::ai_execution::SessionEventDelivery::Live
            )
    }));
    assert!(events
        .iter()
        .all(|event| event.identity.session_id != "fixture-session"));
    let projection = crate::backend::ai_execution::SessionEventProjection::default();
    for event in events.iter().cloned() {
        assert_eq!(
            projection.apply(event),
            crate::backend::ai_execution::SessionEventApplyResult::Applied
        );
    }
    let snapshot = projection.snapshot();
    assert_eq!(
        snapshot
            .items
            .iter()
            .find(|item| item.kind == crate::backend::ai_execution::SessionItemKind::AssistantText)
            .and_then(|item| item.text.as_deref()),
        Some("visible answer")
    );
    assert!(snapshot.items.iter().any(|item| {
        item.kind == crate::backend::ai_execution::SessionItemKind::Tool
            && item.state == crate::backend::ai_execution::SessionItemState::Succeeded
    }));
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn t03_replay_events_are_marked_replay_and_do_not_prompt_again() {
    let (root, record) = test_paths("session-events-replay");
    let backend = AcpExecutionBackend::new(root.join("workspaces"));
    let context_key = "member-replay-fixture";
    let recall_tools = || crate::backend::ai_execution::AiRecallTools {
        tenant_id: "tenant-fixture".to_string(),
        recall_session_id: "recall-fixture".to_string(),
        database_path: root.join("fixture.db").to_string_lossy().into_owned(),
    };

    let live_capture = Arc::new(CaptureSessionEvents::default());
    let mut live_request = request(None);
    live_request.purpose = AiExecutionPurpose::Recall;
    live_request.session_mode = AgentSessionMode::Persistent;
    live_request.execution_context_key = Some(context_key.to_string());
    live_request.recall_tools = Some(recall_tools());
    live_request.progress = Some(live_capture);
    let binding = backend
        .execute(&definition("session_events", &record), live_request)
        .await
        .expect("live persistent ACP execution")
        .persistent_binding
        .expect("persistent binding");

    let replay_capture = Arc::new(CaptureSessionEvents::default());
    let mut replay_request = request(None);
    replay_request.purpose = AiExecutionPurpose::Recall;
    replay_request.session_mode = AgentSessionMode::Persistent;
    replay_request.execution_context_key = Some(context_key.to_string());
    replay_request.recall_tools = Some(recall_tools());
    replay_request.binding = Some(binding);
    replay_request.replay = true;
    replay_request.progress = Some(replay_capture.clone());
    let replay = backend
        .execute(&definition("session_events", &record), replay_request)
        .await
        .expect("provider history replay");

    assert_eq!(
        replay.replay_text.as_deref(),
        Some("replayed:fixture-history")
    );
    let events = replay_capture.events.lock().unwrap();
    assert!(events.iter().any(|event| matches!(
        event.kind,
        crate::backend::ai_execution::SessionEventKind::ThinkingDelta { ref text }
            if text == "replayed provider thought"
    )));
    assert!(events.iter().any(|event| matches!(
        event.kind,
        crate::backend::ai_execution::SessionEventKind::ToolResult { success: true, .. }
    )));
    assert!(events.iter().all(|event| {
        matches!(
            event.delivery,
            crate::backend::ai_execution::SessionEventDelivery::Replay
        ) && event.identity.session_id == context_key
            && event.identity.member_id == context_key
    }));
    let record_contents = records(&record);
    assert_eq!(record_contents.matches("\"event\":\"prompt\"").count(), 1);
    assert_eq!(record_contents.matches("\"event\":\"load\"").count(), 1);
    let _ = fs::remove_dir_all(root);
}

struct ProjectionSink(SessionEventProjection);
impl AiExecutionProgressSink for ProjectionSink {
    fn set_phase(&self, _phase: crate::backend::ai_execution::AiExecutionPhase) {}
    fn emit_session_event(&self, event: SessionEvent) {
        self.0.apply(event);
    }
}

#[test]
fn t02_acp_bridge_maps_tool_call_lifecycle_with_input_and_output() {
    let projection = SessionEventProjection::default();
    let provider_session_id = SessionId::new("session-1");
    let execution_request = AiExecutionRequest {
        prompt: "run tool".to_string(),
        execution_id: "exec-tool".to_string(),
        execution_context_key: Some("member-1".to_string()),
        progress: Some(Arc::new(ProjectionSink(projection.clone()))),
        ..request(None)
    };
    let mut bridge = AcpSessionEventBridge::new(&execution_request, &provider_session_id);

    let secret = "SECRET_CREDENTIAL";
    // 1. ToolCall start with raw_input
    bridge.emit(&AcpRuntimeEvent::ToolCall {
        session_id: provider_session_id.clone(),
        tool_call_id: "call-1".to_string(),
        title: "fetch_api".to_string(),
        status: AcpToolStatus::Pending,
        raw_input: Some(serde_json::json!({"endpoint": "/api", "token": secret})),
        raw_output: None,
    });

    // 2. ToolCallUpdate in progress
    bridge.emit(&AcpRuntimeEvent::ToolCallUpdate {
        session_id: provider_session_id.clone(),
        tool_call_id: "call-1".to_string(),
        title: None,
        status: Some(AcpToolStatus::InProgress),
        raw_input: None,
        raw_output: None,
    });

    // 3. ToolCallUpdate completed with raw_output
    bridge.emit(&AcpRuntimeEvent::ToolCallUpdate {
        session_id: provider_session_id,
        tool_call_id: "call-1".to_string(),
        title: None,
        status: Some(AcpToolStatus::Completed),
        raw_input: None,
        raw_output: Some(serde_json::json!({"status": 200, "token": secret})),
    });

    let snapshot = projection.snapshot();
    assert_eq!(snapshot.items.len(), 1);
    let item = &snapshot.items[0];
    assert_eq!(item.kind, SessionItemKind::Tool);
    assert_eq!(item.state, SessionItemState::Succeeded);
    assert_eq!(item.tool_name.as_deref(), Some("fetch_api"));
    assert_eq!(item.tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(
        item.tool_input,
        Some(serde_json::json!({"endpoint": "/api", "token": secret}))
    );
    assert_eq!(
        item.tool_output,
        Some(serde_json::json!({"status": 200, "token": secret}))
    );

    let debug_str = format!("{snapshot:?}");
    assert!(!debug_str.contains(secret));
}

#[tokio::test(flavor = "current_thread")]
async fn life_01_happy_stdio_flow_closes_reaps_and_removes_workspace() {
    let (root, record) = test_paths("happy");
    let workspace_root = root.join("workspaces");
    let backend = AcpExecutionBackend::new(workspace_root.clone());

    let result = backend
        .execute(&definition("happy", &record), request(Some("vendor/model")))
        .await
        .expect("happy execution");

    assert_eq!(result.text, "translated");
    assert_eq!(result.requested_model.as_deref(), Some("vendor/model"));
    assert_eq!(result.protocol, AgentProtocol::Acp);
    let record = records(&record);
    assert!(record.contains("\"event\":\"initialize\""));
    assert!(record.contains("\"event\":\"new\""));
    assert!(record.contains("\"mcpCount\":0"));
    assert!(record.contains("\"event\":\"model\""));
    assert!(record.contains("\"event\":\"prompt\""));
    assert!(record.contains("\"event\":\"close\""));
    assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn one_shot_cleanup_deletes_the_session_after_close() {
    let (root, record) = test_paths("one-shot-delete");
    let workspace_root = root.join("workspaces");
    let backend = AcpExecutionBackend::new(workspace_root.clone());

    backend
        .execute(&definition("happy", &record), request(None))
        .await
        .expect("one-shot execution");

    let records = records(&record);
    let close = records.find("\"event\":\"close\"").expect("close record");
    let delete = records.find("\"event\":\"delete\"").expect("delete record");
    assert!(
        close < delete,
        "session/delete must run after session/close"
    );
    assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn one_shot_delete_succeeds_when_close_is_not_advertised() {
    let (root, record) = test_paths("one-shot-no-close");
    let backend = AcpExecutionBackend::new(root.join("workspaces"));

    backend
        .execute(&definition("no_close", &record), request(None))
        .await
        .expect("delete does not require close capability");

    let records = records(&record);
    assert!(!records.contains("\"event\":\"close\""));
    assert!(records.contains("\"event\":\"delete\""));
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn one_shot_standard_delete_success_skips_the_declared_fallback() {
    let (root, record) = test_paths("one-shot-standard-delete");
    let backend = AcpExecutionBackend::new(root.join("workspaces"));

    backend
        .execute(
            &definition_with_fallback("happy", &record, "failure"),
            request(None),
        )
        .await
        .expect("standard delete completes cleanup");

    let records = records(&record);
    assert!(records.contains("\"event\":\"delete\""));
    assert!(!records.contains("\"event\":\"fallback_delete\""));
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn one_shot_standard_delete_treats_a_declared_missing_session_as_deleted() {
    let (root, record) = test_paths("one-shot-standard-not-found");
    let backend = AcpExecutionBackend::new(root.join("workspaces"));
    let mut definition = definition_with_fallback("delete_not_found", &record, "failure");
    definition.session_cleanup_not_found_markers = vec!["Session not found:".to_string()];

    backend
        .execute(&definition, request(None))
        .await
        .expect("already missing is idempotent success");

    let records = records(&record);
    assert!(records.contains("\"event\":\"delete\""));
    assert!(!records.contains("\"event\":\"fallback_delete\""));
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn one_shot_standard_delete_failure_or_timeout_uses_the_declared_fallback() {
    for mode in ["delete_error", "delete_hang"] {
        let (root, record) = test_paths(mode);
        let backend = AcpExecutionBackend::new(root.join("workspaces"));

        backend
            .execute(
                &definition_with_fallback(mode, &record, "success"),
                request(None),
            )
            .await
            .expect("fallback completes cleanup");

        let records = records(&record);
        assert!(records.contains("\"event\":\"delete\""));
        assert!(records.contains("\"event\":\"fallback_delete\""));
        let _ = fs::remove_dir_all(root);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn one_shot_uses_declared_fallback_after_reaping_an_agent_without_standard_delete() {
    let (root, record) = test_paths("one-shot-fallback");
    let workspace_root = root.join("workspaces");
    let backend = AcpExecutionBackend::new(workspace_root.clone());

    let result = backend
        .execute(
            &definition_with_fallback("no_delete", &record, "success"),
            request(None),
        )
        .await
        .expect("successful fallback completes the OneShot task");

    assert_eq!(result.text, "translated");
    let records = records(&record);
    assert!(!records.contains("\"event\":\"delete\""));
    #[cfg(unix)]
    {
        let reaped = records.find("\"event\":\"sigterm\"").expect("reap record");
        let fallback = records
            .find("\"event\":\"fallback_delete\"")
            .expect("fallback record");
        assert!(
            reaped < fallback,
            "fallback must run after ACP process reap"
        );
    }
    #[cfg(not(unix))]
    {
        assert!(records.contains("\"event\":\"fallback_delete\""));
    }
    assert!(records.contains("\"sessionId\":\"fixture-session\""));
    assert!(records.contains("\"originalProcessReaped\":true"));
    assert!(records.contains("\"workspaceExists\":true"));
    assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn one_shot_allows_a_slow_provider_fallback_to_finish_after_the_acp_close_timeout() {
    let (root, record) = test_paths("one-shot-slow-fallback");
    let workspace_root = root.join("workspaces");
    let backend = AcpExecutionBackend::new(workspace_root.clone());

    let result = backend
        .execute(
            &definition_with_fallback("no_delete", &record, "slow_success"),
            request(None),
        )
        .await;

    assert!(
        result.is_ok(),
        "a completed prompt must survive a slow session deletion: {result:?}"
    );
    assert!(records(&record).contains("\"event\":\"fallback_delete\""));
    assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn one_shot_fallback_failure_preserves_successful_execution_and_reports_cleanup_failure() {
    for cleanup_mode in ["failure", "timeout"] {
        let (root, record) = test_paths(cleanup_mode);
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());

        let result = backend
            .execute(
                &definition_with_fallback("no_delete", &record, cleanup_mode),
                request(None),
            )
            .await;
        assert!(
            matches!(
                result,
                Ok(ref execution) if matches!(execution.session_cleanup, SessionCleanupStatus::Failed(_))
            ),
            "unexpected result: {result:?}"
        );
        assert!(records(&record).contains("\"event\":\"fallback_delete\""));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn one_shot_unsupported_delete_preserves_successful_execution_and_reports_unsupported() {
    let (root, record) = test_paths("one-shot-unsupported");
    let workspace_root = root.join("workspaces");
    let backend = AcpExecutionBackend::new(workspace_root.clone());

    let result = backend
        .execute(&definition("no_delete", &record), request(None))
        .await;
    assert!(
        matches!(
            result,
            Ok(ref execution) if execution.session_cleanup == SessionCleanupStatus::Unsupported
        ),
        "unexpected result: {result:?}"
    );
    assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn one_shot_execution_failure_remains_primary_when_fallback_also_fails() {
    let (root, record) = test_paths("one-shot-primary-failure");
    let backend = AcpExecutionBackend::new(root.join("workspaces"));

    let result = backend
        .execute(
            &definition_with_fallback("no_delete_empty", &record, "failure"),
            request(None),
        )
        .await;

    assert!(matches!(result, Err(AiExecutionError::EmptyOutput { .. })));
    assert!(records(&record).contains("\"event\":\"fallback_delete\""));
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn one_shot_fallback_treats_a_declared_missing_session_marker_as_deleted() {
    let (root, record) = test_paths("one-shot-fallback-not-found");
    let workspace_root = root.join("workspaces");
    let backend = AcpExecutionBackend::new(workspace_root.clone());
    let mut definition = definition_with_fallback("no_delete", &record, "not_found");
    definition.session_cleanup_not_found_markers = vec!["Session not found:".to_string()];

    let result = backend.execute(&definition, request(None)).await;

    assert!(result.is_ok());
    assert!(records(&record).contains("\"event\":\"fallback_delete\""));
    assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn persistent_mode_creates_a_stable_binding_and_reuses_resume() {
    let (root, record) = test_paths("persistent-resume");
    let workspace_root = root.join("workspaces");
    let backend = AcpExecutionBackend::new(workspace_root.clone());
    let definition = definition("happy", &record);
    let mut first_request = request(None);
    first_request.session_mode = AgentSessionMode::Persistent;
    first_request.tenant_id = Some("tenant-fixture".to_string());
    first_request.execution_context_key = Some("team-member-fixture".to_string());
    let first = backend
        .execute(&definition, first_request)
        .await
        .expect("first persistent execution");
    let binding = first
        .persistent_binding
        .clone()
        .expect("first execution returns a binding");
    assert!(Path::new(&binding.workspace_path).is_dir());

    let mut second_request = request(None);
    second_request.session_mode = AgentSessionMode::Persistent;
    second_request.tenant_id = Some("tenant-fixture".to_string());
    second_request.execution_context_key = Some("team-member-fixture".to_string());
    second_request.binding = Some(binding.clone());
    backend
        .execute(&definition, second_request)
        .await
        .expect("resumed persistent execution");

    let mut replay_request = request(None);
    replay_request.session_mode = AgentSessionMode::Persistent;
    replay_request.tenant_id = Some("tenant-fixture".to_string());
    replay_request.execution_context_key = Some("team-member-fixture".to_string());
    replay_request.binding = Some(binding);
    replay_request.replay = true;
    let replay = backend
        .execute(&definition, replay_request)
        .await
        .expect("persistent history replay");
    assert_eq!(
        replay.replay_text.as_deref(),
        Some("replayed:fixture-history")
    );

    let records = records(&record);
    assert_eq!(records.matches("\"event\":\"new\"").count(), 1);
    assert_eq!(records.matches("\"event\":\"resume\"").count(), 1);
    assert_eq!(records.matches("\"event\":\"load\"").count(), 1);
    assert_eq!(records.matches("\"event\":\"prompt\"").count(), 2);
    assert!(!records.contains("\"event\":\"close\""));
    assert!(!records.contains("\"event\":\"delete\""));
    assert!(Path::new(&first.persistent_binding.unwrap().workspace_path).is_dir());
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn life_02_connection_probe_performs_initialize_and_session_new_without_prompt() {
    let (root, record) = test_paths("connection-probe");
    let workspace_root = root.join("workspaces");
    let backend = AcpExecutionBackend::new(workspace_root.clone());

    backend
        .check_connection(&definition("happy", &record))
        .await
        .expect("ACP connection probe");

    let records = records(&record);
    assert!(records.contains("\"event\":\"initialize\""));
    assert!(records.contains("\"event\":\"new\""));
    assert!(records.contains("\"event\":\"close\""));
    assert!(!records.contains("\"event\":\"prompt\""));
    assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn connection_probe_accepts_declared_delete_not_found_from_error_data() {
    let (root, record) = test_paths("connection-probe-delete-not-found-data");
    let workspace_root = root.join("workspaces");
    let backend = AcpExecutionBackend::new(workspace_root.clone());
    let mut agent = definition("delete_not_found_in_data", &record);
    agent.session_cleanup_not_found_markers =
        vec!["Internal error: no rollout found for thread id".to_string()];

    backend
        .check_connection(&agent)
        .await
        .expect("an absent empty probe session is already deleted");

    let records = records(&record);
    assert!(records.contains("\"event\":\"delete\""));
    assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn life_03_connection_probe_succeeds_even_with_empty_model_list() {
    let (root, record) = test_paths("connection-probe-no-models");
    let workspace_root = root.join("workspaces");
    let backend = AcpExecutionBackend::new(workspace_root.clone());

    backend
        .check_connection(&definition("no_models", &record))
        .await
        .expect("ACP connection probe succeeds even when model list is empty");

    let records = records(&record);
    assert!(records.contains("\"event\":\"initialize\""));
    assert!(records.contains("\"event\":\"new\""));
    assert!(records.contains("\"event\":\"close\""));
    assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn life_06_connection_probe_succeeds_when_delete_is_unsupported() {
    let (root, record) = test_paths("connection-probe-no-delete");
    let workspace_root = root.join("workspaces");
    let backend = AcpExecutionBackend::new(workspace_root.clone());

    backend
        .check_connection(&definition("no_delete", &record))
        .await
        .expect("ACP connection probe succeeds when delete is unsupported");

    let records = records(&record);
    assert!(records.contains("\"event\":\"initialize\""));
    assert!(records.contains("\"event\":\"new\""));
    assert!(records.contains("\"event\":\"close\""));
    assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn life_04_model_discovery_reads_session_config_options_without_prompt() {
    let (root, record) = test_paths("model-discovery");
    let workspace_root = root.join("workspaces");
    let backend = AcpExecutionBackend::new(workspace_root.clone());

    let (models, current_model_id) = backend
        .discover_models(&definition("happy", &record))
        .await
        .expect("ACP model discovery");

    assert_eq!(current_model_id.as_deref(), Some("fixture/model-fast"));
    assert_eq!(models[0].id, "fixture/model-fast");
    assert_eq!(models[0].label, "Fixture Fast");
    assert_eq!(models[0].description.as_deref(), Some("Fast fixture model"));
    assert!(records(&record).contains("\"event\":\"new\""));
    assert!(!records(&record).contains("\"event\":\"prompt\""));
    assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn life_05_model_discovery_rejects_an_empty_model_list() {
    let (root, record) = test_paths("model-discovery-no-models");
    let workspace_root = root.join("workspaces");
    let backend = AcpExecutionBackend::new(workspace_root.clone());

    let error = backend
        .discover_models(&definition("no_models", &record))
        .await
        .expect_err("empty model discovery must fail health checks");

    assert!(matches!(
        error,
        AiExecutionError::Protocol {
            operation: "session_model_catalog_empty"
        }
    ));
    assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn chunked_wrong_session_thinking_and_late_chunk_modes_are_aggregated() {
    for (mode, expected) in [
        ("chunked", "你好🌍"),
        ("wrong_session", "right"),
        ("thinking", "visible"),
        ("late_chunk", "before late"),
    ] {
        let (root, record) = test_paths(mode);
        let backend = AcpExecutionBackend::new(root.join("workspaces"));
        let result = backend
            .execute(&definition(mode, &record), request(None))
            .await
            .unwrap();
        assert_eq!(result.text, expected, "mode {mode}");
        let _ = fs::remove_dir_all(root);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn init_new_and_model_failures_still_cleanup_and_model_never_prompts() {
    for mode in [
        "initialize_error",
        "initialize_timeout",
        "new_error",
        "model_reject",
        "model_timeout",
    ] {
        let (root, record) = test_paths(mode);
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());
        let result = backend
            .execute(&definition(mode, &record), request(Some("vendor/model")))
            .await;
        assert!(result.is_err(), "mode {mode}");
        if mode.starts_with("model_") {
            assert!(!records(&record).contains("\"event\":\"prompt\""));
        }
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn permission_tool_empty_and_output_limit_fail_closed_and_cleanup() {
    for mode in ["permission", "tool_call", "empty", "oversized"] {
        let (root, record) = test_paths(mode);
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());
        let mut execution_request = request(None);
        execution_request.limits.text_bytes = 64;
        let result = backend
            .execute(&definition(mode, &record), execution_request)
            .await;
        match mode {
            "permission" => assert!(matches!(result, Err(AiExecutionError::PermissionDenied))),
            "tool_call" => assert!(matches!(result, Err(AiExecutionError::ToolUseDenied))),
            "empty" => assert!(matches!(result, Err(AiExecutionError::EmptyOutput { .. }))),
            "oversized" => assert!(matches!(result, Err(AiExecutionError::OutputLimit { .. }))),
            _ => unreachable!(),
        }
        if matches!(mode, "permission" | "tool_call" | "oversized") {
            assert!(records(&record).contains("\"event\":\"cancel\""));
        }
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn user_cancel_reaches_in_flight_agent_before_terminal_cleanup() {
    let (root, record) = test_paths("cancel");
    let workspace_root = root.join("workspaces");
    let backend = AcpExecutionBackend::new(workspace_root.clone());
    let execution_request = request(None);
    let cancellation = execution_request.cancellation.clone();
    let definition = definition("cancel_wait", &record);

    let execution = backend.execute(&definition, execution_request);
    tokio::pin!(execution);
    loop {
        tokio::select! {
            result = &mut execution => panic!("execution completed before cancellation: {result:?}"),
            _ = async {
                if records(&record).contains("\"event\":\"prompt\"") {
                    return;
                }
                tokio::task::yield_now().await;
            } => {
                if records(&record).contains("\"event\":\"prompt\"") {
                    break;
                }
            }
        }
    }
    cancellation.cancel();
    let result = tokio::time::timeout(Duration::from_secs(2), &mut execution)
        .await
        .expect("cancel cleanup timeout");

    assert!(matches!(result, Err(AiExecutionError::Cancelled { .. })));
    assert!(records(&record).contains("\"event\":\"cancel\""));
    assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn total_timeout_cancels_in_flight_prompt_and_waits_for_cleanup() {
    let (root, record) = test_paths("total-timeout");
    let workspace_root = root.join("workspaces");
    let backend = AcpExecutionBackend::new(workspace_root.clone());
    let mut execution_request = request(None);
    execution_request.limits.total_timeout = Duration::from_millis(500);

    let result = backend
        .execute(&definition("cancel_wait", &record), execution_request)
        .await;

    assert!(matches!(result, Err(AiExecutionError::Timeout { .. })));
    assert!(records(&record).contains("\"event\":\"cancel\""));
    assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn close_failure_never_returns_success_and_process_is_reaped() {
    for mode in ["close_error", "close_hang"] {
        let (root, record) = test_paths(mode);
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());
        let result = backend
            .execute(&definition(mode, &record), request(None))
            .await;

        assert!(matches!(
            result,
            Err(AiExecutionError::CleanupFailed { .. })
        ));
        assert!(records(&record).contains("\"event\":\"delete\""));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn disconnect_and_process_exit_are_classified_and_cleaned() {
    for mode in ["disconnect", "exit_during_prompt"] {
        let (root, record) = test_paths(mode);
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());
        let result = backend
            .execute(&definition(mode, &record), request(None))
            .await;

        assert!(matches!(
            result,
            Err(AiExecutionError::Protocol { .. }
                | AiExecutionError::ProtocolDetail { .. }
                | AiExecutionError::AgentExited { .. })
        ));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn cleanup_is_idempotent() {
    let (root, _record) = test_paths("cleanup-repeat");
    let workspace = create_workspace(&root.join("workspaces")).unwrap();
    let mut guard = AcpExecutionGuard::new(workspace);
    let execution_request = request(None);

    let definition = definition("happy", &_record);
    let first = guard.cleanup(false, &execution_request, &definition).await;
    let second = guard.cleanup(false, &execution_request, &definition).await;

    assert!(first.workspace_removed);
    assert!(second.workspace_removed);
    assert!(second.process_reaped);
    assert!(second.failures.is_empty());
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn test_probe_connection_and_models_report_complete_stages() {
    // Happy path
    {
        let (root, record) = test_paths("probe-happy");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root);
        let def = definition("happy", &record);
        let report = backend
            .probe_connection_and_models(&def, AiExecutionCancellation::default())
            .await;

        assert_eq!(
            report.protocol_connection,
            AcpProtocolConnectionOutcome::Connected
        );
        match &report.model_discovery {
            AcpModelDiscoveryOutcome::Success {
                models,
                current_model_id,
            } => {
                assert!(!models.is_empty());
                assert_eq!(current_model_id.as_deref(), Some("fixture/model-fast"));
            }
            other => panic!("expected model discovery success, got {:?}", other),
        }
        assert!(report.cleanup.process_reaped);
        assert!(report.cleanup.workspace_removed);
        assert!(!report.cleanup.timed_out);
        assert!(report.timings.total_duration_ms > 0);

        let conn_res = report.to_connection_result(def.id.as_str(), Some("v1"), None, None, None);
        assert!(conn_res.available);
        assert!(conn_res.connected);
        assert_eq!(conn_res.error_code, None);

        let models_res = report.to_models_result(def.id.as_str());
        assert!(models_res.available);
        assert_eq!(models_res.models.len(), 2);
        assert_eq!(models_res.error_code, None);
        let _ = fs::remove_dir_all(root);
    }

    // Init error path: model discovery skipped
    {
        let (root, record) = test_paths("probe-init-error");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root);
        let def = definition("initialize_error", &record);
        let report = backend
            .probe_connection_and_models(&def, AiExecutionCancellation::default())
            .await;

        match &report.protocol_connection {
            AcpProtocolConnectionOutcome::Failed {
                stage, error_code, ..
            } => {
                assert_eq!(*stage, AcpConnectionStage::Initialize);
                assert_eq!(error_code, "connection_failed");
            }
            other => panic!("expected failed initialize connection, got {:?}", other),
        }
        assert_eq!(report.model_discovery, AcpModelDiscoveryOutcome::Skipped);
        assert!(report.cleanup.process_reaped);
        assert!(report.cleanup.workspace_removed);

        let conn_res = report.to_connection_result(def.id.as_str(), None, None, None, None);
        assert!(!conn_res.available);
        assert!(!conn_res.connected);
        assert_eq!(conn_res.error_code.as_deref(), Some("connection_failed"));

        let models_res = report.to_models_result(def.id.as_str());
        assert!(!models_res.available);
        assert!(models_res.models.is_empty());
        assert_eq!(models_res.error_code.as_deref(), Some("connection_failed"));
        let _ = fs::remove_dir_all(root);
    }

    // Empty models path: protocol connected, models empty
    {
        let (root, record) = test_paths("probe-empty-models");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root);
        let def = definition("empty_model_options", &record);
        let report = backend
            .probe_connection_and_models(&def, AiExecutionCancellation::default())
            .await;

        assert_eq!(
            report.protocol_connection,
            AcpProtocolConnectionOutcome::Connected
        );
        assert_eq!(report.model_discovery, AcpModelDiscoveryOutcome::Empty);

        let conn_res = report.to_connection_result(def.id.as_str(), None, None, None, None);
        assert!(conn_res.available);
        assert!(conn_res.connected);

        let models_res = report.to_models_result(def.id.as_str());
        assert!(models_res.available);
        assert!(models_res.models.is_empty());
        assert_eq!(models_res.error_code.as_deref(), Some("model_list_empty"));
        let _ = fs::remove_dir_all(root);
    }
}
