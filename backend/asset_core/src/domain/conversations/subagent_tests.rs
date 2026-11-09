use super::*;

fn subagent_session_id(hash_prefix: &str) -> String {
    format!(
        "conversation-session-{hash_prefix}{}",
        "0".repeat(64 - hash_prefix.len())
    )
}

#[test]
fn link_requires_subagent_execution_origin() {
    let purpose = r#"{"parent_session_id":"conversation-session-aa"}"#;
    assert!(SubagentSessionLink::from_session("user", Some(purpose)).is_none());
    assert!(SubagentSessionLink::from_session("agent", Some(purpose)).is_none());
    assert!(SubagentSessionLink::from_session("subagent", Some(purpose)).is_some());
    assert!(SubagentSessionLink::from_session("SUBAGENT", Some(purpose)).is_some());
}

#[test]
fn link_parses_parent_session_role_and_turn() {
    let parent_id = subagent_session_id("8f3a1c2b");
    let purpose = format!(
        r#"{{"parent_session_id":"{parent_id}","agent_role":"Explore","parent_turn_index":3}}"#
    );
    let link = SubagentSessionLink::from_session("subagent", Some(&purpose)).expect("link parsed");
    assert_eq!(link.parent_session_id, parent_id);
    assert_eq!(link.short_parent_session_id(), "8f3a1c2b");
    assert_eq!(link.agent_role.as_deref(), Some("Explore"));
    assert_eq!(link.parent_turn_index, Some(3));

    let minimal = r#"{"parent_session_id":"conversation-session-ff"}"#;
    let link = SubagentSessionLink::from_session("subagent", Some(minimal)).expect("link parsed");
    assert_eq!(link.agent_role, None);
    assert_eq!(link.parent_turn_index, None);
}

#[test]
fn link_tolerates_missing_or_broken_purpose_payload() {
    assert!(SubagentSessionLink::from_session("subagent", None).is_none());
    assert!(SubagentSessionLink::from_session("subagent", Some("subagent")).is_none());
    assert!(SubagentSessionLink::from_session("subagent", Some("[]")).is_none());
    // 有 JSON 但没有父会话 ID：无法溯源，视为非子代理会话。
    assert!(SubagentSessionLink::from_session("subagent", Some(r#"{"agent_role":"x"}"#)).is_none());
    assert!(
        SubagentSessionLink::from_session("subagent", Some(r#"{"parent_session_id":"  "}"#))
            .is_none()
    );
}

#[test]
fn anchor_parses_dispatch_metadata() {
    let child_id = subagent_session_id("12ab34cd");
    let metadata =
        format!(r#"{{"child_session_id":"{child_id}","agent_role":"Review","title":"审阅差异"}}"#);
    let anchor = SubagentDispatchAnchor::from_metadata(Some(&metadata)).expect("anchor parsed");
    assert_eq!(anchor.child_session_id.as_deref(), Some(child_id.as_str()));
    assert_eq!(anchor.short_child_session_id().as_deref(), Some("12ab34cd"));
    assert_eq!(anchor.agent_role.as_deref(), Some("Review"));
    assert_eq!(anchor.title.as_deref(), Some("审阅差异"));
}

#[test]
fn anchor_ignores_unrelated_metadata() {
    assert!(SubagentDispatchAnchor::from_metadata(None).is_none());
    assert!(SubagentDispatchAnchor::from_metadata(Some("not-json")).is_none());
    assert!(
        SubagentDispatchAnchor::from_metadata(Some(r#"{"content_card":{"kind":"x"}}"#)).is_none()
    );
}

#[test]
fn activity_summary_formats_single_anchor() {
    let child_id = subagent_session_id("8f3a1c2b");
    let with_role = SubagentDispatchAnchor {
        child_session_id: Some(child_id.clone()),
        agent_role: Some("Explore".to_string()),
        title: None,
    };
    assert_eq!(
        subagent_activity_summary(&[with_role]).as_deref(),
        Some("Subagent(Explore -> 8f3a1c2b)")
    );

    let without_role = SubagentDispatchAnchor {
        child_session_id: Some(child_id),
        agent_role: None,
        title: Some("ignored".to_string()),
    };
    assert_eq!(
        subagent_activity_summary(&[without_role]).as_deref(),
        Some("Subagent(-> 8f3a1c2b)")
    );
}

#[test]
fn activity_summary_skips_anchors_without_child_session() {
    let orphan = SubagentDispatchAnchor {
        child_session_id: None,
        agent_role: Some("Explore".to_string()),
        title: None,
    };
    assert_eq!(subagent_activity_summary(&[orphan]), None);
    assert_eq!(subagent_activity_summary(&[]), None);
}

#[test]
fn activity_summary_dedupes_and_folds_many_anchors() {
    fn anchor(hash: &str, role: &str) -> SubagentDispatchAnchor {
        SubagentDispatchAnchor {
            child_session_id: Some(subagent_session_id(hash)),
            agent_role: Some(role.to_string()),
            title: None,
        }
    }

    let duplicated = vec![anchor("aaaaaaaa", "Explore"), anchor("aaaaaaaa", "Explore")];
    assert_eq!(
        subagent_activity_summary(&duplicated).as_deref(),
        Some("Subagent(Explore -> aaaaaaaa)")
    );

    let many = vec![
        anchor("11111111", "Explore"),
        anchor("22222222", "Review"),
        anchor("33333333", "Plan"),
        anchor("44444444", "Verify"),
    ];
    assert_eq!(
        subagent_activity_summary(&many).as_deref(),
        Some("Subagent x4(Explore -> 11111111, Review -> 22222222, Plan -> 33333333, ...)")
    );
}

#[test]
fn anchor_index_text_collects_searchable_fields() {
    let metadata = r#"{"child_session_id":"conversation-session-x","agent_role":"Review","title":"审阅差异","report_summary":"发现两处越权写库"}"#;
    let text = subagent_anchor_index_text(Some(metadata)).expect("index text");
    assert!(text.contains("Review"));
    assert!(text.contains("审阅差异"));
    assert!(text.contains("发现两处越权写库"));

    assert_eq!(
        subagent_anchor_index_text(Some(r#"{"child_session_id":"x"}"#)),
        None
    );
    assert_eq!(subagent_anchor_index_text(Some("")), None);
}
