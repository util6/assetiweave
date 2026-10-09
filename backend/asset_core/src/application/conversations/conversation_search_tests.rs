use super::*;
use crate::backend::store::Database;
use std::path::PathBuf;
use uuid::Uuid;

const TENANT_ID: &str = "default";

fn temporary_database_path() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("assetiweave-test-{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&dir);
    dir.join("test.sqlite")
}

#[tokio::test]
async fn rebuild_fails_when_lease_is_held_by_another_writer() {
    let db_path = temporary_database_path();
    let database = Database::open_async(&db_path)
        .await
        .expect("open search state database");

    let now = Utc::now();
    let lease_expires_at = now + Duration::minutes(10);
    let acquired = crate::backend::store::try_acquire_conversation_search_writer_lease_sqlx(
        database.pool(),
        TENANT_ID,
        "existing-worker",
        &now.to_rfc3339(),
        &lease_expires_at.to_rfc3339(),
    )
    .await
    .expect("acquire lease");
    assert!(acquired);

    let err = rebuild_conversation_search_index(database.pool(), &db_path, TENANT_ID)
        .await
        .expect_err("should fail when lease is held");

    match err {
        AppError::Conflict(message) => {
            assert!(message.contains("already being rebuilt"));
        }
        other => panic!("expected AppError::Conflict, got {other:?}"),
    }

    drop(database);
    let _ = std::fs::remove_dir_all(db_path.parent().unwrap());
}

#[tokio::test]
async fn rebuild_and_search_lifecycle_and_version_upgrade() {
    let db_path = temporary_database_path();
    let database = Database::open_async(&db_path)
        .await
        .expect("open search state database");

    // 初始状态
    let initial_status =
        crate::backend::store::load_or_create_conversation_search_index_state_sqlx(
            database.pool(),
            TENANT_ID,
        )
        .await
        .expect("load initial state");
    assert_eq!(initial_status.source_revision, 0);

    // 执行第一次 rebuild
    let report = rebuild_conversation_search_index(database.pool(), &db_path, TENANT_ID)
        .await
        .expect("rebuild index");

    assert!(!report.generation.is_empty());
    assert_eq!(report.indexed_revision, 0);

    let state = crate::backend::store::load_or_create_conversation_search_index_state_sqlx(
        database.pool(),
        TENANT_ID,
    )
    .await
    .expect("load state after rebuild");
    assert_eq!(state.health.as_str(), "ready");
    assert_eq!(
        state.active_generation.as_deref(),
        Some(report.generation.as_str())
    );
    assert_eq!(state.indexed_revision, Some(0));
    assert!(state.lease_owner.is_none());

    // 验证搜索就绪
    let search_res = search_ready_conversation_index(
        database.pool(),
        &db_path,
        TENANT_ID,
        "test".to_string(),
        "chat".to_string(),
        vec![],
        vec![],
        false,
        false,
        None,
        None,
        None,
        10,
        0,
    )
    .await
    .expect("search index");
    assert!(search_res.is_some());

    // 模拟版本升级或源版本增加
    let mut tx = database.pool().begin().await.expect("begin tx");
    crate::backend::store::bump_conversation_search_source_revision_sqlx_tx(&mut *tx, TENANT_ID)
        .await
        .expect("bump revision");
    tx.commit().await.expect("commit tx");

    // source revision 增加后，索引处于 stale，search_ready_conversation_index 应返回 None
    let search_res_stale = search_ready_conversation_index(
        database.pool(),
        &db_path,
        TENANT_ID,
        "test".to_string(),
        "chat".to_string(),
        vec![],
        vec![],
        false,
        false,
        None,
        None,
        None,
        10,
        0,
    )
    .await
    .expect("search index stale");
    assert!(search_res_stale.is_none());

    // 再次 rebuild 更新至新 revision
    let report_v2 = rebuild_conversation_search_index(database.pool(), &db_path, TENANT_ID)
        .await
        .expect("rebuild index v2");
    assert_eq!(report_v2.indexed_revision, 1);
    assert_ne!(report_v2.generation, report.generation);

    let state_v2 = crate::backend::store::load_or_create_conversation_search_index_state_sqlx(
        database.pool(),
        TENANT_ID,
    )
    .await
    .expect("load state after rebuild v2");
    assert_eq!(state_v2.health.as_str(), "ready");
    assert_eq!(state_v2.indexed_revision, Some(1));

    drop(database);
    let _ = std::fs::remove_dir_all(db_path.parent().unwrap());
}

