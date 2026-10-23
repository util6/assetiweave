use super::*;
use crate::backend::domain::conversations::{
    projection::ConversationCardRenderer, ConversationRecordKind, ConversationSearchCardType,
};
use crate::backend::domain::{
    ConversationContentCardDescriptor, ConversationPartKind, ConversationPartRole,
    ConversationSourceKind, NormalizedConversationPart, NormalizedConversationTurn,
};
use crate::backend::store::Database;
use uuid::Uuid;

const TEST_TENANT_ID: &str = "default";

#[tokio::test]
async fn sqlx_web_records_use_independent_tables_and_remove_legacy_session_rows() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-import-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let source = fixture_source();

    let (legacy_count_before_import, legacy_count_after_import, sessions, detail) = async {
        super::super::conversation_repo::upsert_conversation_source_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
        )
        .await?;
        super::super::conversation_repo::import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[fixture_session()],
            false,
        )
        .await?;
        let legacy_count_before_import =
            count_legacy_conversation_sessions_sqlx(database.pool(), &source.id)
                .await
                .map_err(StoreError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[fixture_session()],
            false,
        )
        .await?;
        let legacy_count_after_import =
            count_legacy_conversation_sessions_sqlx(database.pool(), &source.id).await?;
        let sessions = list_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            None,
            20,
            0,
        )
        .await?;
        let detail = load_web_record_session_detail_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &sessions[0].session.id,
        )
        .await?;
        StoreResult::Ok((
            legacy_count_before_import,
            legacy_count_after_import,
            sessions,
            detail,
        ))
    }
    .await
    .expect("import and read web records through SQLx");

    assert_eq!(legacy_count_before_import, 1);
    assert_eq!(legacy_count_after_import, 0);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].question_count, 1);
    assert_eq!(sessions[0].turn_count, 1);
    assert_eq!(detail.questions.len(), 1);
    assert_eq!(detail.questions[0].turns[0].user_text, "Hello from the web");
    assert!(detail.questions[0]
        .projected_content_nodes
        .iter()
        .any(|node| node.content == "Web answer"));

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_web_record_details_derive_execution_pairs_from_parts() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-execution-pairs-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let source = fixture_source();
    let mut session = fixture_session();
    session.turns[0].parts = vec![
        NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Command,
            text: None,
            language: None,
            command: Some("cat README.md".to_string()),
            cwd: Some("/tmp/qwen".to_string()),
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: Some("exec-web-1".to_string()),
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "qwen-web.command".to_string(),
                semantic_role: Some("command".to_string()),
                renderer: Some("command".to_string()),
            }),
            metadata_json: None,
        },
        NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::Tool,
            text: Some("# AssetIWeave".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: Some("success".to_string()),
            exit_code: Some(0),
            command_label: None,
            source_execution_id: Some("exec-web-1".to_string()),
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "qwen-web.result".to_string(),
                semantic_role: Some("result".to_string()),
                renderer: Some("terminal_output".to_string()),
            }),
            metadata_json: None,
        },
    ];

    let detail = async {
        super::super::conversation_repo::upsert_conversation_source_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
        )
        .await
        .map_err(StoreError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[session],
            false,
        )
        .await
        .map_err(StoreError::external)?;
        let sessions = list_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            None,
            20,
            0,
        )
        .await?;
        load_web_record_session_detail_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &sessions[0].session.id,
        )
        .await
    }
    .await
    .expect("derive web execution pairs from parts");

    let execution_nodes = detail.questions[0]
        .projected_content_nodes
        .iter()
        .filter(|node| node.source_execution_id.as_deref() == Some("exec-web-1"))
        .collect::<Vec<_>>();
    assert_eq!(execution_nodes.len(), 2);
    assert_eq!(execution_nodes[0].semantic_role.as_deref(), Some("command"));
    assert_eq!(execution_nodes[1].semantic_role.as_deref(), Some("result"));
    assert!(execution_nodes[0].part_order < execution_nodes[1].part_order);
    assert_eq!(execution_nodes[1].status.as_deref(), Some("success"));
    assert_eq!(execution_nodes[1].exit_code, Some(0));

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_web_record_sessions_do_not_store_project_paths() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-no-project-path-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let source = fixture_source();
    let mut session = fixture_session();
    session.project_path = Some("/tmp/web-project".to_string());

    let (columns, sessions, detail) = async {
        super::super::conversation_repo::upsert_conversation_source_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
        )
        .await?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[session],
            false,
        )
        .await
        .map_err(StoreError::external)?;
        let columns = sqlx::query_scalar::<_, String>(
            "SELECT name FROM pragma_table_info('web_record_sessions')",
        )
        .fetch_all(database.pool())
        .await
        .map_err(StoreError::external)?;
        let sessions = list_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            None,
            20,
            0,
        )
        .await?;
        let detail = load_web_record_session_detail_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &sessions[0].session.id,
        )
        .await?;
        StoreResult::Ok((columns, sessions, detail))
    }
    .await
    .expect("import web records without persisting project paths");

    assert!(!columns.iter().any(|column| column == "project_path"));
    assert_eq!(sessions[0].session.project_path, None);
    assert_eq!(detail.session.project_path, None);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_web_record_legacy_cleanup_is_tenant_scoped() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-legacy-cleanup-tenant-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let tenant_alpha = "tenant-alpha";
    let tenant_beta = "tenant-beta";
    let source = fixture_source();

    let (beta_before, beta_after, alpha_legacy_count) = async {
        for tenant_id in [tenant_alpha, tenant_beta] {
            super::super::conversation_repo::upsert_conversation_source_sqlx(
                database.pool(),
                tenant_id,
                &source,
            )
            .await
            .map_err(StoreError::external)?;
            super::super::conversation_repo::import_conversation_sessions_sqlx(
                database.pool(),
                tenant_id,
                &source,
                &[fixture_session()],
                false,
            )
            .await
            .map_err(StoreError::external)?;
        }

        let beta_sessions = super::super::conversation_repo::list_conversation_sessions_sqlx(
            database.pool(),
            tenant_beta,
            None,
            Some(&source.id),
            None,
            20,
            0,
        )
        .await
        .map_err(StoreError::external)?;
        let beta_detail = super::super::conversation_repo::load_conversation_session_detail_sqlx(
            database.pool(),
            tenant_beta,
            &beta_sessions[0].session.id,
        )
        .await
        .map_err(StoreError::external)?;
        let beta_before = (
            beta_detail.questions[0].turns.len(),
            beta_detail.questions[0].parts.len(),
        );

        import_web_record_sessions_sqlx(
            database.pool(),
            tenant_alpha,
            &source,
            &[fixture_session()],
            false,
        )
        .await?;

        let beta_detail = super::super::conversation_repo::load_conversation_session_detail_sqlx(
            database.pool(),
            tenant_beta,
            &beta_sessions[0].session.id,
        )
        .await
        .map_err(StoreError::external)?;
        let beta_after = (
            beta_detail.questions[0].turns.len(),
            beta_detail.questions[0].parts.len(),
        );
        let alpha_legacy_count = super::super::conversation_repo::list_conversation_sessions_sqlx(
            database.pool(),
            tenant_alpha,
            None,
            Some(&source.id),
            None,
            20,
            0,
        )
        .await
        .map_err(StoreError::external)?
        .len();

        StoreResult::Ok((beta_before, beta_after, alpha_legacy_count))
    }
    .await
    .expect("web record legacy cleanup stays tenant-scoped");

    assert_eq!(beta_before, (1, 1));
    assert_eq!(beta_after, (1, 1));
    assert_eq!(alpha_legacy_count, 0);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_web_record_import_skips_unchanged_fingerprinted_sessions() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-import-skip-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let source = fixture_source();
    let mut session = fixture_session();
    session.source_fingerprint = Some("unchanged".to_string());

    let imported_at = async {
        super::super::conversation_repo::upsert_conversation_source_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
        )
        .await
        .map_err(|error| error.to_string())?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[session.clone()],
            false,
        )
        .await
        .map_err(|error| error.to_string())?;
        sqlx::query(
            "UPDATE web_record_sessions SET imported_at = 'preserved' WHERE source_id = ?1",
        )
        .bind(&source.id)
        .execute(database.pool())
        .await
        .map_err(|error| error.to_string())?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[session],
            false,
        )
        .await
        .map_err(|error| error.to_string())?;
        sqlx::query_scalar::<_, String>(
            "SELECT imported_at FROM web_record_sessions WHERE source_id = ?1",
        )
        .bind(&source.id)
        .fetch_one(database.pool())
        .await
        .map_err(|error| error.to_string())
    }
    .await
    .expect("import unchanged fingerprinted web session through SQLx");

    assert_eq!(imported_at, "preserved");

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn conversation_incremental_web_import_retains_sessions_omitted_by_source() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-import-retain-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let source = fixture_source();
    let current_session = fixture_session();
    let mut archived_session = fixture_session();
    archived_session.external_id = "archived-web-session".to_string();
    archived_session.title = Some("Archived web fixture".to_string());

    let (listed, retained_detail) = async {
        super::super::conversation_repo::upsert_conversation_source_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
        )
        .await
        .map_err(StoreError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[current_session.clone(), archived_session],
            false,
        )
        .await
        .map_err(StoreError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[current_session],
            false,
        )
        .await?;
        let listed = list_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            None,
            20,
            0,
        )
        .await?;
        let retained_id = stable_id("web-record-session", &[&source.id, "archived-web-session"]);
        let retained_detail =
            load_web_record_session_detail_sqlx(database.pool(), TEST_TENANT_ID, &retained_id)
                .await?;
        StoreResult::Ok((listed, retained_detail))
    }
    .await
    .expect("retain omitted web record sessions through SQLx");

    assert_eq!(listed.len(), 2);
    assert!(listed
        .iter()
        .any(|item| item.session.external_id == "archived-web-session"));
    assert_eq!(retained_detail.session.external_id, "archived-web-session");
    assert_eq!(retained_detail.questions.len(), 1);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_web_record_import_rewrites_when_normalized_parts_change() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-import-refresh-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let source = fixture_source();
    let mut old_session = fixture_session();
    old_session.source_fingerprint = Some("same-source".to_string());
    old_session.turns[0].parts[0].metadata_json = None;
    let mut refreshed_session = fixture_session();
    refreshed_session.source_fingerprint = Some("same-source".to_string());
    refreshed_session.turns[0].parts[0].metadata_json = content_card_metadata("answer");

    let (result, imported_at, metadata_json, indexed_rows) = async {
        super::super::conversation_repo::upsert_conversation_source_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
        )
        .await
        .map_err(StoreError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[old_session],
            false,
        )
        .await
        .map_err(StoreError::external)?;
        let session_id = stable_id("web-record-session", &[&source.id, "web-session-1"]);
        let turn_id: String = sqlx::query_scalar(
            "SELECT id FROM web_record_turns WHERE tenant_id = ?1 AND session_id = ?2 LIMIT 1",
        )
        .bind(TEST_TENANT_ID)
        .bind(&session_id)
        .fetch_one(database.pool())
        .await
        .map_err(StoreError::external)?;
        sqlx::query(
            r#"
                    INSERT INTO conversation_part_fts (
                        tenant_id, part_id, session_id, turn_id, text
                    ) VALUES (?1, 'web-record-part-stale', ?2, ?3, 'stale body')
                    "#,
        )
        .bind(TEST_TENANT_ID)
        .bind(&session_id)
        .bind(&turn_id)
        .execute(database.pool())
        .await
        .map_err(StoreError::external)?;
        sqlx::query(
            "UPDATE web_record_sessions SET imported_at = 'preserved' WHERE source_id = ?1",
        )
        .bind(&source.id)
        .execute(database.pool())
        .await
        .map_err(StoreError::external)?;
        let result = import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[refreshed_session],
            false,
        )
        .await?;
        let imported_at = sqlx::query_scalar::<_, String>(
            "SELECT imported_at FROM web_record_sessions WHERE source_id = ?1",
        )
        .bind(&source.id)
        .fetch_one(database.pool())
        .await
        .map_err(StoreError::external)?;
        let metadata_json = sqlx::query_scalar::<_, Option<String>>(
            r#"
                    SELECT p.metadata_json
                    FROM web_record_parts p
                    JOIN web_record_turns t ON t.id = p.turn_id
                    JOIN web_record_sessions s ON s.id = t.session_id
                    WHERE s.source_id = ?1
                    ORDER BY p.part_index ASC
                    LIMIT 1
                    "#,
        )
        .bind(&source.id)
        .fetch_one(database.pool())
        .await
        .map_err(StoreError::external)?;
        let fts_row_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM conversation_part_fts WHERE tenant_id = ?1 AND session_id = ?2",
        )
        .bind(TEST_TENANT_ID)
        .bind(&session_id)
        .fetch_one(database.pool())
        .await
        .map_err(StoreError::external)?;
        let fts_texts = sqlx::query_scalar::<_, String>(
            "SELECT text FROM conversation_part_fts WHERE tenant_id = ?1 AND session_id = ?2",
        )
        .bind(TEST_TENANT_ID)
        .bind(&session_id)
        .fetch_all(database.pool())
        .await
        .map_err(StoreError::external)?;
        StoreResult::Ok((
            result,
            imported_at,
            metadata_json,
            (fts_row_count, fts_texts),
        ))
    }
    .await
    .expect("refresh normalized web parts through SQLx");

    assert_eq!(result.skipped_session_count, 0);
    assert_ne!(imported_at, "preserved");
    assert!(metadata_json
        .as_deref()
        .unwrap_or("")
        .contains(r#""content_card""#));
    let (fts_row_count, fts_texts) = indexed_rows;
    assert_eq!(fts_row_count, 1, "轮次级重建不应留下重复或过期索引行");
    assert_eq!(fts_texts.len(), 1);
    assert!(
        !fts_texts[0].contains("stale body"),
        "重新导入后索引正文必须来自当前零件投影"
    );

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_web_record_reads_filter_detail() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-read-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let source = fixture_source();
    let first = fixture_session();
    let mut second = fixture_session();
    second.external_id = "web-session-2".to_string();
    second.title = Some("SQLx migration notes".to_string());
    second.project_path = Some("/tmp/sqlx-project".to_string());
    second.turns[0].external_id = "turn-2".to_string();
    second.turns[0].user_text = "How is the read path migrated?".to_string();
    second.turns[0].parts[0].text = Some("Loaded through SQLx answer".to_string());

    let (sessions, detail) = async {
        super::super::conversation_repo::upsert_conversation_source_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
        )
        .await
        .map_err(StoreError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[first, second],
            false,
        )
        .await
        .map_err(StoreError::external)?;
        let sessions = list_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            Some("sqlx answer"),
            20,
            0,
        )
        .await?;
        let detail = load_web_record_session_detail_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &sessions[0].session.id,
        )
        .await?;
        StoreResult::Ok((sessions, detail))
    }
    .await
    .expect("read web records through SQLx");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session.title, "SQLx migration notes");
    assert_eq!(sessions[0].question_count, 1);
    assert_eq!(sessions[0].turn_count, 1);
    assert_eq!(detail.questions.len(), 1);
    assert_eq!(detail.questions[0].turns.len(), 1);
    assert_eq!(detail.questions[0].parts.len(), 1);
    assert_eq!(detail.questions[0].question_turns.len(), 1);
    assert_eq!(
        detail.questions[0].question_turns[0].turn_id,
        detail.questions[0].turns[0].id
    );

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_web_record_lists_sessions_by_display_id_fragment() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-id-fragment-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let source = fixture_source();

    let (fragment_matches, direct_fragment_matches, full_matches) = async {
        super::super::conversation_repo::upsert_conversation_source_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
        )
        .await
        .map_err(StoreError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[fixture_session()],
            false,
        )
        .await
        .map_err(StoreError::external)?;
        let session_id = stable_id("web-record-session", &[&source.id, "web-session-1"]);
        let fragment = crate::backend::domain::conversation_id_fragment(&session_id);
        let fragment_matches = list_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            Some(&fragment),
            20,
            0,
        )
        .await?;
        let direct_fragment_matches =
            super::super::conversation_repo::list_conversation_sessions_by_id_fragment_sqlx(
                database.pool(),
                TEST_TENANT_ID,
                crate::backend::domain::conversations::ConversationRecordKind::Web,
                None,
                Some(&source.id),
                &fragment,
                20,
                0,
            )
            .await
            .map_err(StoreError::external)?;
        let full_matches = list_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            Some(&session_id),
            20,
            0,
        )
        .await?;
        StoreResult::Ok((fragment_matches, direct_fragment_matches, full_matches))
    }
    .await
    .expect("list web record sessions by display id fragment");

    assert_eq!(fragment_matches.len(), 1);
    assert_eq!(direct_fragment_matches.len(), 1);
    assert_eq!(full_matches.len(), 1);
    assert_eq!(full_matches[0].session.external_id, "web-session-1");

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_web_record_aggregates_only_declared_content_cards() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-declared-cards-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let source = fixture_source();
    let mut session = fixture_session();
    session.turns[0].parts[0].text = Some("undeclared web answer".to_string());
    session.turns[0].parts[0].metadata_json = None;
    session.turns.push(NormalizedConversationTurn {
        external_id: "turn-2".to_string(),
        turn_index: 1,
        user_text: "Second web question".to_string(),
        title: None,
        started_at: None,
        ended_at: None,
        model: None,
        parts: vec![NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("declared web answer".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: None,
            metadata_json: content_card_metadata("answer"),
        }],
    });

    let detail = async {
        super::super::conversation_repo::upsert_conversation_source_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
        )
        .await
        .map_err(StoreError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[session],
            false,
        )
        .await
        .map_err(StoreError::external)?;
        let sessions = list_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            None,
            20,
            0,
        )
        .await?;
        load_web_record_session_detail_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &sessions[0].session.id,
        )
        .await
    }
    .await
    .expect("aggregate declared web content cards through SQLx");

    assert!(!detail.questions[0]
        .projected_content_nodes
        .iter()
        .any(|node| node.semantic_role.as_deref() == Some("answer")));
    assert_eq!(
        detail.questions[1]
            .projected_content_nodes
            .iter()
            .find(|node| node.semantic_role.as_deref() == Some("answer"))
            .map(|node| node.content.as_str()),
        Some("declared web answer")
    );

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_web_records_are_isolated_by_tenant() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-tenant-isolation-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let tenant_alpha = "tenant-alpha";
    let tenant_beta = "tenant-beta";
    let source = fixture_source();
    let mut alpha_session = fixture_session();
    alpha_session.turns[0].parts[0].text = Some("alpha web answer".to_string());
    let mut beta_session = fixture_session();
    beta_session.turns[0].parts[0].text = Some("beta web answer".to_string());

    let (session_id, alpha_detail, beta_detail, alpha_page, beta_page) = async {
        for tenant_id in [tenant_alpha, tenant_beta] {
            super::super::conversation_repo::upsert_conversation_source_sqlx(
                database.pool(),
                tenant_id,
                &source,
            )
            .await
            .map_err(StoreError::external)?;
        }
        import_web_record_sessions_sqlx(
            database.pool(),
            tenant_alpha,
            &source,
            &[alpha_session],
            false,
        )
        .await
        .map_err(StoreError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            tenant_beta,
            &source,
            &[beta_session],
            false,
        )
        .await?;

        let alpha_sessions = list_web_record_sessions_sqlx(
            database.pool(),
            tenant_alpha,
            None,
            Some(&source.id),
            Some("alpha web"),
            20,
            0,
        )
        .await?;
        let beta_sessions = list_web_record_sessions_sqlx(
            database.pool(),
            tenant_beta,
            None,
            Some(&source.id),
            Some("beta web"),
            20,
            0,
        )
        .await?;
        let session_id = alpha_sessions[0].session.id.clone();
        assert_eq!(beta_sessions[0].session.id, session_id);
        let alpha_detail =
            load_web_record_session_detail_sqlx(database.pool(), tenant_alpha, &session_id).await?;
        let beta_detail =
            load_web_record_session_detail_sqlx(database.pool(), tenant_beta, &session_id).await?;
        let alpha_page = super::super::conversation_repo::search_conversation_cards_sqlx(
            database.pool(),
            tenant_alpha,
            ConversationRecordKind::Web,
            Some(&source.adapter_id),
            Some(&source.id),
            None,
            "beta web answer",
            &[ConversationSearchCardType::answer()],
            &[],
            false,
            true,
            None,
            None,
            false,
            20,
            0,
            None,
        )
        .await
        .map_err(StoreError::external)?;
        let beta_page = super::super::conversation_repo::search_conversation_cards_sqlx(
            database.pool(),
            tenant_beta,
            ConversationRecordKind::Web,
            Some(&source.adapter_id),
            Some(&source.id),
            None,
            "alpha web answer",
            &[ConversationSearchCardType::answer()],
            &[],
            false,
            true,
            None,
            None,
            false,
            20,
            0,
            None,
        )
        .await
        .map_err(StoreError::external)?;
        StoreResult::Ok((session_id, alpha_detail, beta_detail, alpha_page, beta_page))
    }
    .await
    .expect("isolate web records by tenant");

    assert_eq!(alpha_detail.session.id, session_id);
    assert_eq!(beta_detail.session.id, session_id);
    assert!(alpha_detail.questions[0]
        .projected_content_nodes
        .iter()
        .any(|node| node.content == "alpha web answer"));
    assert!(beta_detail.questions[0]
        .projected_content_nodes
        .iter()
        .any(|node| node.content == "beta web answer"));
    assert_eq!(alpha_page.total_count, 0);
    assert_eq!(beta_page.total_count, 0);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_web_records_round_trip_structured_cards_and_preserve_translation() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-card-persistence-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let source = fixture_source();
    let mut first = fixture_session();
    first.turns[0].parts[0].content_card = Some(ConversationContentCardDescriptor {
        schema_version: 1,
        kind: "qwen-web.reasoning".to_string(),
        semantic_role: None,
        renderer: Some("markdown".to_string()),
    });
    let mut second = first.clone();
    second.turns[0].parts[0].content_card.as_mut().unwrap().kind = "qwen-web.analysis".to_string();

    let (part_id, detail) = async {
        super::super::conversation_repo::upsert_conversation_source_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
        )
        .await
        .map_err(StoreError::external)?;
        import_web_record_sessions_sqlx(database.pool(), TEST_TENANT_ID, &source, &[first], false)
            .await
            .map_err(StoreError::external)?;
        let session_id = stable_id("web-record-session", &[&source.id, "web-session-1"]);
        let initial =
            load_web_record_session_detail_sqlx(database.pool(), TEST_TENANT_ID, &session_id)
                .await?;
        let part_id = initial.questions[0].parts[0].id.clone();
        update_web_record_part_translation_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &part_id,
            "网页译文",
        )
        .await?;
        import_web_record_sessions_sqlx(database.pool(), TEST_TENANT_ID, &source, &[second], false)
            .await?;
        let detail =
            load_web_record_session_detail_sqlx(database.pool(), TEST_TENANT_ID, &session_id)
                .await?;
        StoreResult::Ok((part_id, detail))
    }
    .await
    .expect("round trip web record card");

    let part = &detail.questions[0].parts[0];
    assert_eq!(part.id, part_id);
    assert_eq!(part.translated_text.as_deref(), Some("网页译文"));
    assert_eq!(
        part.content_card.as_ref().map(|card| card.kind.as_str()),
        Some("qwen-web.analysis")
    );
    assert_eq!(detail.questions[0].projected_content_nodes.len(), 1);
    assert_eq!(
        detail.questions[0].projected_content_nodes[0].part_id,
        part_id
    );
    assert_eq!(
        detail.questions[0].projected_content_nodes[0].renderer,
        crate::backend::domain::conversations::projection::ConversationCardRenderer::Markdown
    );

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_web_record_question_columns_mirror_membership_and_freeze_ids() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-question-columns-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let source = fixture_source();
    let template = fixture_session().turns[0].clone();
    let mut initial = fixture_session();
    initial.turns = vec![
        web_turn(&template, "turn-1", 0, "How does sync work?"),
        web_turn(&template, "turn-2", 1, "Export it"),
    ];
    let mut reconciled = initial.clone();
    reconciled.turns[1].user_text = "继续".to_string();
    let mut appended = reconciled.clone();
    appended
        .turns
        .push(web_turn(&template, "turn-3", 2, "继续"));

    let (initial_reads, reconciled_reads, appended_reads, detail) = async {
        super::super::conversation_repo::upsert_conversation_source_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
        )
        .await
        .map_err(StoreError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[initial],
            false,
        )
        .await
        .map_err(StoreError::external)?;
        let initial_reads = load_web_turn_question_reads(database.pool()).await?;

        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[reconciled],
            false,
        )
        .await
        .map_err(StoreError::external)?;
        let reconciled_reads = load_web_turn_question_reads(database.pool()).await?;

        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[appended],
            false,
        )
        .await
        .map_err(StoreError::external)?;
        let appended_reads = load_web_turn_question_reads(database.pool()).await?;

        let session_id = stable_id("web-record-session", &[&source.id, "web-session-1"]);
        let detail =
            load_web_record_session_detail_sqlx(database.pool(), TEST_TENANT_ID, &session_id)
                .await?;
        StoreResult::Ok((initial_reads, reconciled_reads, appended_reads, detail))
    }
    .await
    .expect("freeze web record question ids");

    for reads in [&initial_reads, &reconciled_reads, &appended_reads] {
        assert_web_turn_question_reads_agree(reads);
    }
    let question_id_of = |reads: &BTreeMap<String, (String, String)>, external_id: &str| {
        reads
            .get(external_id)
            .unwrap_or_else(|| panic!("turn {external_id} has a question read"))
            .0
            .clone()
    };
    let origin_of = |reads: &BTreeMap<String, (String, String)>, external_id: &str| {
        reads
            .get(external_id)
            .unwrap_or_else(|| panic!("turn {external_id} has a question read"))
            .1
            .clone()
    };

    let first_question_id = question_id_of(&initial_reads, "turn-1");
    let second_question_id = question_id_of(&initial_reads, "turn-2");
    assert_ne!(first_question_id, second_question_id);
    assert_eq!(
        question_id_of(&reconciled_reads, "turn-2"),
        second_question_id,
        "web record question ids stay frozen across a rewritten session"
    );
    assert_eq!(
        question_id_of(&appended_reads, "turn-3"),
        second_question_id,
        "an acknowledgement turn joins the previous web record group"
    );
    assert_eq!(origin_of(&appended_reads, "turn-2"), "auto_merged");
    assert_eq!(origin_of(&appended_reads, "turn-3"), "auto_merged");
    assert_eq!(origin_of(&appended_reads, "turn-1"), "imported");

    assert_eq!(detail.questions.len(), 2);
    assert_eq!(detail.questions[0].turns.len(), 1);
    assert_eq!(detail.questions[1].turns.len(), 2);
    assert_eq!(detail.questions[1].turns[1].external_id, "turn-3");

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_web_record_block_reads_use_turn_columns_after_legacy_rows_are_dropped() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-read-path-turn-columns-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let source = fixture_source();
    let template = fixture_session().turns[0].clone();
    let mut session = fixture_session();
    session.turns = vec![
        web_turn(&template, "turn-1", 0, "How does sync work?"),
        web_turn(&template, "turn-2", 1, "Export it"),
    ];
    let pool = database.pool();

    super::super::conversation_repo::upsert_conversation_source_sqlx(pool, TEST_TENANT_ID, &source)
        .await
        .expect("upsert source");
    import_web_record_sessions_sqlx(pool, TEST_TENANT_ID, &source, &[session], false)
        .await
        .expect("import web record session");

    let session_id = stable_id("web-record-session", &[&source.id, "web-session-1"]);
    let question_id: String = sqlx::query_scalar(
        "SELECT question_id FROM web_record_turns WHERE tenant_id = ?1 AND external_id = 'turn-1'",
    )
    .bind(TEST_TENANT_ID)
    .fetch_one(pool)
    .await
    .expect("load web turn question column");
    for statement in [
        "DELETE FROM web_record_question_turns WHERE tenant_id = ?1",
        "DELETE FROM web_record_questions WHERE tenant_id = ?1",
    ] {
        sqlx::query(statement)
            .bind(TEST_TENANT_ID)
            .execute(pool)
            .await
            .expect("clear legacy web grouping rows");
    }

    let fragment = crate::backend::domain::conversation_id_fragment(&question_id);
    let (kind, resolved_session_id) =
        super::super::conversation_repo::resolve_any_id_to_session_id_sqlx(
            pool,
            TEST_TENANT_ID,
            &fragment,
        )
        .await
        .expect("resolve web question fragment to its session");
    assert_eq!(kind, ConversationRecordKind::Web);
    assert_eq!(resolved_session_id, session_id);

    let locators = super::super::conversation_repo::list_conversation_block_locators_sqlx(
        pool,
        TEST_TENANT_ID,
        ConversationRecordKind::Web,
        &question_id,
    )
    .await
    .expect("list web block locators from turn columns");
    assert_eq!(locators.len(), 2);
    assert!(locators
        .iter()
        .all(|locator| locator.record_kind == "web" && locator.question_id == question_id));

    let question_locator = locators
        .iter()
        .find(|locator| locator.kind == "question")
        .expect("web question block locator");
    let block = super::super::conversation_repo::load_conversation_block_detail_sqlx(
        pool,
        TEST_TENANT_ID,
        ConversationRecordKind::Web,
        &question_locator.block_id,
    )
    .await
    .expect("load web question block from turn columns");
    assert_eq!(block.content, "How does sync work?");
    assert_eq!(block.locator.question_id, question_id);
    assert_eq!(block.locator.record_kind, "web");

    drop(database);
    cleanup_database(&db_path);
}

