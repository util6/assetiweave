use super::*;

#[test]
fn changed_session_ids_are_capped_and_stable() {
    let ids = (0..300).map(|i| format!("session-{i}")).collect::<Vec<_>>();
    assert!(cap_changed_session_ids(ids).is_none());

    let small = vec![
        "session-b".to_string(),
        "session-a".to_string(),
        "session-b".to_string(),
    ];
    let capped = cap_changed_session_ids(small.clone()).expect("under limit");
    assert_eq!(
        capped,
        vec!["session-a".to_string(), "session-b".to_string()]
    );
}

#[test]
fn conversation_source_committed_construction_and_metadata() {
    let event = DomainEvent::conversation_source_committed(
        "tenant_1",
        "sync_1",
        "source_1",
        42,
        vec!["s2".to_string(), "s1".to_string()],
    );

    let (tenant, event_type, source, rev_start, rev_end) = event.metadata();
    assert_eq!(tenant, "tenant_1");
    assert_eq!(event_type, "conversation_source_committed");
    assert_eq!(source, Some("source_1"));
    assert_eq!(rev_start, Some(42));
    assert_eq!(rev_end, Some(42));

    let DomainEvent::ConversationSourceCommitted {
        event_id,
        changed_session_ids,
        ..
    } = &event;
    assert!(event_id.starts_with("evt-"));
    assert_eq!(
        changed_session_ids.as_deref(),
        Some(&["s1".to_string(), "s2".to_string()][..])
    );
}
