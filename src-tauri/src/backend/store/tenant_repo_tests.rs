use super::*;
use crate::backend::store::Database;
use uuid::Uuid;

#[tokio::test]
async fn tenant_repo_loads_local_request_context() {
    let db_path =
        std::env::temp_dir().join(format!("assetiweave-tenant-{}.sqlite", Uuid::new_v4()));
    let database = Database::open_initialized_async(&db_path)
        .await
        .expect("open initialized database");

    let context = load_local_request_context_sqlx(database.pool())
        .await
        .expect("load local request context");

    assert_eq!(context.principal.id, LOCAL_PRINCIPAL_ID);
    assert_eq!(context.principal.kind, PrincipalKind::Local);
    assert_eq!(context.tenant.id, DEFAULT_TENANT_ID);
    assert_eq!(context.membership.role, TenantRole::Owner);
    assert_eq!(context.auth_mode, AuthMode::Local);
    drop(database);
    cleanup_database(&db_path);
}

fn cleanup_database(db_path: &std::path::Path) {
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}
