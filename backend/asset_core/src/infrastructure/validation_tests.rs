use super::*;

#[test]
fn non_blank_rejects_empty_or_whitespace_strings() {
    assert!(validate_non_blank("").is_err());
    assert!(validate_non_blank("   ").is_err());
    assert!(validate_non_blank("\t\n ").is_err());
    assert!(validate_non_blank("valid").is_ok());
    assert!(validate_non_blank("  valid  ").is_ok());
}

#[test]
fn byte_limit_preserves_multibyte_boundary() {
    assert!(validate_max_120_bytes(&"界".repeat(40)).is_ok());
    assert!(validate_max_120_bytes(&"界".repeat(41)).is_err());
}

#[test]
fn max_500_bytes_works_correctly() {
    assert!(validate_max_500_bytes(&"a".repeat(500)).is_ok());
    assert!(validate_max_500_bytes(&"a".repeat(501)).is_err());
}

#[test]
fn no_null_bytes_works_correctly() {
    assert!(validate_no_null_bytes("hello world").is_ok());
    assert!(validate_no_null_bytes("hello\0world").is_err());
}

#[test]
fn sanitize_public_message_redacts_sensitive_patterns() {
    assert_eq!(
        sanitize_public_message("error at /var/data/users.json occurred"),
        "The operation failed."
    );
    assert_eq!(
        sanitize_public_message("error at ~/Documents/keys.pem occurred"),
        "The operation failed."
    );
    assert_eq!(
        sanitize_public_message("error at C:\\Users\\Admin\\config.ini occurred"),
        "The operation failed."
    );
    assert_eq!(
        sanitize_public_message("error at C:/Users/Admin/config.ini occurred"),
        "The operation failed."
    );
    assert_eq!(
        sanitize_public_message("SQL error near SELECT * FROM users"),
        "The operation failed."
    );
    assert_eq!(
        sanitize_public_message("invalid bearer token=xyz123"),
        "The operation failed."
    );
    assert_eq!(
        sanitize_public_message("leaked secret value in header"),
        "The operation failed."
    );
    assert_eq!(
        sanitize_public_message("invalid password provided for user"),
        "The operation failed."
    );
    assert_eq!(
        sanitize_public_message("invalid syntax in prompt=system_prompt"),
        "The operation failed."
    );
    assert_eq!(
        sanitize_public_message("missing prompt: user_input"),
        "The operation failed."
    );
    assert_eq!(
        sanitize_public_message("failed to read environment variable PATH"),
        "The operation failed."
    );
    assert_eq!(
        sanitize_public_message("file not found: item-42"),
        "file not found: item-42"
    );
}

#[test]
fn sanitize_details_removes_sensitive_keys() {
    let details = serde_json::json!({
        "token": "secret123",
        "safeKey": "safeValue",
        "nested": {
            "password": "pass",
            "count": 42
        }
    });
    let sanitized = sanitize_details(&details).expect("sanitized");
    assert_eq!(
        sanitized,
        serde_json::json!({
            "safeKey": "safeValue",
            "nested": {
                "count": 42
            }
        })
    );
}
