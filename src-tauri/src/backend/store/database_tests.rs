use super::*;
use rusqlite::Connection;
use sha2::{Digest, Sha384};
use uuid::Uuid;

#[test]
fn released_memory_domain_migration_remains_immutable() {
    let migration = include_str!("../../../migrations/202607230001_memory_domain.sql");
    assert_eq!(
            format!("{:x}", Sha384::digest(migration.as_bytes())),
            "bdc84ed0a2c958b8bcd86ed8ffe364b9cf5954002531ac9e015503bab702ba2850586cacbdfcd291fcdc97b41bc8db8b"
        );
}

#[test]
fn released_conversation_question_contract_migration_remains_immutable() {
    let migration =
        include_str!("../../../migrations/202608250005_rebuild_conversation_questions.sql");
    assert_eq!(
            format!("{:x}", Sha384::digest(migration.as_bytes())),
            "85bfcaa1edbb892e90fb943086a77885e6a6c7d349106049ca71a43f9655a60682b18a9d69cce576eea3ccdcee1e0cf2"
        );
}

#[test]
fn released_session_memory_durable_jobs_migration_remains_immutable() {
    let migration =
        include_str!("../../../migrations/202608310004_session_memory_durable_jobs.sql");
    assert_eq!(
            format!("{:x}", Sha384::digest(migration.as_bytes())),
            "d786fd5e32cc28a7864c23b44c332452a50d78c7fd78ae4cc1958105ac710b0b5a7bbbd5db6a57f69f0510ceaeaf91d4"
        );
}

#[tokio::test]
async fn migrations_retain_recall_indexes_after_question_contract_rebuild() {
    let db_path = temp_database_path("memory-recall-index-upgrade");
    migrate_database(&db_path)
        .await
        .expect("create current database");

    let conn = Connection::open(&db_path).expect("open migrated database");
    let index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN ('idx_memory_recall_questions_created', 'idx_memory_recall_web_questions_created')",
                [],
                |row| row.get(0),
            )
            .expect("query Recall indexes");
    let memory_checksum: String = conn
        .query_row(
            "SELECT lower(hex(checksum)) FROM _sqlx_migrations WHERE version = 202607230001",
            [],
            |row| row.get(0),
        )
        .expect("query Memory migration checksum");
    assert_eq!(index_count, 2);
    assert_eq!(
            memory_checksum,
            "bdc84ed0a2c958b8bcd86ed8ffe364b9cf5954002531ac9e015503bab702ba2850586cacbdfcd291fcdc97b41bc8db8b"
        );
    cleanup_database(&db_path);
}

