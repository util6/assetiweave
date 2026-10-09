use super::*;

#[test]
fn test_cancellation() {
    let cancel = AgentCancellation::new();
    assert!(!cancel.is_cancelled());
    cancel.cancel();
    assert!(cancel.is_cancelled());
}

#[test]
fn test_default_limits() {
    let limits = AgentExecutionLimits::default();
    assert_eq!(limits.timeout, Duration::from_secs(120));
    assert!(limits.max_response_bytes > 0);
}
