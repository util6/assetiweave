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

#[test]
fn conversation_incremental_rejects_content_from_a_different_version() {
    let descriptor = ConversationSessionDescriptor {
        external_id: "session-1".to_string(),
        updated_at: None,
        source_locator: None,
        version_token: "version-before-read".to_string(),
    };
    let session = NormalizedConversationSession {
        external_id: "session-1".to_string(),
        title: None,
        project_path: None,
        started_at: None,
        updated_at: None,
        source_locator: None,
        source_fingerprint: Some("version-after-read".to_string()),
        turns: Vec::new(),
        ..Default::default()
    };

    assert!(!session_matches_descriptor(&session, &descriptor));
}
