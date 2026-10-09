use super::*;

#[test]
fn test_account_creation() {
    let key_acc = AgentAccount::new_api_key(
        "acc-1",
        "My OpenAI Key",
        AccountProvider::OpenAi,
        "sk-test-123",
    );
    assert_eq!(key_acc.auth_type, AuthType::ApiKey);
    assert_eq!(key_acc.provider.as_str(), "openai");
    assert_eq!(key_acc.credential.as_deref(), Some("sk-test-123"));

    let oauth_acc =
        AgentAccount::new_oauth_profile("acc-2", "Codex Pro", AccountProvider::Codex, None);
    assert_eq!(oauth_acc.auth_type, AuthType::ConfigDirectory);
    assert_eq!(oauth_acc.provider.as_str(), "codex");
}

#[test]
fn test_provider_from_str() {
    assert_eq!(AccountProvider::from_str("Codex"), AccountProvider::Codex);
    assert_eq!(
        AccountProvider::from_str("antigravity"),
        AccountProvider::Antigravity
    );
    assert_eq!(
        AccountProvider::from_str("other_bot"),
        AccountProvider::Custom("other_bot".into())
    );
}
