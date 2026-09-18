use super::*;

#[test]
fn redacts_supported_secret_fixtures_before_external_ai_use() {
    let input = concat!(
        "OpenAI: sk-proj-1234567890abcdefghijklmnopqrstuvwxyz\n",
        "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.abcdefghijklmnopqrstuvwxyz.1234567890\n",
        "Cookie: session=super-secret-cookie; theme=dark\n",
        "-----BEGIN PRIVATE KEY-----\n",
        "MIIEvQIBADANBgkqhkiG9w0BAQEFAASC1234567890abcdefghijklmnopqrstuvwxyz\n",
        "-----END PRIVATE KEY-----\n",
        "opaque=QWxhZGRpbjpPcGVuU2VzYW1lMTIzNDU2Nzg5MC9hYmNkZWZnaGlqa2xtbm9w\n",
        "safe=short-value\n",
    );

    let result = redact_memory_text(input);

    assert!(!result.text.contains("sk-proj-"));
    assert!(!result.text.contains("eyJhbGci"));
    assert!(!result.text.contains("super-secret-cookie"));
    assert!(!result.text.contains("MIIEvQIB"));
    assert!(!result.text.contains("QWxhZGRp"));
    assert!(result.text.contains("safe=short-value"));
    assert!(result.redaction_count >= 5);
}

#[test]
fn redaction_preserves_non_secret_code_entities_and_hashes() {
    let git_sha = "bc5c14e1234567890abcdef1234567890abcdef1";
    let sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    let uuid = "5ebbb321-00bb-4a1e-b829-5e9d04a5dca0";
    let unix_path =
        "/Users/developer/code-space/assetiweave/src-tauri/src/backend/session_memory.rs";
    let windows_path = r"C:\Users\admin\workspace\assetiweave\src\index.ts";
    let long_fn_name = "test_internal_source_isolation_hides_agent_sessions_from_views_and_memory";

    let input = format!(
        "commit: {git_sha}\nchecksum: {sha256}\nsession: {uuid}\nfile1: {unix_path}\nfile2: {windows_path}\nfn: {long_fn_name}\n"
    );

    let result = redact_memory_text(&input);

    assert!(
        result.text.contains(git_sha),
        "Git 40-hex SHA must be preserved without redaction"
    );
    assert!(
        result.text.contains(sha256),
        "SHA256 64-hex checksum must be preserved without redaction"
    );
    assert!(
        result.text.contains(uuid),
        "UUID must be preserved without redaction"
    );
    assert!(
        result.text.contains(unix_path),
        "UNIX file path must be preserved without redaction"
    );
    assert!(
        result.text.contains(windows_path),
        "Windows file path must be preserved without redaction"
    );
    assert!(
        result.text.contains(long_fn_name),
        "Long test/function name must be preserved without redaction"
    );
    assert_eq!(
        result.redaction_count, 0,
        "Negative samples should not trigger any redaction"
    );
}

#[test]
fn redaction_catches_all_specified_api_credentials() {
    let sample = concat!(
        "github: ghp_1234567890abcdefghijklmnopqrstuvwxyz\n",
        "aws: AKIAIOSFODNN7EXAMPLE\n",
        "slack: xoxb-1234567890-abcdef123456\n",
        "google: AIzaSyD-1234567890abcdefghijklmnopqrst\n",
        "jwt: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozGz_abcdef1234567890\n"
    );

    let result = redact_memory_text(sample);

    assert!(!result.text.contains("ghp_"));
    assert!(!result.text.contains("AKIAIOSFODNN7EXAMPLE"));
    assert!(!result.text.contains("xoxb-"));
    assert!(!result.text.contains("AIzaSyD"));
    assert!(!result.text.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    assert!(result.redaction_count >= 5);
}

#[test]
fn redaction_is_idempotent() {
    let first = redact_memory_text("Bearer abcdefghijklmnopqrstuvwxyz1234567890");
    let second = redact_memory_text(&first.text);

    assert_eq!(second.text, first.text);
    assert_eq!(second.redaction_count, 0);
}
