use super::*;

#[test]
fn tenant_kind_serde_preserves_snake_case() {
    assert_eq!(
        serde_json::to_string(&TenantKind::LocalWorkspace).unwrap(),
        "\"local_workspace\""
    );
    assert_eq!(
        serde_json::to_string(&TenantStatus::Active).unwrap(),
        "\"active\""
    );
}
