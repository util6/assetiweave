use super::*;

#[test]
fn test_is_session_delete_failure() {
    assert!(is_session_delete_failure("delete_unsupported"));
    assert!(is_session_delete_failure("delete"));
    assert!(is_session_delete_failure("delete_timeout"));
    assert!(is_session_delete_failure("delete_fallback"));
    assert!(!is_session_delete_failure("process_reap_failed"));
    assert!(!is_session_delete_failure("workspace_cleanup_failed"));
}

#[test]
fn test_is_transient_probe_failure() {
    assert!(is_transient_probe_failure("cleanup_failed"));
    assert!(is_transient_probe_failure("connection_failed"));
    assert!(is_transient_probe_failure("agent_exited"));
    assert!(is_transient_probe_failure("initialize_timeout"));
    assert!(is_transient_probe_failure("session_new_timeout"));
    assert!(is_transient_probe_failure("custom_timeout"));
    assert!(is_transient_probe_failure("disconnect"));
    assert!(is_transient_probe_failure("transport_error"));
    assert!(is_transient_probe_failure("stdio_unavailable"));

    assert!(!is_transient_probe_failure("auth_required"));
    assert!(!is_transient_probe_failure("definition_invalid"));
    assert!(!is_transient_probe_failure("entry_missing"));
    assert!(!is_transient_probe_failure("executable_not_found"));
}
