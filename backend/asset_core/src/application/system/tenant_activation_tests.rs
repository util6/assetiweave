use super::*;
use crate::backend::application::AppService;
use crate::backend::infrastructure::runtime::RuntimeRole;
use std::path::PathBuf;
use uuid::Uuid;

fn temporary_database_path() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("assetiweave-tenant-act-{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&dir);
    dir.join("test.sqlite")
}

#[tokio::test]
async fn test_activate_tenant_success_and_fault_compensation() {
    let db_path = temporary_database_path();
    let runtime = AppService::bootstrap_runtime(db_path.clone(), RuntimeRole::OneShot)
        .await
        .expect("bootstrap runtime");

    let initial_context = runtime.context();
    let principal_id = &initial_context.request_context.principal.id;

    // 1. 创建新租户 tenant-beta
    let tenant_beta = crate::backend::store::create_local_tenant_sqlx(
        runtime.pool(),
        principal_id,
        "Tenant Beta",
        Some("tenant-beta"),
    )
    .await
    .expect("create tenant-beta");

    // 2. 正常激活 tenant-beta
    let activated = activate_tenant(&runtime, &tenant_beta.id)
        .await
        .expect("activate tenant-beta");
    assert_eq!(activated.id, tenant_beta.id);
    assert_eq!(runtime.context().tenant.id, tenant_beta.id);

    // 验证 DB 中也是 tenant-beta
    let current_db_context = crate::backend::store::load_local_request_context_sqlx(runtime.pool())
        .await
        .expect("load db context");
    assert_eq!(current_db_context.tenant.id, tenant_beta.id);

    // 3. 创建第三个租户 tenant-gamma
    let tenant_gamma = crate::backend::store::create_local_tenant_sqlx(
        runtime.pool(),
        principal_id,
        "Tenant Gamma",
        Some("tenant-gamma"),
    )
    .await
    .expect("create tenant-gamma");

    // 4. 故障注入激活：在加载上下文后注入失败
    let err = activate_tenant_with_fault(
        &runtime,
        &tenant_gamma.id,
        Some("after_load_request_context"),
    )
    .await
    .expect_err("should fail due to fault injection");

    assert!(err.to_string().contains("fault injected"));

    // 5. 验证状态一致性：DB active tenant 与 runtime context 均保持为 tenant-beta（成功回滚）
    assert_eq!(runtime.context().tenant.id, tenant_beta.id);
    let rolled_back_db_context =
        crate::backend::store::load_local_request_context_sqlx(runtime.pool())
            .await
            .expect("load db context after rollback");
    assert_eq!(rolled_back_db_context.tenant.id, tenant_beta.id);

    let _ = std::fs::remove_dir_all(db_path.parent().unwrap());
}
