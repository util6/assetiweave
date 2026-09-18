use super::*;
use std::error::Error;

#[test]
fn stored_json_error_preserves_codec_source_without_public_payload() {
    let malformed = "{\"secret_token\": 12345, bad_json}".to_string();
    let err = decode_json_app::<serde_json::Value>(malformed).unwrap_err();
    assert_eq!(err.code(), "storage_error");
    let mut found_serde = false;
    let mut cur: Option<&(dyn Error + 'static)> = err.source();
    while let Some(e) = cur {
        if e.is::<serde_json::Error>() {
            found_serde = true;
            break;
        }
        cur = e.source();
    }
    assert!(found_serde, "source chain must contain serde_json::Error");
    let view = err.view();
    assert_eq!(view.code, "storage_error");
    assert!(!view.message.contains("secret_token"));
    assert!(!view.message.contains("12345"));
    if let Some(details) = &view.details {
        assert!(!details.to_string().contains("secret_token"));
        assert!(!details.to_string().contains("12345"));
    }
}
