use super::*;

#[test]
fn postflight_metadata_is_emitted_for_failed_commands() {
    let (registry, mut invocation) = before("delete_source");
    after(&registry, &mut invocation, Some("command_denied"));
    let meta = response_meta(&invocation);

    assert_eq!(meta["canonical_method"], json!("source.remove"));
    assert_eq!(meta["risk"], json!("high-risk-write"));
    assert_eq!(meta["outcome"], json!("error"));
    assert_eq!(meta["error_type"], json!("command_denied"));
    assert_eq!(meta["hooks"], json!(["runtime.timing"]));
    assert!(meta["duration_ms"].is_u64());
}