#[tokio::test]
async fn rebuild_recovers_lease_and_allows_immediate_retry_when_state_read_fails() {
    let db_path = temporary_database_path();
    let database = Database::open_async(&db_path)
        .await
        .expect("open search state database");

    // 初始状态
    let _ = crate::backend::store::load_or_create_conversation_search_index_state_sqlx(
        database.pool(),
        TENANT_ID,
    )
    .await
    .expect("init state");

    // 创建 SQLite 触发器：在 lease_owner 被更新（即成功获取写租约）后，立即破坏 schema_version
    // 使得随后的状态读取无法解码 i64，精准模拟“取得租约后内部读取状态失败”
    sqlx::query(
        r#"
        CREATE TRIGGER corrupt_state_on_lease_acquire
        AFTER UPDATE OF lease_owner ON conversation_search_index_state
        WHEN NEW.lease_owner LIKE 'rebuild-%'
        BEGIN
            UPDATE conversation_search_index_state
            SET schema_version = 'corrupted_schema'
            WHERE tenant_id = NEW.tenant_id;
        END;
        "#,
    )
    .execute(database.pool())
    .await
    .expect("create test trigger");

    // 第一次 rebuild：成功获取租约，但随后读取状态解码失败
    let err = rebuild_conversation_search_index(database.pool(), &db_path, TENANT_ID)
        .await
        .expect_err("rebuild should fail due to state read decoding failure");
    let err_str = err.to_string();
    assert!(
        err_str.contains("corrupted_schema")
            || err_str.to_lowercase().contains("decode")
            || err_str.to_lowercase().contains("mismatched types")
            || err_str.to_lowercase().contains("invalid type")
            || err_str.to_lowercase().contains("type mismatch"),
        "unexpected error: {err_str}"
    );

    // 删除触发器并恢复合法的 schema_version
    sqlx::query("DROP TRIGGER corrupt_state_on_lease_acquire")
        .execute(database.pool())
        .await
        .expect("drop test trigger");
    sqlx::query(
        "UPDATE conversation_search_index_state SET schema_version = 1 WHERE tenant_id = ?1",
    )
    .bind(TENANT_ID)
    .execute(database.pool())
    .await
    .expect("restore schema_version");

    // 验证租约已被清理，health 被置为 failed，lease_owner 为 NULL
    let (lease_owner, health): (Option<String>, String) = sqlx::query_as(
        "SELECT lease_owner, health FROM conversation_search_index_state WHERE tenant_id = ?1",
    )
    .bind(TENANT_ID)
    .fetch_one(database.pool())
    .await
    .expect("fetch state raw row");
    assert_eq!(health.as_str(), "failed");
    assert!(lease_owner.is_none());

    // 第二次 rebuild：可以立即再次重建，而不会因为租约泄漏报 Conflict ("already being rebuilt")
    let retry_report = rebuild_conversation_search_index(database.pool(), &db_path, TENANT_ID)
        .await
        .expect("immediate retry should succeed because lease was released");
    assert!(!retry_report.generation.is_empty());

    let state_after_retry =
        crate::backend::store::load_or_create_conversation_search_index_state_sqlx(
            database.pool(),
            TENANT_ID,
        )
        .await
        .expect("load state after retry");
    assert_eq!(state_after_retry.health.as_str(), "ready");

    drop(database);
    let _ = std::fs::remove_dir_all(db_path.parent().unwrap());
}