fn web_turn(
    template: &NormalizedConversationTurn,
    external_id: &str,
    turn_index: i64,
    user_text: &str,
) -> NormalizedConversationTurn {
    NormalizedConversationTurn {
        external_id: external_id.to_string(),
        turn_index,
        user_text: user_text.to_string(),
        ..template.clone()
    }
}

/// 同一会话的两条读法：web 轮次列与 web 成员表，逐轮对齐后返回列视图。
async fn load_web_turn_question_reads(
    pool: &SqlitePool,
) -> StoreResult<BTreeMap<String, (String, String)>> {
    let column_rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT external_id, question_id, question_origin FROM web_record_turns WHERE tenant_id = ?1",
    )
    .bind(TEST_TENANT_ID)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let membership_rows = sqlx::query_as::<_, (String, String, String)>(
        r#"
        SELECT t.external_id, qt.question_id, qt.assignment_origin
        FROM web_record_question_turns qt
        JOIN web_record_turns t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1
        "#,
    )
    .bind(TEST_TENANT_ID)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let to_map = |rows: Vec<(String, String, String)>| {
        rows.into_iter()
            .map(|(external_id, question_id, origin)| (external_id, (question_id, origin)))
            .collect::<BTreeMap<_, _>>()
    };
    let columns = to_map(column_rows);
    let membership = to_map(membership_rows);
    assert_eq!(
        columns, membership,
        "web record turn columns and membership disagree"
    );
    Ok(columns)
}

