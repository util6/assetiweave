use super::*;
use std::collections::BTreeMap;

#[test]
fn conversation_incremental_selects_old_session_when_its_version_changes() {
    let descriptors = vec![
        ConversationSessionDescriptor {
            external_id: "old-session".to_string(),
            updated_at: Some("2026-07-16T01:02:03Z".to_string()),
            source_locator: Some("/tmp/old.jsonl".to_string()),
            version_token: "version-2".to_string(),
        },
        ConversationSessionDescriptor {
            external_id: "unchanged-session".to_string(),
            updated_at: Some("2026-07-15T01:02:03Z".to_string()),
            source_locator: Some("/tmp/unchanged.jsonl".to_string()),
            version_token: "same-version".to_string(),
        },
    ];
    let known_versions = BTreeMap::from([
        ("old-session".to_string(), "version-1".to_string()),
        ("unchanged-session".to_string(), "same-version".to_string()),
    ]);

    let active = select_active_session_descriptors(&descriptors, &known_versions)
        .expect("select active sessions");

    assert_eq!(active.len(), 1);
    assert_eq!(active[0].external_id, "old-session");
}

#[test]
fn conversation_incremental_rejects_conflicting_duplicate_descriptors() {
    let descriptors = vec![
        ConversationSessionDescriptor {
            external_id: "duplicate".to_string(),
            updated_at: None,
            source_locator: None,
            version_token: "version-1".to_string(),
        },
        ConversationSessionDescriptor {
            external_id: "duplicate".to_string(),
            updated_at: None,
            source_locator: None,
            version_token: "version-2".to_string(),
        },
    ];

    let error = select_active_session_descriptors(&descriptors, &BTreeMap::new())
        .expect_err("conflicting versions must fail discovery");

    assert!(error.contains("duplicate"));
}

fn descriptor(external_id: &str, version_token: &str) -> ConversationSessionDescriptor {
    ConversationSessionDescriptor {
        external_id: external_id.to_string(),
        updated_at: None,
        source_locator: None,
        version_token: version_token.to_string(),
    }
}

fn session(external_id: &str, fingerprint: Option<&str>) -> NormalizedConversationSession {
    NormalizedConversationSession {
        external_id: external_id.to_string(),
        source_fingerprint: fingerprint.map(str::to_string),
        ..Default::default()
    }
}

fn inline_child(external_id: &str, fingerprint: Option<&str>) -> NormalizedConversationSession {
    NormalizedConversationSession {
        external_id: external_id.to_string(),
        source_fingerprint: fingerprint.map(str::to_string),
        execution_origin: Some(SUBAGENT_EXECUTION_ORIGIN.to_string()),
        ..Default::default()
    }
}

#[test]
fn conversation_incremental_rejects_content_from_a_different_version() {
    let descriptor = descriptor("session-1", "version-before-read");
    let read = vec![session("session-1", Some("version-after-read"))];

    let failure = resolve_read_result("codex", &descriptor, read).expect_err("drift must fail");

    assert_eq!(failure.error_code, "session_version_drift");
    assert!(failure.retryable);
}

#[test]
fn read_result_accepts_one_root_plus_its_inline_children() {
    let descriptor = descriptor("parent", "version-1");
    let read = vec![
        session("parent", Some("version-1")),
        inline_child("parent:subagent:agent-1", None),
    ];

    let resolved = resolve_read_result("codex", &descriptor, read).expect("root plus children");

    assert_eq!(resolved.len(), 2);
    assert_eq!(
        resolved[1].source_fingerprint.as_deref(),
        Some("version-1:subagent:parent:subagent:agent-1")
    );
}

#[test]
fn inline_child_without_fingerprint_inherits_parent_watermark_deterministically() {
    let descriptor = descriptor("parent", "version-9");
    let read = vec![
        session("parent", Some("version-9")),
        inline_child("parent:subagent:a", None),
    ];

    let first = resolve_read_result("codex", &descriptor, read.clone()).expect("first read");
    let second = resolve_read_result("codex", &descriptor, read).expect("second read");

    assert_eq!(first[1].source_fingerprint, second[1].source_fingerprint);
    assert_eq!(
        first[1].source_fingerprint.as_deref(),
        Some("version-9:subagent:parent:subagent:a")
    );
}

#[test]
fn read_result_rejects_missing_root() {
    let descriptor = descriptor("parent", "version-1");
    let read = vec![session("someone-else", Some("version-1"))];

    let failure = resolve_read_result("codex", &descriptor, read).expect_err("missing root");

    assert_eq!(failure.error_code, "session_mismatch");
}

#[test]
fn read_result_rejects_duplicate_root() {
    let descriptor = descriptor("parent", "version-1");
    let read = vec![
        session("parent", Some("version-1")),
        session("parent", Some("version-1")),
    ];

    let failure = resolve_read_result("codex", &descriptor, read).expect_err("duplicate root");

    assert_eq!(failure.error_code, "session_duplicate");
}

#[test]
fn read_result_rejects_unrelated_sessions() {
    let descriptor = descriptor("parent", "version-1");
    let read = vec![
        session("parent", Some("version-1")),
        session("unrelated-user-session", Some("version-7")),
    ];

    let failure = resolve_read_result("codex", &descriptor, read).expect_err("unrelated session");

    assert_eq!(failure.error_code, "session_unrelated");
}

#[test]
fn read_result_rejects_unexpected_child_version_tokens() {
    let descriptor = descriptor("parent", "version-1");
    let read = vec![
        session("parent", Some("version-1")),
        inline_child("parent:subagent:a", Some("some-other-token")),
    ];

    let failure =
        resolve_read_result("codex", &descriptor, read).expect_err("child drift must fail");

    assert_eq!(failure.error_code, "session_child_version_drift");
}

#[test]
fn read_result_accepts_child_with_its_own_content_fingerprint() {
    let descriptor = descriptor("parent", "version-1");
    let read = vec![
        session("parent", Some("version-1")),
        inline_child("parent:subagent:a", Some("version-1")),
    ];

    let resolved = resolve_read_result("codex", &descriptor, read).expect("independent file");

    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[1].source_fingerprint.as_deref(), Some("version-1"));
}
