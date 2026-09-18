use super::*;
use crate::backend::ai_execution::{
    SessionEvent, SessionEventDelivery, SessionEventIdentity, SessionEventKind,
};

fn key(id: &str) -> SessionStreamKey {
    SessionStreamKey {
        tenant_id: "tenant".to_string(),
        team_id: "team".to_string(),
        member_id: "member".to_string(),
        execution_id: id.to_string(),
    }
}

#[test]
fn registry_is_bounded_and_keeps_active_projection_until_capacity_is_exhausted() {
    let registry = SessionStreamRegistry::new(2);
    let first = key("first");
    let second = key("second");
    let third = key("third");
    registry.register(first.clone());
    registry.register(second.clone());
    registry.mark_terminal(&first);

    registry.register(third.clone());

    assert!(registry.get(&first).is_none());
    assert!(registry.get(&second).is_some());
    assert!(registry.get(&third).is_some());
}

#[test]
fn clear_drops_all_transient_projections() {
    let registry = SessionStreamRegistry::new(1);
    let key = key("execution");
    registry.register(key.clone());

    registry.clear();

    assert!(registry.get(&key).is_none());
}

#[test]
fn metadata_and_by_ref_lookup_and_eviction_work() {
    let registry = SessionStreamRegistry::new(2);
    let key1 = key("first");
    let ref1 = AgentSessionRef::new("ref-1");
    let meta1 = AgentSessionMetadata {
        session_ref: ref1.clone(),
        execution_id: "first".to_string(),
        purpose: "memory".to_string(),
        mode: "oneshot".to_string(),
        tenant_id: Some("tenant".to_string()),
        agent: AgentInfoView {
            id: "agent-1".to_string(),
            display_name: None,
            model: None,
            protocol: "builtin".to_string(),
        },
        context: AgentSessionContextView {
            team_id: None,
            member_id: None,
            memory_scope: Some("session".to_string()),
            memory_job_id: Some("job-1".to_string()),
            task_id: Some("task-1".to_string()),
        },
        allow_stop: true,
    };

    registry.register_with_metadata(key1.clone(), meta1);
    let snapshot = registry.get_by_ref("ref-1");
    assert!(snapshot.is_some());
    let view = snapshot.unwrap().to_view();
    assert_eq!(view.session_ref.value, "ref-1");
    assert_eq!(view.state, "active");

    registry.mark_terminal_by_ref(
        "ref-1",
        Some(AgentSessionTerminalView {
            state: "succeeded".to_string(),
            code: None,
            message: None,
            retryable: false,
        }),
    );
    let updated = registry.get_by_ref("ref-1").unwrap().to_view();
    assert_eq!(updated.state, "terminal");
    assert_eq!(
        updated.terminal.as_ref().map(|t| t.state.as_str()),
        Some("succeeded")
    );
}

#[test]
fn test_eviction_removes_ref_lookup() {
    let registry = SessionStreamRegistry::new(2);

    let make_meta = |ref_name: &str, task_id: &str| AgentSessionMetadata {
        session_ref: AgentSessionRef::new(ref_name),
        execution_id: ref_name.to_string(),
        purpose: "memory".to_string(),
        mode: "oneshot".to_string(),
        tenant_id: Some("tenant-1".to_string()),
        agent: AgentInfoView {
            id: "agent-1".to_string(),
            display_name: None,
            model: None,
            protocol: "builtin".to_string(),
        },
        context: AgentSessionContextView {
            team_id: None,
            member_id: None,
            memory_scope: Some("project".to_string()),
            memory_job_id: Some("job-1".to_string()),
            task_id: Some(task_id.to_string()),
        },
        allow_stop: true,
    };

    let key1 = key("item-1");
    let key2 = key("item-2");
    let key3 = key("item-3");

    registry.register_with_metadata(key1.clone(), make_meta("ref-1", "task-1"));
    registry.register_with_metadata(key2.clone(), make_meta("ref-2", "task-2"));

    assert!(registry.get_by_ref("ref-1").is_some());
    assert!(registry.get_by_ref("ref-2").is_some());

    // Register 3rd item, causing key1 / ref-1 to be evicted
    registry.register_with_metadata(key3.clone(), make_meta("ref-3", "task-3"));

    assert!(registry.get_by_ref("ref-1").is_none());
    assert!(registry.get_by_ref("ref-2").is_some());
    assert!(registry.get_by_ref("ref-3").is_some());
}