fn assert_web_turn_question_reads_agree(reads: &BTreeMap<String, (String, String)>) {
    assert!(!reads.is_empty(), "no web record turn question reads found");
    for (external_id, (question_id, _)) in reads {
        assert!(
            !question_id.is_empty(),
            "web record turn {external_id} has an empty question id"
        );
    }
}

async fn count_legacy_conversation_sessions_sqlx(
    pool: &SqlitePool,
    source_id: &str,
) -> StoreResult<i64> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversation_sessions WHERE source_id = ?1")
        .bind(source_id)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())
        .map_err(StoreError::External)
}

fn fixture_source() -> ConversationSource {
    let now = Utc::now().to_rfc3339();
    ConversationSource {
        id: "qwen-web-export".to_string(),
        adapter_id: "qwen-web".to_string(),
        name: "Qwen Web".to_string(),
        kind: ConversationSourceKind::Directory,
        location: "/tmp/qwen".to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn fixture_session() -> NormalizedConversationSession {
    NormalizedConversationSession {
        external_id: "web-session-1".to_string(),
        title: Some("Web session".to_string()),
        project_path: None,
        started_at: None,
        updated_at: None,
        source_locator: None,
        source_fingerprint: None,
        turns: vec![NormalizedConversationTurn {
            external_id: "turn-1".to_string(),
            turn_index: 0,
            user_text: "Hello from the web".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("Web answer".to_string()),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: None,
                content_card: None,
                metadata_json: content_card_metadata("answer"),
            }],
        }],
        ..Default::default()
    }
}

