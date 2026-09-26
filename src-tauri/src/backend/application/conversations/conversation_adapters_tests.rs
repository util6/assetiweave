use super::super::conversation_sync_pipeline::*;
use super::super::conversation_sync_support::*;
use super::*;

fn adapter(id: &str, capabilities: &[&str]) -> ConversationAdapter {
    ConversationAdapter {
        id: id.to_string(),
        name: id.to_string(),
        kind: crate::backend::domain::ConversationAdapterKind::External,
        version: "0.1.0".to_string(),
        enabled: true,
        manifest_path: None,
        executable_path: None,
        content_hash: None,
        trusted_hash: None,
        trust_state: crate::backend::domain::ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: capabilities.iter().map(|value| value.to_string()).collect(),
        input_kinds: vec![crate::backend::domain::ConversationSourceKind::Directory],
        card_contract_version: None,
        card_kinds: Vec::new(),
        created_at: "2026-06-23T00:00:00Z".to_string(),
        updated_at: "2026-06-23T00:00:00Z".to_string(),
    }
}

#[test]
fn conversation_session_descriptors_map_to_domain_observations() {
    let descriptors = vec![
        crate::backend::infrastructure::conversations::ConversationSessionDescriptor {
            external_id: "session-1".to_string(),
            updated_at: Some("2026-09-23T00:00:00Z".to_string()),
            source_locator: Some("sessions/session-1.json".to_string()),
            version_token: "source-v1".to_string(),
        },
    ];

    assert_eq!(
        persistable_conversation_session_observations(&descriptors),
        vec![
            crate::backend::domain::conversations::ConversationSessionObservation {
                external_id: "session-1".to_string(),
                updated_at: Some("2026-09-23T00:00:00Z".to_string()),
                source_locator: Some("sessions/session-1.json".to_string()),
                version_token: "source-v1".to_string(),
            }
        ]
    );
}

#[test]
fn sync_record_kind_filters_session_and_web_sources_by_adapter_capability() {
    let session_adapter = adapter("codex", &["read_session"]);
    let web_adapter = adapter("qwen-web", &["read_session", "web_records"]);

    assert!(sync_source_matches_record_kind(
        Some(&session_adapter),
        "codex",
        normalize_sync_record_kind(Some("session")).unwrap(),
    ));
    assert!(!sync_source_matches_record_kind(
        Some(&web_adapter),
        "qwen-web",
        normalize_sync_record_kind(Some("session")).unwrap(),
    ));
    assert!(!sync_source_matches_record_kind(
        Some(&session_adapter),
        "codex",
        normalize_sync_record_kind(Some("web_records")).unwrap(),
    ));
    assert!(sync_source_matches_record_kind(
        Some(&web_adapter),
        "qwen-web",
        normalize_sync_record_kind(Some("web")).unwrap(),
    ));
    assert!(matches!(
        normalize_sync_record_kind(Some("assets")),
        Err(AppError::Validation(_))
    ));
}
