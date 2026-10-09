use super::*;

#[test]
fn conversation_adapter_entry_path_policy_rejects_rooted_and_parent_paths() {
    assert_eq!(
        validate_conversation_adapter_entry_path("bin/adapter.js"),
        Ok(())
    );
    assert_eq!(
        validate_conversation_adapter_entry_path(" ./bin/adapter.js "),
        Ok(())
    );
    assert_eq!(
        validate_conversation_adapter_entry_path("../adapter.js"),
        Err(ConversationAdapterEntryPathError::ParentTraversal)
    );
    assert_eq!(
        validate_conversation_adapter_entry_path("nested\\..\\adapter.js"),
        Err(ConversationAdapterEntryPathError::ParentTraversal)
    );
    for rooted_path in [
        "/adapter.js",
        "\\adapter.js",
        "C:\\adapter.js",
        "C:adapter.js",
    ] {
        assert_eq!(
            validate_conversation_adapter_entry_path(rooted_path),
            Err(ConversationAdapterEntryPathError::Rooted),
            "expected rooted path to be rejected: {rooted_path}"
        );
    }
}

#[test]
fn conversation_id_fragment_extracts_hashes_and_supports_legacy_ids() {
    assert_eq!(
        conversation_id_fragment(
            "conversation-session-ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        ),
        "abcdef01"
    );
    assert_eq!(
        conversation_id_fragment(
            "conversation-part-1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef-answer"
        ),
        "12345678"
    );
    assert_eq!(conversation_id_fragment("  Legacy-Session  "), "legacy-s");
    assert_eq!(
        conversation_id_fragment("conversation-session-abcdef1234567890abcdef1234567890"),
        "abcdef12"
    );
    assert_eq!(conversation_id_fragment("   "), "");
}

#[test]
fn conversation_id_search_term_only_accepts_display_fragments_or_full_hash_ids() {
    assert_eq!(
        conversation_id_search_term("ABCDEF01"),
        Some("abcdef01".to_string())
    );
    assert_eq!(conversation_id_search_term("dead"), None);
    assert_eq!(conversation_id_search_term("abcdef123456"), None);
    assert_eq!(conversation_id_search_term("session title"), None);
    assert_eq!(
        conversation_id_search_term(
            "conversation-session-abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        ),
        Some(
            "conversation-session-abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
                .to_string()
        )
    );
    assert_eq!(
        conversation_id_search_term(
            "unrelated-abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        ),
        None
    );
}

#[test]
fn only_merges_exact_simple_acknowledgements() {
    assert!(should_auto_merge_acknowledgement("继续"));
    assert!(should_auto_merge_acknowledgement("OK"));
    assert!(!should_auto_merge_acknowledgement("继续解释一下原因"));
    assert!(!should_auto_merge_acknowledgement("ok?"));
    assert!(!should_auto_merge_acknowledgement("ok\nnow add tests"));
}

#[test]
fn groups_acknowledgement_turns_with_previous_question() {
    let groups = group_turn_ids_by_question(vec![
        ("t1".to_string(), "How does sync work?".to_string()),
        ("t2".to_string(), "继续".to_string()),
        ("t3".to_string(), "Now export it".to_string()),
    ]);

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].turn_ids, vec!["t1", "t2"]);
    assert_eq!(groups[0].origin, ConversationGroupingOrigin::AutoMerged);
    assert_eq!(groups[1].turn_ids, vec!["t3"]);
}

#[test]
fn groups_interruption_recovery_and_micro_follow_up_with_previous_question() {
    let groups = group_turn_ids_by_question(vec![
        ("t1".to_string(), "Design the sync boundary".to_string()),
        ("t2".to_string(), "继续上一个问题".to_string()),
        ("t3".to_string(), "改成 Rust".to_string()),
        ("t4".to_string(), "Now export it".to_string()),
    ]);

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].turn_ids, vec!["t1", "t2", "t3"]);
    assert_eq!(groups[0].origin, ConversationGroupingOrigin::AutoMerged);
    assert_eq!(groups[1].turn_ids, vec!["t4"]);
    assert_eq!(groups[1].origin, ConversationGroupingOrigin::Imported);
}

#[test]
fn sanitize_sync_error_message_masks_unix_user_paths() {
    let raw = "Error reading /Users/bob/project/data.json: timed out";
    let sanitized = sanitize_sync_error_message(raw, &[]);
    assert_eq!(sanitized, "Error reading ~/project/data.json: timed out");

    let raw_linux = "Error reading /home/alice/project/data.json: process exit 1";
    let sanitized_linux = sanitize_sync_error_message(raw_linux, &[]);
    assert_eq!(
        sanitized_linux,
        "Error reading ~/project/data.json: process exit 1"
    );

    // Test explicit home_paths matching
    let raw_custom = "Failed in /data/custom_home/workspace/file.rs";
    let sanitized_custom = sanitize_sync_error_message(raw_custom, &["/data/custom_home"]);
    assert_eq!(sanitized_custom, "Failed in ~/workspace/file.rs");

    // Test multiple explicit home paths
    let raw_multi = "Path1: /var/home1/a, Path2: /var/home2/b";
    let sanitized_multi = sanitize_sync_error_message(raw_multi, &["/var/home1", "/var/home2"]);
    assert_eq!(sanitized_multi, "Path1: ~/a, Path2: ~/b");

    // Test empty/whitespace home path ignored when string doesn't match
    let raw_ignored = "No change in /opt/app/dir";
    let sanitized_ignored = sanitize_sync_error_message(raw_ignored, &["", "   "]);
    assert_eq!(sanitized_ignored, "No change in /opt/app/dir");

    // Preserves legacy behavior: home paths with whitespace are matched exactly without silent trimming
    let raw_spaced = "Error reading  /data/spaced home /project/data.json";
    let sanitized_spaced = sanitize_sync_error_message(raw_spaced, &[" /data/spaced home "]);
    assert_eq!(sanitized_spaced, "Error reading ~/project/data.json");

    // An untrimmed home path does not accidentally match an unspaced path
    let raw_unspaced = "Error reading /data/spaced home/project/data.json";
    let sanitized_unspaced = sanitize_sync_error_message(raw_unspaced, &[" /data/spaced home "]);
    assert_eq!(
        sanitized_unspaced,
        "Error reading /data/spaced home/project/data.json"
    );

    // Empty home path does not replace anything
    let raw_empty = "No change in /custom/dir";
    let sanitized_empty = sanitize_sync_error_message(raw_empty, &[""]);
    assert_eq!(sanitized_empty, "No change in /custom/dir");
}