fn content_card_metadata(card_type: &str) -> Option<String> {
    Some(format!(
        r#"{{"content_card":{{"type":"{card_type}","format":"markdown"}}}}"#
    ))
}

#[tokio::test]
async fn sqlx_web_record_unchanged_session_keeps_part_index_rows() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-index-skip-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let source = fixture_source();
    let mut session = fixture_session();
    session.source_fingerprint = Some("same-source".to_string());

    let (skipped_session_count, indexed_texts) = async {
        super::super::conversation_repo::upsert_conversation_source_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
        )
        .await
        .map_err(StoreError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[session.clone()],
            false,
        )
        .await?;
        let result = import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[session],
            false,
        )
        .await?;
        let indexed_texts = sqlx::query_scalar::<_, String>(
            "SELECT text FROM conversation_part_fts WHERE tenant_id = ?1 ORDER BY text ASC",
        )
        .bind(TEST_TENANT_ID)
        .fetch_all(database.pool())
        .await
        .map_err(StoreError::external)?;
        StoreResult::Ok((result.skipped_session_count, indexed_texts))
    }
    .await
    .expect("re-import an unchanged web record");

    assert_eq!(skipped_session_count, 1);
    assert_eq!(
        indexed_texts,
        vec!["Web answer".to_string()],
        "跳过未变化的会话时不得丢失既有索引行"
    );

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_web_record_search_hits_tool_output_bodies() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-part-search-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let source = fixture_source();
    let mut session = fixture_session();
    session.turns[0].parts.push(NormalizedConversationPart {
        role: ConversationPartRole::Tool,
        kind: ConversationPartKind::Tool,
        text: Some("web runner crashed with SIGSEGV".to_string()),
        language: None,
        command: None,
        cwd: None,
        status: None,
        exit_code: None,
        command_label: None,
        source_execution_id: None,
        content_card: None,
        metadata_json: content_card_metadata("tool_result"),
    });

    let (tool_hits, answer_hits, prompt_hits, missing_hits) = async {
        super::super::conversation_repo::upsert_conversation_source_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
        )
        .await
        .map_err(StoreError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[session],
            false,
        )
        .await?;
        let mut results = Vec::new();
        for needle in [
            "SIGSEGV",
            "Web answer",
            "Hello from the web",
            "nothing-matches-this-needle",
        ] {
            results.push(
                list_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    Some(needle),
                    20,
                    0,
                )
                .await?,
            );
        }
        let mut results = results.into_iter();
        StoreResult::Ok((
            results.next().unwrap(),
            results.next().unwrap(),
            results.next().unwrap(),
            results.next().unwrap(),
        ))
    }
    .await
    .expect("search web records by part bodies through SQLx");

    assert_eq!(tool_hits.len(), 1, "web 记录表族同样命中工具输出正文");
    assert_eq!(answer_hits.len(), 1);
    assert_eq!(prompt_hits.len(), 1);
    assert!(missing_hits.is_empty());

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_web_record_search_degrades_to_title_while_part_index_rebuilds() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-web-record-part-search-degraded-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let source = fixture_source();

    let (ready_body_hits, rebuilding_body_hits, rebuilding_title_hits) = async {
        super::super::conversation_repo::upsert_conversation_source_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
        )
        .await
        .map_err(StoreError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[fixture_session()],
            false,
        )
        .await?;
        let ready_body_hits = list_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            Some("Web answer"),
            20,
            0,
        )
        .await?;
        super::super::conversation_repo::set_conversation_part_index_status_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            "building",
            0,
            None,
        )
        .await
        .map_err(StoreError::external)?;
        let rebuilding_body_hits = list_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            Some("Web answer"),
            20,
            0,
        )
        .await?;
        let rebuilding_title_hits = list_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            Some("Web session"),
            20,
            0,
        )
        .await?;
        StoreResult::Ok((ready_body_hits, rebuilding_body_hits, rebuilding_title_hits))
    }
    .await
    .expect("search web records while the part index rebuilds");

    assert_eq!(ready_body_hits.len(), 1);
    assert!(rebuilding_body_hits.is_empty());
    assert_eq!(rebuilding_title_hits.len(), 1);

    drop(database);
    cleanup_database(&db_path);
}

fn cleanup_database(db_path: &std::path::Path) {
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
}