#[tokio::test]
async fn migrations_create_fresh_schema_and_track_version() {
    let db_path = temp_database_path("fresh");

    migrate_database(&db_path).await.expect("run migrations");

    let conn = Connection::open(&db_path).expect("open migrated database");
    let source_table_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'sources'",
            [],
            |row| row.get(0),
        )
        .expect("query sources table");
    let memory_table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('memory_recall_sessions', 'memory_recall_turns')",
                [],
                |row| row.get(0),
            )
            .expect("query memory tables");
    let migration_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM _sqlx_migrations", [], |row| {
            row.get(0)
        })
        .expect("query migrations");
    let memory_recall_index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN ('idx_memory_recall_questions_created', 'idx_memory_recall_web_questions_created')",
                [],
                |row| row.get(0),
            )
            .expect("query Memory Recall indexes");
    let execution_projection_index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN ('idx_conversation_parts_execution', 'idx_web_record_parts_execution')",
                [],
                |row| row.get(0),
            )
            .expect("query obsolete Execution projection indexes");

    assert_eq!(source_table_count, 1);
    assert_eq!(memory_table_count, 2);
    assert_eq!(memory_recall_index_count, 2);
    assert_eq!(execution_projection_index_count, 0);
    assert_eq!(migration_count, MIGRATOR.migrations.len() as i64);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn conversation_question_contract_contains_only_stable_metadata() {
    let db_path = temp_database_path("conversation-question-contract");
    migrate_database(&db_path).await.expect("run migrations");

    let conn = Connection::open(&db_path).expect("open migrated database");
    for table in ["conversation_questions", "web_record_questions"] {
        let columns = conn
            .prepare(&format!(
                "SELECT name FROM pragma_table_info('{table}') ORDER BY cid"
            ))
            .expect("prepare question columns")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query question columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect question columns");
        assert_eq!(
            columns,
            vec![
                "tenant_id",
                "id",
                "session_id",
                "title",
                "created_at",
                "updated_at"
            ]
        );
    }
    let audit_table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'conversation_data_audit_issues'",
                [],
                |row| row.get(0),
            )
            .expect("query conversation audit table");
    assert_eq!(audit_table_count, 1);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn migrations_rebuild_representative_legacy_question_rows_and_audit_snapshots() {
    let db_path = temp_database_path("conversation-question-legacy-rebuild");
    migrate_database(&db_path)
        .await
        .expect("create current database");

    let conn = Connection::open(&db_path).expect("open current database");
    conn.execute_batch(
        r#"
            PRAGMA foreign_keys = OFF;
            INSERT INTO conversation_sessions (
                tenant_id, id, source_id, adapter_id, external_id, title, project_path,
                started_at, updated_at, source_locator, source_fingerprint, missing,
                created_at, imported_at
            ) VALUES (
                'default', 'legacy-session', 'legacy-source', 'legacy-adapter',
                'legacy-external', 'Legacy session', NULL, NULL, NULL, NULL, NULL, 0,
                '2026-08-25T00:00:00Z', '2026-08-25T00:00:00Z'
            );
            INSERT INTO web_record_sessions (
                tenant_id, id, source_id, adapter_id, external_id, title,
                started_at, updated_at, source_locator, source_fingerprint, missing,
                created_at, imported_at
            ) VALUES (
                'default', 'legacy-web-session', 'legacy-source', 'legacy-adapter',
                'legacy-web-external', 'Legacy web session', NULL, NULL, NULL, NULL, 0,
                '2026-08-25T00:00:00Z', '2026-08-25T00:00:00Z'
            );
            DROP TABLE conversation_data_audit_issues;
            DROP TABLE conversation_questions;
            DROP TABLE web_record_questions;
            CREATE TABLE conversation_questions (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                question_index INTEGER NOT NULL,
                title TEXT,
                question_text TEXT NOT NULL,
                answer_text TEXT NOT NULL,
                code_text TEXT NOT NULL,
                command_text TEXT NOT NULL,
                grouping_origin TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (tenant_id, id),
                UNIQUE (tenant_id, session_id, question_index)
            );
            CREATE TABLE web_record_questions (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                question_index INTEGER NOT NULL,
                title TEXT,
                question_text TEXT NOT NULL,
                answer_text TEXT NOT NULL,
                code_text TEXT NOT NULL,
                command_text TEXT NOT NULL,
                grouping_origin TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (tenant_id, id),
                UNIQUE (tenant_id, session_id, question_index)
            );
            INSERT INTO conversation_questions (
                tenant_id, id, session_id, question_index, title, question_text,
                answer_text, code_text, command_text, grouping_origin, created_at, updated_at
            ) VALUES (
                'default', 'legacy-question', 'legacy-session', 7, NULL,
                'Legacy question snapshot', 'Legacy answer', 'Legacy code', 'Legacy command',
                'auto_merged', '2026-08-25T00:00:00Z', '2026-08-25T00:00:00Z'
            );
            INSERT INTO web_record_questions (
                tenant_id, id, session_id, question_index, title, question_text,
                answer_text, code_text, command_text, grouping_origin, created_at, updated_at
            ) VALUES (
                'default', 'legacy-web-question', 'legacy-web-session', 3, NULL,
                'Legacy web question snapshot', 'Legacy web answer', 'Legacy web code',
                'Legacy web command', 'imported', '2026-08-25T00:00:00Z',
                '2026-08-25T00:00:00Z'
            );
            INSERT INTO conversation_turns (
                tenant_id, id, session_id, external_id, turn_index, user_text,
                title, started_at, ended_at, fingerprint, missing, imported_at
            ) VALUES (
                'default', 'legacy-turn', 'legacy-session', 'legacy-turn-external', 0,
                'Legacy question snapshot', NULL, NULL, NULL, 'legacy-turn-fingerprint', 0,
                '2026-08-25T00:00:00Z'
            );
            INSERT INTO conversation_question_turns (
                tenant_id, question_id, turn_id, turn_order, assignment_origin,
                assigned_at, updated_at
            ) VALUES (
                'default', 'legacy-question', 'legacy-turn', 0, 'auto_merged',
                '2026-08-25T00:00:00Z', '2026-08-25T00:00:00Z'
            );
            INSERT INTO web_record_turns (
                tenant_id, id, session_id, external_id, turn_index, user_text,
                title, started_at, ended_at, fingerprint, missing, imported_at
            ) VALUES (
                'default', 'legacy-web-turn', 'legacy-web-session',
                'legacy-web-turn-external', 0, 'Legacy web question snapshot', NULL,
                NULL, NULL, 'legacy-web-turn-fingerprint', 0,
                '2026-08-25T00:00:00Z'
            );
            INSERT INTO web_record_question_turns (
                tenant_id, question_id, turn_id, turn_order, assignment_origin,
                assigned_at, updated_at
            ) VALUES (
                'default', 'legacy-web-question', 'legacy-web-turn', 0, 'imported',
                '2026-08-25T00:00:00Z', '2026-08-25T00:00:00Z'
            );
            DELETE FROM _sqlx_migrations
            WHERE version IN (202608250004, 202608250005, 202608260001);
            "#,
    )
    .expect("seed representative legacy question schema");
    drop(conn);

    migrate_database(&db_path)
        .await
        .expect("rebuild legacy question schema");

    let conn = Connection::open(&db_path).expect("open rebuilt database");
    for table in ["conversation_questions", "web_record_questions"] {
        let columns = conn
            .prepare(&format!(
                "SELECT name FROM pragma_table_info('{table}') ORDER BY cid"
            ))
            .expect("prepare rebuilt question columns")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query rebuilt question columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect rebuilt question columns");
        assert_eq!(
            columns,
            vec![
                "tenant_id",
                "id",
                "session_id",
                "title",
                "created_at",
                "updated_at"
            ]
        );
    }
    let question_title: String = conn
        .query_row(
            "SELECT title FROM conversation_questions WHERE id = 'legacy-question'",
            [],
            |row| row.get(0),
        )
        .expect("query rebuilt conversation question");
    let web_question_title: String = conn
        .query_row(
            "SELECT title FROM web_record_questions WHERE id = 'legacy-web-question'",
            [],
            |row| row.get(0),
        )
        .expect("query rebuilt web question");
    let snapshot_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM conversation_data_audit_issues WHERE category = 'question_snapshot_dependencies' AND status = 'open'",
                [],
                |row| row.get(0),
            )
            .expect("query question snapshot audit");
    let snapshot_affected_count: i64 = conn
            .query_row(
                "SELECT affected_count FROM conversation_data_audit_issues WHERE category = 'question_snapshot_dependencies' AND status = 'open'",
                [],
                |row| row.get(0),
            )
            .expect("query question snapshot audit count");
    assert_eq!(question_title, "Legacy question snapshot");
    assert_eq!(web_question_title, "Legacy web question snapshot");
    assert_eq!(snapshot_count, 1);
    assert_eq!(snapshot_affected_count, 2);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn migrations_adopt_legacy_schema_without_losing_rows() {
    let db_path = temp_database_path("legacy");
    let conn = Connection::open(&db_path).expect("open legacy database");
    conn.execute_batch(
            r#"
            CREATE TABLE sources (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                root_path TEXT NOT NULL,
                include_globs TEXT NOT NULL,
                exclude_globs TEXT NOT NULL,
                default_kind TEXT,
                enabled INTEGER NOT NULL,
                priority INTEGER NOT NULL,
                last_scanned_at TEXT,
                last_scan_status TEXT
            );
            INSERT INTO sources (
                id, name, kind, root_path, include_globs, exclude_globs,
                default_kind, enabled, priority
            ) VALUES (
                'legacy-source', 'Legacy', 'local', '/tmp/legacy', '[]', '[]',
                NULL, 1, 10
            );
            CREATE TABLE profiles (
                id TEXT PRIMARY KEY,
                payload TEXT NOT NULL
            );
            INSERT INTO profiles (id, payload) VALUES (
                'cursor',
                '{"id":"cursor","name":"Cursor","app_kind":"cursor","target_paths":["~/Library/Application Support/Cursor/skills"],"supported_kinds":["skill"],"deployment_strategy":"symlink_to_source","enabled":true,"include":{"kinds":["skill"],"tags":[],"groups":[],"sources":[],"path_patterns":[]},"exclude":{"kinds":[],"tags":[],"groups":[],"sources":[],"path_patterns":[]},"safety":{"allow_remove":false,"allow_overwrite":false}}'
            );
            "#,
        )
        .expect("create legacy schema");
    drop(conn);

    migrate_database(&db_path)
        .await
        .expect("adopt legacy database");

    let conn = Connection::open(&db_path).expect("open migrated database");
    let source: (String, String, String) = conn
        .query_row(
            "SELECT id, scanner_kind, source_origin FROM sources WHERE id = 'legacy-source'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("query preserved source");
    let migration_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM _sqlx_migrations", [], |row| {
            row.get(0)
        })
        .expect("query migrations");
    let cursor_target_path: String = conn
        .query_row(
            "SELECT json_extract(payload, '$.target_paths[0]') FROM profiles WHERE id = 'cursor'",
            [],
            |row| row.get(0),
        )
        .expect("query migrated cursor target path");

    assert_eq!(
        source,
        (
            "legacy-source".to_string(),
            "mixed".to_string(),
            "local_folder".to_string()
        )
    );
    assert_eq!(cursor_target_path, "@config/Cursor/skills");
    assert_eq!(migration_count, MIGRATOR.migrations.len() as i64);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn migrations_accept_the_original_catalog_release_details_checksum() {
    let db_path = temp_database_path("catalog-release-checksum");
    migrate_database(&db_path)
        .await
        .expect("create current database");

    let conn = Connection::open(&db_path).expect("open migrated database");
    conn.execute_batch(
            r#"
            ALTER TABLE conversation_adapter_catalog_releases DROP COLUMN source_json;
            UPDATE _sqlx_migrations
            SET checksum = X'E75F75A803B17920C2E1498962D6DCB8A34167DE66AAB216FBC2F4CA056A383E0600CAEFE44633CDA48A0F9FE61DC581'
            WHERE version = 202607150002;
            DELETE FROM _sqlx_migrations WHERE version = 202607160001;
            "#,
        )
        .expect("simulate database created by the original migration");
    drop(conn);

    migrate_database(&db_path)
        .await
        .expect("upgrade database without modifying applied migrations");

    let conn = Connection::open(&db_path).expect("open upgraded database");
    let source_json_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('conversation_adapter_catalog_releases') WHERE name = 'source_json'",
                [],
                |row| row.get(0),
            )
            .expect("query source_json column");
    assert_eq!(source_json_count, 1);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn migrations_repair_the_known_modified_catalog_release_details_checksum() {
    let db_path = temp_database_path("catalog-release-modified-checksum");
    migrate_database(&db_path)
        .await
        .expect("create current database");

    let conn = Connection::open(&db_path).expect("open migrated database");
    conn.execute_batch(
            r#"
            UPDATE _sqlx_migrations
            SET checksum = X'863286E8B8E292E94EB63E42AC751D8EAD340500965B16ADCB4EEF61BEB38187B9E8FF4F19379B7C817F18DDD3BF0A83'
            WHERE version = 202607150002;
            DELETE FROM _sqlx_migrations WHERE version = 202607160001;
            "#,
        )
        .expect("simulate database created by the modified migration");
    drop(conn);

    migrate_database(&db_path)
        .await
        .expect("repair known migration checksum and continue");

    let conn = Connection::open(&db_path).expect("open repaired database");
    let migration_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM _sqlx_migrations", [], |row| {
            row.get(0)
        })
        .expect("query migrations");
    assert_eq!(migration_count, MIGRATOR.migrations.len() as i64);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn migrations_repair_the_known_modified_question_contract_checksum() {
    let db_path = temp_database_path("question-contract-modified-checksum");
    migrate_database(&db_path)
        .await
        .expect("create current database");

    let conn = Connection::open(&db_path).expect("open migrated database");
    conn.execute_batch(
            r#"
            DROP INDEX idx_memory_recall_questions_created;
            DROP INDEX idx_memory_recall_web_questions_created;
            UPDATE _sqlx_migrations
            SET checksum = X'E9A07424F1C86E99CED78966A8CB750B73D6C00E089576DD5C077B357B259BE2C34C14F0BEC320A2FB991B98B6BB9D38'
            WHERE version = 202608250005;
            DELETE FROM _sqlx_migrations WHERE version = 202608260001;
            "#,
        )
        .expect("simulate database opened after the released migration was modified");
    drop(conn);

    migrate_database(&db_path)
        .await
        .expect("repair known checksum and apply follow-up migration");

    let conn = Connection::open(&db_path).expect("open repaired database");
    let question_contract_checksum: String = conn
        .query_row(
            "SELECT lower(hex(checksum)) FROM _sqlx_migrations WHERE version = 202608250005",
            [],
            |row| row.get(0),
        )
        .expect("query repaired question contract checksum");
    let repair_migration_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 202608260001",
            [],
            |row| row.get(0),
        )
        .expect("query follow-up migration");
    let recall_index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN ('idx_memory_recall_questions_created', 'idx_memory_recall_web_questions_created')",
                [],
                |row| row.get(0),
            )
            .expect("query restored Recall indexes");

    assert_eq!(
            question_contract_checksum,
            "85bfcaa1edbb892e90fb943086a77885e6a6c7d349106049ca71a43f9655a60682b18a9d69cce576eea3ccdcee1e0cf2"
        );
    assert_eq!(repair_migration_count, 1);
    assert_eq!(recall_index_count, 2);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn database_reuses_pool_for_queries_after_migration() {
    let db_path = temp_database_path("pool");
    let database = Database::open_async(&db_path).await.expect("open database");

    let source_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sources")
        .fetch_one(database.pool())
        .await
        .expect("query via SQLx pool");

    assert_eq!(source_count, 0);
    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn initialized_database_seeds_defaults_without_reseeding() {
    let db_path = temp_database_path("initialized");
    let database = Database::open_initialized_async(&db_path)
        .await
        .expect("open initialized database");

    let source_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sources")
        .fetch_one(database.pool())
        .await
        .map_err(AppError::external)
        .expect("query sources");
    let profile_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM profiles")
        .fetch_one(database.pool())
        .await
        .map_err(AppError::external)
        .expect("query profiles");
    let navigation_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM navigation_state")
        .fetch_one(database.pool())
        .await
        .map_err(AppError::external)
        .expect("query navigation_state");
    let shortcut_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM app_shortcut_items")
        .fetch_one(database.pool())
        .await
        .map_err(AppError::external)
        .expect("query app_shortcut_items");

    assert!(source_count > 0);
    assert!(profile_count > 0);
    assert_eq!(navigation_count, 1);
    assert!(shortcut_count > 0);
    drop(database);

    let conn = Connection::open(&db_path).expect("open seeded database");
    conn.execute(
        "UPDATE conversation_adapters SET name = 'preserved' WHERE id = 'codex'",
        [],
    )
    .expect("customize seeded adapter");
    drop(conn);

    let reopened = Database::open_initialized_async(&db_path)
        .await
        .expect("reopen initialized database");
    let codex_name = sqlx::query_scalar::<_, String>(
        "SELECT name FROM conversation_adapters WHERE id = 'codex'",
    )
    .fetch_one(reopened.pool())
    .await
    .map_err(AppError::external)
    .expect("query preserved adapter");

    assert_eq!(codex_name, "preserved");
    drop(reopened);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn initialized_database_restores_missing_builtin_app_icon_rows() {
    let db_path = temp_database_path("app-icon-defaults");
    let database = Database::open_initialized_async(&db_path)
        .await
        .expect("open initialized database");
    drop(database);

    let conn = Connection::open(&db_path).expect("open seeded database");
    conn.execute_batch(
            "UPDATE app_shortcut_items SET accent_color = '#123456' WHERE profile_id = 'codex';
             DELETE FROM app_shortcut_items WHERE profile_id IN ('kiro', 'zcode', 'qoder', 'hermes');
             DELETE FROM profiles WHERE id IN ('kiro', 'zcode', 'qoder', 'hermes');",
        )
        .expect("remove newly introduced app defaults");
    drop(conn);

    let reopened = Database::open_async(&db_path)
        .await
        .expect("reopen initialized database");
    seed_tenant_defaults_sqlx(reopened.pool(), "default")
        .await
        .expect("seed tenant defaults");
    let profile_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM profiles")
        .fetch_one(reopened.pool())
        .await
        .map_err(AppError::external)
        .expect("query profiles");
    let shortcut_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM app_shortcut_items")
        .fetch_one(reopened.pool())
        .await
        .map_err(AppError::external)
        .expect("query shortcuts");
    let codex_accent = sqlx::query_scalar::<_, String>(
        "SELECT accent_color FROM app_shortcut_items WHERE profile_id = 'codex'",
    )
    .fetch_one(reopened.pool())
    .await
    .map_err(AppError::external)
    .expect("query codex accent");
    let hermes_accent = sqlx::query_scalar::<_, String>(
        "SELECT accent_color FROM app_shortcut_items WHERE profile_id = 'hermes'",
    )
    .fetch_one(reopened.pool())
    .await
    .map_err(AppError::external)
    .expect("query hermes accent");

    let expected_profile_count = crate::backend::defaults::default_profiles_from_catalog(
        &crate::backend::target_catalog::TargetCatalog::builtin_for_tests()
            .expect("builtin target descriptors"),
    )
    .len() as i64;
    assert_eq!(profile_count, expected_profile_count);
    assert_eq!(
        shortcut_count,
        crate::backend::defaults::default_app_shortcuts().len() as i64
    );
    assert_eq!(codex_accent, "#123456");
    assert_eq!(hermes_accent, "#f97316");
    drop(reopened);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn initialized_database_seeds_local_principal_and_default_tenant() {
    let db_path = temp_database_path("tenant-identity");
    let database = Database::open_initialized_async(&db_path)
        .await
        .expect("open initialized database");

    let principal_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM principals WHERE id = 'local'")
            .fetch_one(database.pool())
            .await
            .map_err(AppError::external)
            .expect("query principal count");
    let tenant_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tenants WHERE id = 'default'")
            .fetch_one(database.pool())
            .await
            .map_err(AppError::external)
            .expect("query tenant count");
    let membership_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM tenant_memberships WHERE principal_id = 'local' AND tenant_id = 'default' AND role = 'owner'",
        )
        .fetch_one(database.pool())
        .await
        .map_err(AppError::external)
        .expect("query membership count");
    let active_tenant_id = sqlx::query_scalar::<_, String>(
        "SELECT active_tenant_id FROM tenant_state WHERE principal_id = 'local'",
    )
    .fetch_one(database.pool())
    .await
    .map_err(AppError::external)
    .expect("query active tenant");

    assert_eq!(principal_count, 1);
    assert_eq!(tenant_count, 1);
    assert_eq!(membership_count, 1);
    assert_eq!(active_tenant_id, "default");
    drop(database);
    cleanup_database(&db_path);
}

fn temp_database_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "assetiweave-sqlx-{label}-{}.sqlite",
        Uuid::new_v4()
    ))
}

fn cleanup_database(db_path: &std::path::Path) {
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}
