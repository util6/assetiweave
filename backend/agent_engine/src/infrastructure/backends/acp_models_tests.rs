use super::*;
use agent_client_protocol::schema::v1::NewSessionResponse;
use serde_json::json;

fn create_test_session_with_models(
    current: &str,
    options: Vec<(&str, &str)>,
) -> NewSessionResponse {
    let options_json = options
        .into_iter()
        .map(|(val, name)| {
            json!({
                "value": val,
                "name": name,
            })
        })
        .collect::<Vec<_>>();

    let val = json!({
        "sessionId": "session-1",
        "configOptions": [
            {
                "id": "model",
                "name": "Model",
                "type": "select",
                "currentValue": current,
                "options": options_json
            }
        ]
    });

    serde_json::from_value(val).expect("valid session response")
}

#[test]
fn test_resolve_model_when_requested_matches() {
    let session = create_test_session_with_models(
        "gpt-5.6-terra",
        vec![("gpt-5.6-terra", "Terra"), ("gpt-5.6-luna", "Luna")],
    );

    let chosen = resolve_model_for_session(&session, Some("gpt-5.6-luna"), "codex");
    assert_eq!(chosen.as_deref(), Some("gpt-5.6-luna"));
}

#[test]
fn test_resolve_model_when_requested_is_invalid_for_codex() {
    let session = create_test_session_with_models(
        "gpt-6-luna",
        vec![("gpt-5.6-terra", "Terra"), ("gpt-5.6-luna", "Luna")],
    );

    // Cross-agent residual model like "anthropic/claude-3-5" should fallback to gpt-5.6-terra for Codex
    let chosen = resolve_model_for_session(&session, Some("anthropic/claude-3-5"), "codex");
    assert_eq!(chosen.as_deref(), Some("gpt-5.6-terra"));
}

#[test]
fn test_resolve_model_when_default_current_is_invalid_for_codex() {
    // Codex default is gpt-6-luna on ChatGPT accounts, which is not in the options
    let session = create_test_session_with_models(
        "gpt-6-luna",
        vec![("gpt-5.6-terra", "Terra"), ("gpt-5.6-luna", "Luna")],
    );

    // No model requested (empty)
    let chosen = resolve_model_for_session(&session, None, "codex");
    assert_eq!(chosen.as_deref(), Some("gpt-5.6-terra"));
}

#[test]
fn test_resolve_model_when_default_current_is_valid() {
    let session = create_test_session_with_models(
        "model-a",
        vec![("model-a", "Model A"), ("model-b", "Model B")],
    );

    let chosen = resolve_model_for_session(&session, None, "generic-agent");
    assert_eq!(chosen.as_deref(), Some("model-a"));
}

#[test]
fn test_resolve_model_without_catalog() {
    let val = json!({
        "sessionId": "session-1"
    });
    let session: NewSessionResponse = serde_json::from_value(val).unwrap();
    let chosen = resolve_model_for_session(&session, Some("custom-model"), "generic-agent");
    assert_eq!(chosen.as_deref(), Some("custom-model"));
}

#[test]
fn test_resolve_model_when_default_current_is_gpt6_luna() {
    let session = create_test_session_with_models(
        "gpt-6-luna",
        vec![
            ("gpt-6.1-sol", "Sol"),
            ("gpt-6-luna", "Luna"),
            ("gpt-5.6-terra", "Terra"),
        ],
    );

    let chosen = resolve_model_for_session(&session, None, "codex");
    assert_eq!(chosen.as_deref(), Some("gpt-6-luna"));
}

#[test]
fn test_resolve_model_fallback_picks_first_when_default_invalid() {
    let session = create_test_session_with_models(
        "unknown-default",
        vec![
            ("gpt-5.6-terra", "Terra"),
            ("gpt-6.1-sol", "Sol"),
            ("gpt-6-luna", "Luna"),
        ],
    );

    let chosen = resolve_model_for_session(&session, None, "codex");
    assert_eq!(chosen.as_deref(), Some("gpt-5.6-terra"));
}

#[test]
fn test_resolve_model_cross_agent_fallback_picks_valid_current() {
    let session = create_test_session_with_models(
        "gpt-6-luna",
        vec![("gpt-6-luna", "Luna"), ("gpt-5.6-terra", "Terra")],
    );

    let chosen = resolve_model_for_session(&session, Some("anthropic/claude-3-5"), "codex");
    assert_eq!(chosen.as_deref(), Some("gpt-6-luna"));
}

#[test]
fn test_resolve_model_generic_claude_agent_unsupported_model_falls_back_to_current() {
    let session = create_test_session_with_models(
        "claude-3-7-sonnet",
        vec![
            ("claude-3-7-sonnet", "Claude 3.7 Sonnet"),
            ("claude-3-5-haiku", "Claude 3.5 Haiku"),
        ],
    );

    // Foreign model request like "gpt-4o" to Claude agent falls back to Claude's current model
    let chosen = resolve_model_for_session(&session, Some("gpt-4o"), "claude");
    assert_eq!(chosen.as_deref(), Some("claude-3-7-sonnet"));
}

#[test]
fn test_resolve_model_generic_opencode_picks_exact_requested() {
    let session = create_test_session_with_models(
        "llama-3.3-70b",
        vec![
            ("llama-3.3-70b", "Llama 3.3"),
            ("qwen-2.5-coder-32b", "Qwen 2.5 Coder"),
        ],
    );

    let chosen = resolve_model_for_session(&session, Some("qwen-2.5-coder-32b"), "opencode");
    assert_eq!(chosen.as_deref(), Some("qwen-2.5-coder-32b"));
}
