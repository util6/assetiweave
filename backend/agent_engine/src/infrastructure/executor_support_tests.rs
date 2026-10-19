use super::*;
use crate::domain::{AgentId, AgentProtocol};
use crate::infrastructure::types::{AiExecutionResult, SessionCleanupStatus};

fn fake_result(session_cleanup: SessionCleanupStatus) -> AiExecutionResult {
    AiExecutionResult {
        text: "generated content".to_string(),
        agent_id: AgentId::parse("test-agent").unwrap(),
        protocol: AgentProtocol::Acp,
        requested_model: None,
        elapsed_ms: 100,
        persistent_binding: None,
        replay_text: None,
        session_cleanup,
    }
}

#[test]
fn memory_purposes_bypass_session_cleanup_enforcement() {
    let memory_purposes = [
        AiExecutionPurpose::SessionMemory,
        AiExecutionPurpose::MemoryGeneration,
        AiExecutionPurpose::ProjectMemory,
        AiExecutionPurpose::GlobalMemory,
        AiExecutionPurpose::Recall,
    ];

    for purpose in memory_purposes {
        // Unsupported cleanup is bypassed and returns Ok
        let outcome = Ok(fake_result(SessionCleanupStatus::Unsupported));
        let result = enforce_cleanup_contract(purpose, outcome);
        assert!(
            result.is_ok(),
            "purpose {purpose:?} should bypass Unsupported"
        );

        // Failed cleanup is bypassed and returns Ok
        let outcome = Ok(fake_result(SessionCleanupStatus::Failed(
            "timeout".to_string(),
        )));
        let result = enforce_cleanup_contract(purpose, outcome);
        assert!(result.is_ok(), "purpose {purpose:?} should bypass Failed");

        // Normal cleanup returns Ok
        let outcome = Ok(fake_result(SessionCleanupStatus::Deleted));
        let result = enforce_cleanup_contract(purpose, outcome);
        assert!(result.is_ok(), "purpose {purpose:?} should pass Deleted");
    }
}

#[test]
fn non_memory_purposes_enforce_session_cleanup_contract() {
    let non_memory_purposes = [
        AiExecutionPurpose::Translation,
        AiExecutionPurpose::PromptOptimization,
        AiExecutionPurpose::ConnectionTest,
        AiExecutionPurpose::ModelDiscovery,
    ];

    for purpose in non_memory_purposes {
        // Deleted & Skipped pass
        let outcome = Ok(fake_result(SessionCleanupStatus::Deleted));
        assert!(enforce_cleanup_contract(purpose, outcome).is_ok());

        let outcome = Ok(fake_result(SessionCleanupStatus::Skipped));
        assert!(enforce_cleanup_contract(purpose, outcome).is_ok());

        // Unsupported produces CleanupFailed
        let outcome = Ok(fake_result(SessionCleanupStatus::Unsupported));
        let err = enforce_cleanup_contract(purpose, outcome).unwrap_err();
        assert!(matches!(err, AiExecutionError::CleanupFailed { .. }));

        // Failed produces CleanupFailed
        let outcome = Ok(fake_result(SessionCleanupStatus::Failed(
            "delete_failed".to_string(),
        )));
        let err = enforce_cleanup_contract(purpose, outcome).unwrap_err();
        assert!(matches!(err, AiExecutionError::CleanupFailed { .. }));
    }
}

#[test]
fn enforce_cleanup_contract_preserves_underlying_error() {
    let outcome = Err(AiExecutionError::ToolUseDenied);
    let result = enforce_cleanup_contract(AiExecutionPurpose::MemoryGeneration, outcome);
    assert!(matches!(result, Err(AiExecutionError::ToolUseDenied)));
}
