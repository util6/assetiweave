use super::*;
use crate::backend::agents::types::AgentId;
use std::sync::{Arc, Mutex};

#[test]
fn default_limits_match_the_phase_one_contract() {
    let limits = AiExecutionLimits::default();

    assert_eq!(limits.total_timeout.as_secs(), 180);
    assert_eq!(limits.spawn_timeout.as_secs(), 30);
    assert_eq!(limits.initialize_timeout.as_secs(), 10);
    assert_eq!(limits.config_rpc_timeout.as_secs(), 5);
    assert_eq!(limits.cancel_grace.as_secs(), 2);
    assert_eq!(limits.close_timeout.as_secs(), 2);
    assert_eq!(limits.cleanup_timeout.as_secs(), 10);
    assert_eq!(limits.text_bytes, 1024 * 1024);
    assert_eq!(limits.stderr_bytes, 256 * 1024);
}

#[test]
fn request_validation_rejects_invalid_prompt_and_model_before_execution() {
    let empty_prompt = request("   ", None);
    assert!(empty_prompt.validate().is_err());

    let oversized_prompt = request(&"x".repeat(1_000_001), None);
    assert!(oversized_prompt.validate().is_err());

    let invalid_model = request("translate", Some("model\nwith-newline"));
    assert!(invalid_model.validate().is_err());
}

#[test]
fn request_debug_output_redacts_the_prompt() {
    let request = request("SECRET_PROMPT", Some("SECRET_MODEL"));

    let debug = format!("{request:?}");

    assert!(!debug.contains("SECRET_PROMPT"));
    assert!(!debug.contains("SECRET_MODEL"));
    assert!(debug.contains("<redacted>"));
}

#[test]
fn result_debug_output_redacts_text_and_requested_model() {
    let result = AiExecutionResult {
        text: "SECRET_RESULT".to_string(),
        agent_id: AgentId::parse("opencode").unwrap(),
        protocol: AgentProtocol::Acp,
        requested_model: Some("SECRET_MODEL".to_string()),
        elapsed_ms: 1,
        persistent_binding: None,
        replay_text: None,
        session_cleanup: SessionCleanupStatus::Deleted,
    };

    let debug = format!("{result:?}");

    assert!(!debug.contains("SECRET_RESULT"));
    assert!(!debug.contains("SECRET_MODEL"));
    assert!(debug.contains("<redacted>"));
}

#[test]
fn request_forwards_session_events_through_the_existing_progress_sink() {
    let sink = Arc::new(CaptureProgressSink::default());
    let mut request = request("prompt", None);
    request.progress = Some(sink.clone());
    request.report_session_event(SessionEvent {
        identity: crate::backend::ai_execution::SessionEventIdentity {
            session_id: "session".to_string(),
            member_id: "member".to_string(),
            execution_id: "execution".to_string(),
            turn_id: "turn".to_string(),
            item_id: "item".to_string(),
            event_id: "event".to_string(),
        },
        sequence: 1,
        delivery: crate::backend::ai_execution::SessionEventDelivery::Live,
        kind: crate::backend::ai_execution::SessionEventKind::Processing {
            state: crate::backend::ai_execution::SessionProcessingState::Active,
        },
        truncation: None,
    });

    let events = sink.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].identity.item_id, "item");
}

#[test]
fn memory_generation_tools_are_scoped_to_memory_generation_requests() {
    let tools = AiMemoryGenerationTools {
        tenant_id: "tenant-fixture".to_string(),
        job_id: "recent-job-fixture".to_string(),
        ownership_token: "lease-fixture".to_string(),
        database_path: "/tmp/fixture.db".to_string(),
    };

    let mut generation = request("generate memory", None);
    generation.purpose = AiExecutionPurpose::MemoryGeneration;
    generation.memory_generation_tools = Some(tools.clone());
    assert!(generation.validate().is_ok());

    let mut translation = request("translate", None);
    translation.memory_generation_tools = Some(tools);
    assert!(matches!(
        translation.validate(),
        Err(AiExecutionError::Protocol {
            operation: "memory_generation_tools_scope"
        })
    ));

    let debug = format!("{generation:?}");
    assert!(!debug.contains("lease-fixture"));
    assert!(!debug.contains("/tmp/fixture.db"));
}

#[derive(Default)]
struct CaptureProgressSink {
    events: Mutex<Vec<SessionEvent>>,
}

impl AiExecutionProgressSink for CaptureProgressSink {
    fn set_phase(&self, _phase: AiExecutionPhase) {}

    fn emit_session_event(&self, event: SessionEvent) {
        self.events.lock().unwrap().push(event);
    }
}

fn request(prompt: &str, model: Option<&str>) -> AiExecutionRequest {
    AiExecutionRequest {
        execution_id: uuid::Uuid::new_v4().to_string(),
        agent_id: AgentId::parse("opencode").unwrap(),
        purpose: AiExecutionPurpose::Translation,
        session_mode: AgentSessionMode::OneShot,
        prompt: prompt.to_string(),
        model: model.map(str::to_string),
        limits: AiExecutionLimits::default(),
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
