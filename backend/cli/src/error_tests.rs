use super::*;

#[test]
fn test_cli_error_constructors() {
    let err = CliError::new("TEST_CODE", "An error occurred");
    assert_eq!(err.code, "TEST_CODE");
    assert_eq!(err.message, "An error occurred");
    assert_eq!(err.hint, None);

    let err_with_hint = err.with_hint("run 'aiwc test'");
    assert_eq!(err_with_hint.hint, Some("run 'aiwc test'".to_string()));

    let direct = CliError::new_with_hint("DIRECT", "msg", "hint");
    assert_eq!(direct.code, "DIRECT");
    assert_eq!(direct.message, "msg");
    assert_eq!(direct.hint, Some("hint".to_string()));
}

#[test]
fn test_cli_error_from_anyhow() {
    let original = CliError::card_not_found("card-123");
    let anyhow_err: anyhow::Error = original.clone().into();
    let recovered = CliError::from_anyhow(&anyhow_err);
    assert_eq!(recovered, original);

    let generic_err = anyhow::anyhow!("generic failure");
    let converted = CliError::from_anyhow(&generic_err);
    assert_eq!(converted.code, "COMMAND_FAILED");
    assert_eq!(converted.message, "generic failure");
    assert_eq!(converted.hint, None);
}

#[test]
fn test_conversation_error_helpers() {
    let card_err = CliError::card_not_found("c1");
    assert_eq!(card_err.code, "CARD_NOT_FOUND");
    assert_eq!(card_err.message, "Conversation card not found: c1");
    assert_eq!(
        card_err.hint,
        Some("run 'aiwc conversation outline' or 'aiwc conversation search'".to_string())
    );

    let outline_err = CliError::outline_not_found("o1");
    assert_eq!(outline_err.code, "OUTLINE_NOT_FOUND");
    assert_eq!(outline_err.message, "Conversation outline not found: o1");
    assert_eq!(
        outline_err.hint,
        Some("run 'aiwc conversation list'".to_string())
    );

    let adapter_err = CliError::adapter_not_found("a1");
    assert_eq!(adapter_err.code, "ADAPTER_NOT_FOUND");
    assert_eq!(adapter_err.message, "Conversation adapter not found: a1");
    assert_eq!(
        adapter_err.hint,
        Some("run 'aiwc conversation adapter list'".to_string())
    );

    let search_err = CliError::search_query_required();
    assert_eq!(search_err.code, "INVALID_PARAMETER");
    assert_eq!(search_err.message, "Conversation search query is required");
    assert_eq!(
        search_err.hint,
        Some(
            "run 'aiwc conversation search --query <QUERY>' or 'aiwc conversation list'"
                .to_string()
        )
    );
}

#[test]
fn test_cli_error_serialization() {
    let with_hint = CliError::new_with_hint("CODE1", "msg1", "hint1");
    let json_with_hint = serde_json::to_value(&with_hint).unwrap();
    assert_eq!(
        json_with_hint,
        serde_json::json!({
            "code": "CODE1",
            "message": "msg1",
            "hint": "hint1"
        })
    );

    let without_hint = CliError::new("CODE2", "msg2");
    let json_without_hint = serde_json::to_value(&without_hint).unwrap();
    assert_eq!(
        json_without_hint,
        serde_json::json!({
            "code": "CODE2",
            "message": "msg2"
        })
    );
}
