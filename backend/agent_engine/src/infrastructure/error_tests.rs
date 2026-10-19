use super::*;

#[test]
fn public_error_view_has_a_stable_code() {
    let error = AiExecutionError::AgentExited {
        code: Some(1),
        stderr: None,
    };

    let view = error.to_view();
    let public_debug = format!("{view:?}");

    assert_eq!(view.code, "agent_exited");
    assert!(view.retryable);
    assert!(!public_debug.contains("/private/"));
}

#[test]
fn public_protocol_error_preserves_the_sanitized_agent_message() {
    let view = AiExecutionError::ProtocolDetail {
        operation: "prompt",
        detail: "Free promotion has ended for the selected model.".to_string(),
    }
    .to_view();

    assert_eq!(view.code, "protocol_failed");
    assert!(view.message.contains("Free promotion has ended"));
}

#[test]
fn public_model_unavailable_error_is_actionable() {
    let view = AiExecutionError::ModelUnavailable {
        detail: "No allowed providers are available for the selected model.".to_string(),
    }
    .to_view();

    assert_eq!(view.code, "model_unavailable");
    assert!(view
        .message
        .contains("Choose another model in Agent settings"));
    assert!(view.message.contains("No allowed providers"));
    assert!(!view.retryable);
}

#[test]
fn public_cleanup_failed_error_is_non_retryable() {
    let view = AiExecutionError::CleanupFailed {
        failures: vec!["delete_unsupported".to_string()],
    }
    .to_view();

    assert_eq!(view.code, "cleanup_failed");
    assert!(!view.retryable);
}
