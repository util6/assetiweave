use super::*;
use crate::backend::dto::{ConversationRecordKind, ConversationSearchCardType};
use crate::backend::models::{
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
                .map_err(AppError::external)?;
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
        AppResult::Ok((
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
        .map_err(AppError::external)?;
        let columns = sqlx::query_scalar::<_, String>(
            "SELECT name FROM pragma_table_info('web_record_sessions')",
        )
        .fetch_all(database.pool())
        .await
        .map_err(AppError::external)?;
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
        AppResult::Ok((columns, sessions, detail))
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
            .map_err(AppError::external)?;
            super::super::conversation_repo::import_conversation_sessions_sqlx(
                database.pool(),
                tenant_id,
                &source,
                &[fixture_session()],
                false,
            )
            .await
            .map_err(AppError::external)?;
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
        .map_err(AppError::external)?;
        let beta_detail = super::super::conversation_repo::load_conversation_session_detail_sqlx(
            database.pool(),
            tenant_beta,
            &beta_sessions[0].session.id,
        )
        .await
        .map_err(AppError::external)?;
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
        .map_err(AppError::external)?;
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
        .map_err(AppError::external)?
        .len();

        AppResult::Ok((beta_before, beta_after, alpha_legacy_count))
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
        .map_err(AppError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[current_session.clone(), archived_session],
            false,
        )
        .await
        .map_err(AppError::external)?;
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
        AppResult::Ok((listed, retained_detail))
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

    let (result, imported_at, metadata_json, fts_row_count) = async {

                super::super::conversation_repo::upsert_conversation_source_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                )
                .await
                .map_err(AppError::external)?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[old_session],
                    false,
                )
                .await
                .map_err(AppError::external)?;
                let session_id = stable_id("web-record-session", &[&source.id, "web-session-1"]);
                sqlx::query(
                    r#"
                    INSERT INTO conversation_question_fts (
                        tenant_id, question_id, session_id, question_text, answer_text,
                        code_text, command_text
                    ) VALUES (?1, 'web-record-question-stale', ?2, '', 'stale', '', '')
                    "#,
                )
                .bind(TEST_TENANT_ID)
                .bind(&session_id)
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                sqlx::query(
                    "UPDATE web_record_sessions SET imported_at = 'preserved' WHERE source_id = ?1",
                )
                .bind(&source.id)
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
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
                .map_err(AppError::external)?;
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
                .map_err(AppError::external)?;
                let fts_row_count = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM conversation_question_fts WHERE tenant_id = ?1 AND session_id = ?2",
                )
                .bind(TEST_TENANT_ID)
                .bind(&session_id)
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)?;
                AppResult::Ok((result, imported_at, metadata_json, fts_row_count))

        }.await
            .expect("refresh normalized web parts through SQLx");

    assert_eq!(result.skipped_session_count, 0);
    assert_ne!(imported_at, "preserved");
    assert!(metadata_json
        .as_deref()
        .unwrap_or("")
        .contains(r#""content_card""#));
    assert_eq!(fts_row_count, 1);

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
        .map_err(AppError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[first, second],
            false,
        )
        .await
        .map_err(AppError::external)?;
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
        AppResult::Ok((sessions, detail))
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
        .map_err(AppError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[fixture_session()],
            false,
        )
        .await
        .map_err(AppError::external)?;
        let session_id = stable_id("web-record-session", &[&source.id, "web-session-1"]);
        let fragment = crate::backend::models::conversation_id_fragment(&session_id);
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
                crate::backend::dto::ConversationRecordKind::Web,
                None,
                Some(&source.id),
                &fragment,
                20,
                0,
            )
            .await
            .map_err(AppError::external)?;
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
        AppResult::Ok((fragment_matches, direct_fragment_matches, full_matches))
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
        .map_err(AppError::external)?;
        import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[session],
            false,
        )
        .await
        .map_err(AppError::external)?;
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
            .map_err(AppError::external)?;
        }
        import_web_record_sessions_sqlx(
            database.pool(),
            tenant_alpha,
            &source,
            &[alpha_session],
            false,
        )
        .await
        .map_err(AppError::external)?;
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
        .map_err(AppError::external)?;
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
        .map_err(AppError::external)?;
        AppResult::Ok((session_id, alpha_detail, beta_detail, alpha_page, beta_page))
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
        .map_err(AppError::external)?;
        import_web_record_sessions_sqlx(database.pool(), TEST_TENANT_ID, &source, &[first], false)
            .await
            .map_err(AppError::external)?;
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
        AppResult::Ok((part_id, detail))
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
        crate::backend::dto::ConversationCardRenderer::Markdown
    );

    drop(database);
    cleanup_database(&db_path);
}

async fn count_legacy_conversation_sessions_sqlx(
    pool: &SqlitePool,
    source_id: &str,
) -> AppResult<i64> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversation_sessions WHERE source_id = ?1")
        .bind(source_id)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())
        .map_err(AppError::External)
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

fn cleanup_database(db_path: &std::path::Path) {
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
}
