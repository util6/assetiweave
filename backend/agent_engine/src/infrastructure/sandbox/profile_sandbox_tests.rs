use super::*;

#[test]
fn test_prepare_codex_sandbox() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = SandboxContext::new(temp.path());

    let acc =
        AgentAccount::new_oauth_profile("user-01", "Codex Account", AccountProvider::Codex, None);
    let prepared = ctx.prepare_profile_sandbox(&acc).unwrap();

    assert!(prepared.profile_dir.exists());
    assert!(prepared.profile_dir.ends_with("codex/user-01"));
    assert_eq!(
        prepared.env_vars.get("CODEX_HOME").map(|s| s.as_str()),
        Some(prepared.profile_dir.to_str().unwrap())
    );
}

#[test]
fn test_prepare_openai_key_sandbox() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = SandboxContext::new(temp.path());

    let acc = AgentAccount::new_api_key(
        "user-02",
        "OpenAI Key",
        AccountProvider::OpenAi,
        "sk-test-live",
    );
    let prepared = ctx.prepare_profile_sandbox(&acc).unwrap();

    assert_eq!(
        prepared.env_vars.get("OPENAI_API_KEY").map(|s| s.as_str()),
        Some("sk-test-live")
    );
    let token_file = prepared.profile_dir.join("token.txt");
    assert!(token_file.exists());
    assert_eq!(std::fs::read_to_string(token_file).unwrap(), "sk-test-live");
}
