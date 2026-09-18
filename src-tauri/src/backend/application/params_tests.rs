use super::*;

#[test]
fn conversation_sync_mode_defaults_to_incremental_and_accepts_full() {
    let incremental: ConversationSyncParams =
        serde_json::from_value(serde_json::json!({})).expect("default conversation sync params");
    let full: ConversationSyncParams = serde_json::from_value(serde_json::json!({
        "mode": "full"
    }))
    .expect("full conversation sync params");

    assert_eq!(incremental.mode, ConversationSyncMode::Incremental);
    assert_eq!(full.mode, ConversationSyncMode::Full);
    assert!(incremental.mode.uses_known_versions());
    assert!(!full.mode.uses_known_versions());
}