#[test]
fn test_recall_multi_turn_reinitialization_and_accumulation() {
    let registry = SessionStreamRegistry::new(4);
    let recall_key = key("recall-session-1");
    let recall_ref = AgentSessionRef::new("agent-session://recall/session-123");

    let make_meta = || AgentSessionMetadata {
        session_ref: recall_ref.clone(),
        execution_id: "recall-session-1".to_string(),
        purpose: "recall_memory".to_string(),
        mode: "persistent".to_string(),
        tenant_id: Some("tenant-1".to_string()),
        agent: AgentInfoView {
            id: "agent-recall".to_string(),
            display_name: Some("Recall Agent".to_string()),
            model: None,
            protocol: "builtin".to_string(),
        },
        context: AgentSessionContextView {
            team_id: None,
            member_id: None,
            memory_scope: Some("recall".to_string()),
            memory_job_id: Some("session-123".to_string()),
            task_id: Some("task-turn-1".to_string()),
        },
        allow_stop: true,
    };

    // Turn 1
    let proj1 = registry.register_with_metadata(recall_key.clone(), make_meta());
    let view1 = registry.get_by_ref(&recall_ref.value).unwrap().to_view();
    assert_eq!(view1.state, "active");
    assert!(view1.terminal.is_none());

    proj1.apply(SessionEvent {
        identity: SessionEventIdentity {
            session_id: "recall-session-1".to_string(),
            member_id: "recall_memory".to_string(),
            execution_id: "recall-session-1".to_string(),
            turn_id: "turn-1".to_string(),
            item_id: "turn-1:user".to_string(),
            event_id: "turn-1:user:1".to_string(),
        },
        sequence: 1,
        delivery: SessionEventDelivery::Live,
        kind: SessionEventKind::UserMessageAcknowledged {
            accepted: true,
            text: Some("第一轮问题".to_string()),
        },
        truncation: None,
    });

    registry.mark_terminal_by_ref(
        &recall_ref.value,
        Some(AgentSessionTerminalView {
            state: "succeeded".to_string(),
            code: None,
            message: None,
            retryable: false,
        }),
    );
    let turn1_finished = registry.get_by_ref(&recall_ref.value).unwrap().to_view();
    assert_eq!(turn1_finished.state, "terminal");
    assert_eq!(turn1_finished.items.len(), 1);

    // Turn 2 re-registers the same key & ref
    let proj2 = registry.register_with_metadata(recall_key.clone(), make_meta());
    let turn2_started = registry.get_by_ref(&recall_ref.value).unwrap().to_view();
    // Should be reactivated to active, terminal info cleared
    assert_eq!(turn2_started.state, "active");
    assert!(turn2_started.terminal.is_none());

    // Events should accumulate on the same projection
    proj2.apply(SessionEvent {
        identity: SessionEventIdentity {
            session_id: "recall-session-1".to_string(),
            member_id: "recall_memory".to_string(),
            execution_id: "recall-session-1".to_string(),
            turn_id: "turn-2".to_string(),
            item_id: "turn-2:user".to_string(),
            event_id: "turn-2:user:1".to_string(),
        },
        sequence: 2,
        delivery: SessionEventDelivery::Live,
        kind: SessionEventKind::UserMessageAcknowledged {
            accepted: true,
            text: Some("第二轮追问".to_string()),
        },
        truncation: None,
    });

    let turn2_view = registry.get_by_ref(&recall_ref.value).unwrap().to_view();
    assert_eq!(turn2_view.items.len(), 2);
    assert_eq!(
        turn2_view.items[0].identity.item_id,
        "turn-1:user".to_string()
    );
    assert_eq!(
        turn2_view.items[1].identity.item_id,
        "turn-2:user".to_string()
    );
}

#[test]
fn test_tenant_metadata_filtering() {
    let registry = SessionStreamRegistry::new(2);
    let key_a = key("tenant-a-exec");
    let ref_a = AgentSessionRef::new("ref-a");

    let meta_a = AgentSessionMetadata {
        session_ref: ref_a.clone(),
        execution_id: "tenant-a-exec".to_string(),
        purpose: "global_memory".to_string(),
        mode: "oneshot".to_string(),
        tenant_id: Some("tenant-apple".to_string()),
        agent: AgentInfoView {
            id: "agent-global".to_string(),
            display_name: None,
            model: None,
            protocol: "builtin".to_string(),
        },
        context: AgentSessionContextView {
            team_id: None,
            member_id: None,
            memory_scope: Some("global".to_string()),
            memory_job_id: Some("job-global".to_string()),
            task_id: Some("task-global".to_string()),
        },
        allow_stop: false,
    };

    registry.register_with_metadata(key_a, meta_a);
    let snap = registry.get_by_ref("ref-a").expect("must exist");
    assert_eq!(snap.metadata.tenant_id.as_deref(), Some("tenant-apple"));
    assert_ne!(snap.metadata.tenant_id.as_deref(), Some("tenant-banana"));
}
