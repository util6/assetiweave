use super::catalog::SourceKind;

#[test]
fn domain_source_kind_preserves_existing_serde_names() {
    let value = serde_json::to_value(SourceKind::Local).unwrap();
    assert_eq!(value, serde_json::json!("local"));
}
