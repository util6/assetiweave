use crate::backend::application::AppService;
use crate::backend::infrastructure::tasks::{stages, StageStatus, TaskState};

async fn seed_indexed_session(service: &AppService, session_id: &str, body: &str) {
    let tenant_id = service.tenant_id();
    let turn_id = format!("{session_id}-turn");
    sqlx::query(
        r#"
        INSERT INTO conversation_sessions (
            tenant_id, id, source_id, adapter_id, external_id, title, project_path,
            started_at, updated_at, source_locator, source_fingerprint, missing,
            created_at, imported_at
        ) VALUES (?1, ?2, 'fixture-source', 'fixture-adapter', ?2, 'Fixture', NULL,
                  NULL, NULL, NULL, NULL, 0, '2026-10-23T00:00:00Z',
                  '2026-10-23T00:00:00Z')
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .execute(service.pool())
    .await
    .expect("seed session");
    sqlx::query(
        r#"
        INSERT INTO conversation_turns (
            tenant_id, id, session_id, external_id, turn_index, prompt, title,
            started_at, ended_at, fingerprint, missing, imported_at
        ) VALUES (?1, ?2, ?3, ?2, 0, 'index rebuild probe', NULL, NULL, NULL,
                  'fixture-fingerprint', 0, '2026-10-23T00:00:00Z')
        "#,
    )
    .bind(tenant_id)
    .bind(&turn_id)
    .bind(session_id)
    .execute(service.pool())
    .await
    .expect("seed turn");
    let part_id = crate::backend::domain::derive_conversation_part_id(&turn_id, 0);
    let id_key = crate::backend::domain::derive_part_id_key(&part_id).unwrap_or([0u8; 8]);
    sqlx::query(
        r#"
        INSERT INTO conversation_parts (
            tenant_id, turn_id, part_index, id_key, kind, content,
            attrs_json, translated_text
        ) VALUES (?1, ?2, 0, ?3, 'tool_result', ?4, NULL, NULL)
        "#,
    )
    .bind(tenant_id)
    .bind(&turn_id)
    .bind(id_key.to_vec())
    .bind(body)
    .execute(service.pool())
    .await
    .expect("seed part");
}

#[tokio::test]
async fn rebuild_part_index_runs_as_staged_pipeline_and_restores_body_matching() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-part-index-pipeline-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path.clone())
        .await
        .expect("open test service");
    seed_indexed_session(&service, "conversation-session-pipeline", "pipe probe body").await;

    // 索引清空 + 标记重建中：正文匹配应当不可用。
    sqlx::query("DELETE FROM conversation_part_fts")
        .execute(service.pool())
        .await
        .expect("wipe index");
    crate::backend::store::set_conversation_part_index_status_sqlx(
        service.pool(),
        service.tenant_id(),
        crate::backend::store::PART_INDEX_BUILDING,
        0,
        None,
    )
    .await
    .expect("mark building");
    let degraded = service
        .get_conversation_part_index_status()
        .await
        .expect("read status while building");
    assert!(!degraded.is_ready);

    let handle = service
        .rebuild_conversation_part_index()
        .expect("start part index rebuild pipeline");
    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("task snapshot");

    assert_eq!(snapshot.state, TaskState::Succeeded);
    let stage_ids: Vec<&str> = snapshot
        .stages
        .iter()
        .map(|stage| stage.id.as_str())
        .collect();
    assert_eq!(
        stage_ids,
        vec![stages::PREPARE, stages::PERSIST_INDEX, stages::VERIFY]
    );
    assert!(
        snapshot
            .stages
            .iter()
            .all(|stage| stage.status == StageStatus::Succeeded),
        "所有阶段都必须成功完成: {:?}",
        snapshot.stages
    );

    let status = service
        .get_conversation_part_index_status()
        .await
        .expect("read status after rebuild");
    assert!(status.is_ready);
    assert_eq!(status.part_count, 1);

    let indexed = sqlx::query_scalar::<_, String>(
        "SELECT text FROM conversation_part_fts WHERE tenant_id = ?1 AND session_id = ?2",
    )
    .bind(service.tenant_id())
    .bind("conversation-session-pipeline")
    .fetch_all(service.pool())
    .await
    .expect("read indexed rows");
    assert_eq!(indexed, vec!["pipe probe body".to_string()]);

    drop(service);
    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn part_index_status_defaults_to_ready_without_state_row() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-part-index-status-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path.clone())
        .await
        .expect("open test service");

    let status = service
        .get_conversation_part_index_status()
        .await
        .expect("read default status");
    assert!(status.is_ready);
    assert_eq!(status.status, crate::backend::store::PART_INDEX_READY);
    assert_eq!(status.part_count, 0);

    drop(service);
    let _ = std::fs::remove_file(db_path);
}
