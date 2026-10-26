use super::*;
use rusqlite::Connection;
use sha2::{Digest, Sha384};
use uuid::Uuid;

#[test]
fn released_memory_domain_migration_remains_immutable() {
    let migration = include_str!("../../migrations/202607230001_memory_domain.sql");
    assert_eq!(
            format!("{:x}", Sha384::digest(migration.as_bytes())),
            "bdc84ed0a2c958b8bcd86ed8ffe364b9cf5954002531ac9e015503bab702ba2850586cacbdfcd291fcdc97b41bc8db8b"
        );
}

#[test]
fn released_conversation_question_contract_migration_remains_immutable() {
    let migration =
        include_str!("../../migrations/202608250005_rebuild_conversation_questions.sql");
    assert_eq!(
            format!("{:x}", Sha384::digest(migration.as_bytes())),
            "85bfcaa1edbb892e90fb943086a77885e6a6c7d349106049ca71a43f9655a60682b18a9d69cce576eea3ccdcee1e0cf2"
        );
}

#[test]
fn released_session_memory_durable_jobs_migration_remains_immutable() {
    let migration = include_str!("../../migrations/202608310004_session_memory_durable_jobs.sql");
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
    assert_eq!(
        index_count, 0,
        "legacy recall indexes dropped with question tables"
    );
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
    assert_eq!(
        memory_recall_index_count, 0,
        "legacy recall indexes dropped with question tables"
    );
    assert_eq!(execution_projection_index_count, 0);
    assert_eq!(migration_count, MIGRATOR.migrations.len() as i64);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn migrations_drop_conversation_part_link_edge_tables() {
    let db_path = temp_database_path("conversation-part-link-drop");
    migrate_database(&db_path).await.expect("run migrations");

    let conn = Connection::open(&db_path).expect("open migrated database");
    let edge_table_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('conversation_part_links', 'web_record_part_links')",
            [],
            |row| row.get(0),
        )
        .expect("query part link tables");
    let edge_index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND (name IN ('idx_conversation_part_links_target', 'idx_web_record_part_links_target') OR tbl_name IN ('conversation_part_links', 'web_record_part_links'))",
            [],
            |row| row.get(0),
        )
        .expect("query part link indexes");

    assert_eq!(edge_table_count, 0);
    assert_eq!(edge_index_count, 0);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn conversation_question_contract_contains_only_stable_metadata() {
    let db_path = temp_database_path("conversation-question-contract");
    migrate_database(&db_path).await.expect("run migrations");

    let conn = Connection::open(&db_path).expect("open migrated database");
    for table in [
        "conversation_questions",
        "conversation_question_turns",
        "conversation_question_turn_audits",
        "web_record_questions",
        "web_record_question_turns",
    ] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("query dropped table");
        assert_eq!(count, 0, "{table} must not exist in converged schema");
    }
    for table in ["conversation_turns", "web_record_turns"] {
        let columns = conn
            .prepare(&format!(
                "SELECT name FROM pragma_table_info('{table}') ORDER BY cid"
            ))
            .expect("prepare turn columns")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query turn columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect turn columns");
        assert!(columns.contains(&"question_id".to_string()));
        assert!(columns.contains(&"question_origin".to_string()));
    }
    let index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN (
                'idx_conversation_turns_tenant_session_question',
                'idx_conversation_turns_tenant_question',
                'idx_web_record_turns_tenant_session_question'
            )",
            [],
            |row| row.get(0),
        )
        .expect("query question indexes");
    assert_eq!(index_count, 3);
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
            CREATE TABLE IF NOT EXISTS conversation_question_turns (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                question_id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                turn_order INTEGER NOT NULL,
                assignment_origin TEXT NOT NULL DEFAULT 'imported',
                assigned_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (tenant_id, question_id, turn_id)
            );
            CREATE TABLE IF NOT EXISTS web_record_question_turns (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                question_id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                turn_order INTEGER NOT NULL,
                assignment_origin TEXT NOT NULL DEFAULT 'imported',
                assigned_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (tenant_id, question_id, turn_id)
            );
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
            DROP TABLE IF EXISTS conversation_data_audit_issues;
            DROP TABLE IF EXISTS conversation_questions;
            DROP TABLE IF EXISTS web_record_questions;
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
            ALTER TABLE conversation_turns ADD COLUMN user_text TEXT DEFAULT '';
            ALTER TABLE web_record_turns ADD COLUMN user_text TEXT DEFAULT '';
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
                tenant_id, id, session_id, external_id, turn_index, prompt, user_text,
                title, started_at, ended_at, fingerprint, missing, imported_at
            ) VALUES (
                'default', 'legacy-turn', 'legacy-session', 'legacy-turn-external', 0,
                'Legacy question snapshot', 'Legacy question snapshot', NULL, NULL, NULL, 'legacy-turn-fingerprint', 0,
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
                tenant_id, id, session_id, external_id, turn_index, prompt, user_text,
                title, started_at, ended_at, fingerprint, missing, imported_at
            ) VALUES (
                'default', 'legacy-web-turn', 'legacy-web-session',
                'legacy-web-turn-external', 0, 'Legacy web question snapshot', 'Legacy web question snapshot', NULL,
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
            DROP INDEX IF EXISTS idx_memory_recall_questions_created;
            DROP INDEX IF EXISTS idx_memory_recall_web_questions_created;
            ALTER TABLE conversation_turns ADD COLUMN user_text TEXT DEFAULT '';
            ALTER TABLE web_record_turns ADD COLUMN user_text TEXT DEFAULT '';
            CREATE TABLE IF NOT EXISTS conversation_questions (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                id TEXT NOT NULL PRIMARY KEY,
                session_id TEXT NOT NULL,
                title TEXT,
                created_at TEXT NOT NULL DEFAULT '2026-08-25T00:00:00Z',
                updated_at TEXT NOT NULL DEFAULT '2026-08-25T00:00:00Z'
            );
            CREATE TABLE IF NOT EXISTS web_record_questions (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                id TEXT NOT NULL PRIMARY KEY,
                session_id TEXT NOT NULL,
                title TEXT,
                created_at TEXT NOT NULL DEFAULT '2026-08-25T00:00:00Z',
                updated_at TEXT NOT NULL DEFAULT '2026-08-25T00:00:00Z'
            );
            CREATE TABLE IF NOT EXISTS conversation_question_turns (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                question_id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                turn_order INTEGER NOT NULL,
                assignment_origin TEXT NOT NULL DEFAULT 'imported',
                assigned_at TEXT NOT NULL DEFAULT '2026-08-25T00:00:00Z',
                updated_at TEXT NOT NULL DEFAULT '2026-08-25T00:00:00Z',
                PRIMARY KEY (tenant_id, question_id, turn_id)
            );
            CREATE TABLE IF NOT EXISTS web_record_question_turns (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                question_id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                turn_order INTEGER NOT NULL,
                assignment_origin TEXT NOT NULL DEFAULT 'imported',
                assigned_at TEXT NOT NULL DEFAULT '2026-08-25T00:00:00Z',
                updated_at TEXT NOT NULL DEFAULT '2026-08-25T00:00:00Z',
                PRIMARY KEY (tenant_id, question_id, turn_id)
            );
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
async fn conversation_session_observations_keeps_only_incremental_sync_columns() {
    let db_path = temp_database_path("conversation-observation-slim");
    migrate_database(&db_path).await.expect("run migrations");

    let conn = Connection::open(&db_path).expect("open migrated database");
    assert_eq!(
        conversation_session_observation_columns(&conn),
        expected_conversation_session_observation_columns()
    );
    let dirty_index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_conversation_session_observations_dirty'",
            [],
            |row| row.get(0),
        )
        .expect("query observation dirty index");
    assert_eq!(dirty_index_count, 1);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn migrations_rebuild_legacy_observation_table_preserving_incremental_state() {
    let db_path = temp_database_path("conversation-observation-legacy-rebuild");
    migrate_database(&db_path)
        .await
        .expect("create current database");

    let conn = Connection::open(&db_path).expect("open current database");
    conn.execute_batch(
        r#"
            DROP TABLE conversation_session_observations;
            CREATE TABLE conversation_session_observations (
                tenant_id TEXT NOT NULL,
                source_id TEXT NOT NULL,
                record_kind TEXT NOT NULL CHECK (record_kind IN ('session', 'web')),
                external_id TEXT NOT NULL,
                observed_version TEXT NOT NULL,
                hydrated_version TEXT,
                last_seen_at TEXT NOT NULL,
                source_presence TEXT NOT NULL CHECK (source_presence IN ('present', 'absent', 'unknown')),
                dirty INTEGER NOT NULL DEFAULT 0,
                hydrated_adapter_hash TEXT,
                hydrated_card_contract_version INTEGER,
                hydrated_payload_policy_version INTEGER,
                error_code TEXT,
                error_message TEXT,
                error_stage TEXT,
                retryable INTEGER,
                attempt_count INTEGER NOT NULL DEFAULT 0,
                last_attempt_at TEXT,
                last_failure_at TEXT,
                hydrated_projection_version INTEGER,
                PRIMARY KEY (tenant_id, source_id, record_kind, external_id)
            );
            CREATE INDEX idx_conversation_session_observations_dirty
            ON conversation_session_observations(tenant_id, source_id, record_kind, dirty);
            INSERT INTO conversation_session_observations (
                tenant_id, source_id, record_kind, external_id, observed_version,
                hydrated_version, last_seen_at, source_presence, dirty,
                hydrated_adapter_hash, hydrated_card_contract_version,
                hydrated_payload_policy_version, error_code, error_message,
                error_stage, retryable, attempt_count, last_attempt_at,
                last_failure_at, hydrated_projection_version
            ) VALUES (
                'default', 'legacy-source', 'session', 'legacy-external', 'observed-v2',
                'hydrated-v1', '2026-10-01T00:00:00Z', 'absent', 1,
                'adapter-hash-v1', 7, 3, 'adapter_process_error', 'boom',
                'adapter', 1, 4, '2026-10-01T00:00:00Z',
                '2026-10-01T00:00:00Z', 5
            );
            DELETE FROM _sqlx_migrations WHERE version = 202610230002;
            "#,
    )
    .expect("seed legacy observation schema");
    drop(conn);

    migrate_database(&db_path)
        .await
        .expect("slim legacy observation schema");

    let conn = Connection::open(&db_path).expect("open rebuilt database");
    assert_eq!(
        conversation_session_observation_columns(&conn),
        expected_conversation_session_observation_columns()
    );
    let (
        observed_version,
        hydrated_version,
        last_seen_at,
        dirty,
        hydrated_adapter_hash,
        hydrated_card_contract_version,
        hydrated_payload_policy_version,
        hydrated_projection_version,
    ): (
        String,
        Option<String>,
        String,
        i64,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    ) = conn
        .query_row(
            r#"
            SELECT observed_version, hydrated_version, last_seen_at, dirty,
                   hydrated_adapter_hash, hydrated_card_contract_version,
                   hydrated_payload_policy_version, hydrated_projection_version
            FROM conversation_session_observations
            WHERE tenant_id = 'default' AND source_id = 'legacy-source'
              AND record_kind = 'session' AND external_id = 'legacy-external'
            "#,
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .expect("query rebuilt observation row");
    assert_eq!(observed_version, "observed-v2");
    assert_eq!(hydrated_version.as_deref(), Some("hydrated-v1"));
    assert_eq!(last_seen_at, "2026-10-01T00:00:00Z");
    assert_eq!(dirty, 1);
    assert_eq!(hydrated_adapter_hash.as_deref(), Some("adapter-hash-v1"));
    assert_eq!(hydrated_card_contract_version, Some(7));
    assert_eq!(hydrated_payload_policy_version, Some(3));
    assert_eq!(hydrated_projection_version, Some(5));
    cleanup_database(&db_path);
}

fn expected_conversation_session_observation_columns() -> Vec<&'static str> {
    vec![
        "tenant_id",
        "source_id",
        "record_kind",
        "external_id",
        "observed_version",
        "hydrated_version",
        "last_seen_at",
        "dirty",
        "hydrated_adapter_hash",
        "hydrated_card_contract_version",
        "hydrated_payload_policy_version",
        "hydrated_projection_version",
    ]
}

fn conversation_session_observation_columns(conn: &Connection) -> Vec<String> {
    conn.prepare(
        "SELECT name FROM pragma_table_info('conversation_session_observations') ORDER BY cid",
    )
    .expect("prepare observation columns")
    .query_map([], |row| row.get::<_, String>(0))
    .expect("query observation columns")
    .collect::<Result<Vec<_>, _>>()
    .expect("collect observation columns")
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
        .map_err(StoreError::external)
        .expect("query sources");
    let profile_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM profiles")
        .fetch_one(database.pool())
        .await
        .map_err(StoreError::external)
        .expect("query profiles");
    let navigation_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM navigation_state")
        .fetch_one(database.pool())
        .await
        .map_err(StoreError::external)
        .expect("query navigation_state");
    let shortcut_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM app_shortcut_items")
        .fetch_one(database.pool())
        .await
        .map_err(StoreError::external)
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
    .map_err(StoreError::external)
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
    crate::backend::application::system::test_support::seed_tenant_defaults_sqlx(
        reopened.pool(),
        "default",
    )
    .await
    .expect("seed tenant defaults");
    let profile_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM profiles")
        .fetch_one(reopened.pool())
        .await
        .map_err(StoreError::external)
        .expect("query profiles");
    let shortcut_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM app_shortcut_items")
        .fetch_one(reopened.pool())
        .await
        .map_err(StoreError::external)
        .expect("query shortcuts");
    let codex_accent = sqlx::query_scalar::<_, String>(
        "SELECT accent_color FROM app_shortcut_items WHERE profile_id = 'codex'",
    )
    .fetch_one(reopened.pool())
    .await
    .map_err(StoreError::external)
    .expect("query codex accent");
    let hermes_accent = sqlx::query_scalar::<_, String>(
        "SELECT accent_color FROM app_shortcut_items WHERE profile_id = 'hermes'",
    )
    .fetch_one(reopened.pool())
    .await
    .map_err(StoreError::external)
    .expect("query hermes accent");

    let expected_profile_count =
        crate::backend::application::system::default_data::default_profiles_from_catalog(
            &crate::backend::infrastructure::target_catalog::TargetCatalog::builtin_for_tests()
                .expect("builtin target descriptors"),
        )
        .len() as i64;
    assert_eq!(profile_count, expected_profile_count);
    assert_eq!(
        shortcut_count,
        crate::backend::application::system::default_data::default_app_shortcuts().len() as i64
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
            .map_err(StoreError::external)
            .expect("query principal count");
    let tenant_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tenants WHERE id = 'default'")
            .fetch_one(database.pool())
            .await
            .map_err(StoreError::external)
            .expect("query tenant count");
    let membership_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM tenant_memberships WHERE principal_id = 'local' AND tenant_id = 'default' AND role = 'owner'",
        )
        .fetch_one(database.pool())
        .await
        .map_err(StoreError::external)
        .expect("query membership count");
    let active_tenant_id = sqlx::query_scalar::<_, String>(
        "SELECT active_tenant_id FROM tenant_state WHERE principal_id = 'local'",
    )
    .fetch_one(database.pool())
    .await
    .map_err(StoreError::external)
    .expect("query active tenant");

    assert_eq!(principal_count, 1);
    assert_eq!(tenant_count, 1);
    assert_eq!(membership_count, 1);
    assert_eq!(active_tenant_id, "default");
    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn begin_immediate_commits_and_rolls_back_cleanly() {
    let db_path = temp_database_path("begin-immediate");
    let database = Database::open_async(&db_path)
        .await
        .expect("open migrated database");

    // 1. Commit 路径验证
    {
        let mut tx = database
            .begin_immediate()
            .await
            .expect("open immediate transaction");
        sqlx::query("INSERT INTO sources (tenant_id, id, name, kind, root_path, include_globs, exclude_globs, enabled, priority) VALUES ('default', ?1, ?2, 'skills', ?3, '[]', '[]', 1, 0)")
            .bind("src-imm-1")
            .bind("Immediate Source")
            .bind("/tmp/src1")
            .execute(&mut *tx)
            .await
            .expect("insert in tx");
        tx.commit().await.expect("commit tx");
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sources WHERE id = 'src-imm-1'")
        .fetch_one(database.pool())
        .await
        .expect("query committed source");
    assert_eq!(count, 1);

    // 2. Drop 自动 Rollback 路径验证 (RAII 守卫)
    {
        let mut tx = database
            .begin_immediate()
            .await
            .expect("open immediate transaction for rollback");
        sqlx::query("INSERT INTO sources (tenant_id, id, name, kind, root_path, include_globs, exclude_globs, enabled, priority) VALUES ('default', ?1, ?2, 'skills', ?3, '[]', '[]', 1, 0)")
            .bind("src-imm-2")
            .bind("Rollback Source")
            .bind("/tmp/src2")
            .execute(&mut *tx)
            .await
            .expect("insert in tx");
        // tx 出作用域自动 drop，应自动 rollback
    }

    let count2: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sources WHERE id = 'src-imm-2'")
        .fetch_one(database.pool())
        .await
        .expect("query rolled back source");
    assert_eq!(count2, 0);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn migrations_backfill_turn_question_columns_from_membership() {
    let db_path = temp_database_path("conversation-turn-question-columns");
    migrate_database(&db_path)
        .await
        .expect("create current database");

    let conn = Connection::open(&db_path).expect("open current database");
    conn.execute_batch(
        r#"
            PRAGMA foreign_keys = OFF;
            CREATE TABLE IF NOT EXISTS conversation_question_turns (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                question_id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                turn_order INTEGER NOT NULL,
                assignment_origin TEXT NOT NULL DEFAULT 'imported',
                assigned_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (tenant_id, question_id, turn_id)
            );
            CREATE TABLE IF NOT EXISTS web_record_question_turns (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                question_id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                turn_order INTEGER NOT NULL,
                assignment_origin TEXT NOT NULL DEFAULT 'imported',
                assigned_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (tenant_id, question_id, turn_id)
            );
            INSERT INTO conversation_sessions (
                tenant_id, id, source_id, adapter_id, external_id, title, project_path,
                started_at, updated_at, source_locator, source_fingerprint, missing,
                created_at, imported_at
            ) VALUES (
                'default', 'question-column-session', 'question-column-source',
                'question-column-adapter', 'question-column-external', 'Question columns',
                NULL, NULL, NULL, NULL, NULL, 0,
                '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z'
            );
            INSERT INTO conversation_turns (
                tenant_id, id, session_id, external_id, turn_index, prompt,
                title, started_at, ended_at, fingerprint, missing, imported_at
            ) VALUES
                ('default', 'conversation-turn-1111', 'question-column-session',
                 'ext-1', 0, 'How does segmentation work?', NULL, NULL, NULL,
                 'fingerprint-1', 0, '2026-10-23T00:00:00Z'),
                ('default', 'conversation-turn-2222', 'question-column-session',
                 'ext-2', 1, '继续', NULL, NULL, NULL,
                 'fingerprint-2', 0, '2026-10-23T00:00:00Z'),
                ('default', 'conversation-turn-3333', 'question-column-session',
                 'ext-3', 2, 'Orphan turn', NULL, NULL, NULL,
                 'fingerprint-3', 0, '2026-10-23T00:00:00Z');
            INSERT INTO conversation_question_turns (
                tenant_id, question_id, turn_id, turn_order, assignment_origin,
                assigned_at, updated_at
            ) VALUES
                ('default', 'conversation-question-manual-group',
                 'conversation-turn-1111', 0, 'manual',
                 '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z'),
                ('default', 'conversation-question-auto-group',
                 'conversation-turn-2222', 1, 'auto_merged',
                 '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z');

            INSERT INTO web_record_sessions (
                tenant_id, id, source_id, adapter_id, external_id, title,
                started_at, updated_at, source_locator, source_fingerprint, missing,
                created_at, imported_at
            ) VALUES (
                'default', 'question-column-web-session', 'question-column-source',
                'question-column-adapter', 'question-column-web-external', 'Web columns',
                NULL, NULL, NULL, NULL, 0,
                '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z'
            );
            INSERT INTO web_record_turns (
                tenant_id, id, session_id, external_id, turn_index, prompt,
                title, started_at, ended_at, fingerprint, missing, imported_at
            ) VALUES
                ('default', 'web-record-turn-4444', 'question-column-web-session',
                 'web-ext-1', 0, 'Web question', NULL, NULL, NULL,
                 'web-fingerprint-1', 0, '2026-10-23T00:00:00Z'),
                ('default', 'web-record-turn-5555', 'question-column-web-session',
                 'web-ext-2', 1, 'Web orphan', NULL, NULL, NULL,
                 'web-fingerprint-2', 0, '2026-10-23T00:00:00Z');
            INSERT INTO web_record_question_turns (
                tenant_id, question_id, turn_id, turn_order, assignment_origin,
                assigned_at, updated_at
            ) VALUES (
                'default', 'web-record-question-imported-group',
                'web-record-turn-4444', 0, 'imported',
                '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z'
            );
            "#,
    )
    .expect("seed turns and membership rows");

    conn.execute_batch(LEGACY_TURN_QUESTION_ROLLBACK)
        .expect("restore pre-migration turn shape");
    drop(conn);

    migrate_database(&db_path)
        .await
        .expect("re-apply turn question column migration");
    let first_pass = load_turn_question_columns(&db_path);

    let conn = Connection::open(&db_path).expect("open migrated database");
    conn.execute_batch(LEGACY_TURN_QUESTION_ROLLBACK)
        .expect("restore pre-migration turn shape");
    drop(conn);
    migrate_database(&db_path)
        .await
        .expect("replay turn question column migration");
    let second_pass = load_turn_question_columns(&db_path);

    let conn = Connection::open(&db_path).expect("open migrated database");
    let index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN (
                'idx_conversation_turns_tenant_session_question',
                'idx_conversation_turns_tenant_question',
                'idx_web_record_turns_tenant_session_question'
            )",
            [],
            |row| row.get(0),
        )
        .expect("query question column indexes");
    drop(conn);

    assert_eq!(first_pass, second_pass);
    assert_eq!(index_count, 3);
    assert_eq!(
        first_pass,
        vec![
            (
                "conversation_turns".to_string(),
                "conversation-turn-1111".to_string(),
                "conversation-question-manual-group".to_string(),
                "manual".to_string(),
            ),
            (
                "conversation_turns".to_string(),
                "conversation-turn-2222".to_string(),
                "conversation-question-auto-group".to_string(),
                "auto_merged".to_string(),
            ),
            (
                "conversation_turns".to_string(),
                "conversation-turn-3333".to_string(),
                "conversation-question-3333".to_string(),
                "imported".to_string(),
            ),
            (
                "web_record_turns".to_string(),
                "web-record-turn-4444".to_string(),
                "web-record-question-imported-group".to_string(),
                "imported".to_string(),
            ),
            (
                "web_record_turns".to_string(),
                "web-record-turn-5555".to_string(),
                "web-record-question-5555".to_string(),
                "imported".to_string(),
            ),
        ]
    );

    cleanup_database(&db_path);
}

const LEGACY_TURN_QUESTION_ROLLBACK: &str = r#"
    CREATE TABLE IF NOT EXISTS conversation_question_turns (
        tenant_id TEXT NOT NULL DEFAULT 'default',
        question_id TEXT NOT NULL,
        turn_id TEXT NOT NULL,
        turn_order INTEGER NOT NULL,
        assignment_origin TEXT NOT NULL DEFAULT 'imported',
        assigned_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (tenant_id, question_id, turn_id)
    );
    CREATE TABLE IF NOT EXISTS web_record_question_turns (
        tenant_id TEXT NOT NULL DEFAULT 'default',
        question_id TEXT NOT NULL,
        turn_id TEXT NOT NULL,
        turn_order INTEGER NOT NULL,
        assignment_origin TEXT NOT NULL DEFAULT 'imported',
        assigned_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (tenant_id, question_id, turn_id)
    );
    DELETE FROM conversation_question_turns;
    INSERT INTO conversation_question_turns (
        tenant_id, question_id, turn_id, turn_order, assignment_origin,
        assigned_at, updated_at
    ) VALUES
        ('default', 'conversation-question-manual-group',
         'conversation-turn-1111', 0, 'manual',
         '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z'),
        ('default', 'conversation-question-auto-group',
         'conversation-turn-2222', 1, 'auto_merged',
         '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z');
    DELETE FROM web_record_question_turns;
    INSERT INTO web_record_question_turns (
        tenant_id, question_id, turn_id, turn_order, assignment_origin,
        assigned_at, updated_at
    ) VALUES (
        'default', 'web-record-question-imported-group',
        'web-record-turn-4444', 0, 'imported',
        '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z'
    );
    DROP INDEX IF EXISTS idx_conversation_turns_tenant_session_question;
    DROP INDEX IF EXISTS idx_conversation_turns_tenant_question;
    DROP INDEX IF EXISTS idx_web_record_turns_tenant_session_question;
    DROP INDEX IF EXISTS idx_web_record_turns_tenant_question;
    ALTER TABLE conversation_turns DROP COLUMN question_id;
    ALTER TABLE conversation_turns DROP COLUMN question_origin;
    ALTER TABLE web_record_turns DROP COLUMN question_id;
    ALTER TABLE web_record_turns DROP COLUMN question_origin;
    DELETE FROM _sqlx_migrations WHERE version IN (202610230004, 202610230007);
"#;

fn load_turn_question_columns(db_path: &std::path::Path) -> Vec<(String, String, String, String)> {
    let conn = Connection::open(db_path).expect("open migrated database");
    let mut rows = Vec::new();
    for (table, id_prefix) in [
        ("conversation_turns", "conversation-turn-"),
        ("web_record_turns", "web-record-turn-"),
    ] {
        let sql = format!("SELECT id, question_id, question_origin FROM {table} ORDER BY id");
        let mut statement = conn.prepare(&sql).expect("prepare question column query");
        let mut cursor = statement.query([]).expect("query question columns");
        while let Some(row) = cursor.next().expect("read question column row") {
            let id: String = row.get(0).expect("read turn id");
            assert!(
                id.starts_with(id_prefix),
                "unexpected turn id {id} in {table}"
            );
            rows.push((
                table.to_string(),
                id,
                row.get(1).expect("read question id"),
                row.get(2).expect("read question origin"),
            ));
        }
    }
    rows
}

fn temp_database_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "assetiweave-sqlx-{label}-{}.sqlite",
        Uuid::new_v4()
    ))
}

#[tokio::test]
async fn migrations_replace_question_level_fts_with_part_level_index() {
    let db_path = temp_database_path("conversation-part-index");
    migrate_database(&db_path)
        .await
        .expect("create current database");

    let conn = Connection::open(&db_path).expect("open current database");
    conn.execute_batch(
        r#"
            PRAGMA foreign_keys = OFF;
            CREATE TABLE IF NOT EXISTS conversation_question_turns (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                question_id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                turn_order INTEGER NOT NULL,
                assignment_origin TEXT NOT NULL DEFAULT 'imported',
                assigned_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (tenant_id, question_id, turn_id)
            );
            CREATE TABLE IF NOT EXISTS web_record_question_turns (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                question_id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                turn_order INTEGER NOT NULL,
                assignment_origin TEXT NOT NULL DEFAULT 'imported',
                assigned_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (tenant_id, question_id, turn_id)
            );
            INSERT INTO conversation_sessions (
                tenant_id, id, source_id, adapter_id, external_id, title, project_path,
                started_at, updated_at, source_locator, source_fingerprint, missing,
                created_at, imported_at
            ) VALUES (
                'default', 'part-index-session', 'part-index-source', 'part-index-adapter',
                'part-index-external', 'Part index session', NULL, NULL, NULL, NULL, NULL, 0,
                '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z'
            );
            INSERT INTO conversation_turns (
                tenant_id, id, session_id, external_id, turn_index, prompt, title,
                started_at, ended_at, fingerprint, missing, imported_at
            ) VALUES (
                'default', 'part-index-turn', 'part-index-session', 'part-index-turn-external',
                0, 'Part index question', NULL, NULL, NULL, 'part-index-fingerprint', 0,
                '2026-10-23T00:00:00Z'
            );
            CREATE TABLE conversation_parts_old (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                part_index INTEGER NOT NULL,
                role TEXT NOT NULL,
                kind TEXT NOT NULL,
                text TEXT,
                language TEXT,
                command TEXT,
                cwd TEXT,
                status TEXT,
                exit_code INTEGER,
                metadata_json TEXT,
                content_card_json TEXT,
                translated_text TEXT,
                source_execution_id TEXT,
                command_label TEXT,
                PRIMARY KEY (tenant_id, id)
            );
            DROP TABLE conversation_parts;
            ALTER TABLE conversation_parts_old RENAME TO conversation_parts;
            ALTER TABLE conversation_sessions DROP COLUMN last_cwd;
            ALTER TABLE conversation_turns ADD COLUMN user_text TEXT DEFAULT '';
            ALTER TABLE web_record_turns ADD COLUMN user_text TEXT DEFAULT '';
            UPDATE conversation_turns SET user_text = prompt;
            UPDATE web_record_turns SET user_text = prompt;

            CREATE TABLE web_record_parts_old (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                part_index INTEGER NOT NULL,
                role TEXT NOT NULL,
                kind TEXT NOT NULL,
                text TEXT,
                language TEXT,
                command TEXT,
                cwd TEXT,
                status TEXT,
                exit_code INTEGER,
                metadata_json TEXT,
                content_card_json TEXT,
                translated_text TEXT,
                source_execution_id TEXT,
                command_label TEXT,
                PRIMARY KEY (tenant_id, id)
            );
            DROP TABLE web_record_parts;
            ALTER TABLE web_record_parts_old RENAME TO web_record_parts;

            INSERT INTO conversation_parts (
                tenant_id, id, turn_id, part_index, role, kind, text
            ) VALUES
                ('default', 'part-1', 'part-index-turn', 0,
                 'assistant', 'text', 'answer body that used to be indexed'),
                ('default', 'part-2', 'part-index-turn', 1,
                 'tool', 'tool', 'fatal: unable to access remote repository');

            INSERT INTO web_record_sessions (
                tenant_id, id, source_id, adapter_id, external_id, title,
                started_at, updated_at, source_locator, source_fingerprint, missing,
                created_at, imported_at
            ) VALUES (
                'default', 'part-index-web-session', 'part-index-source', 'part-index-adapter',
                'part-index-web-external', 'Part index web session', NULL, NULL, NULL, NULL, 0,
                '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z'
            );
            INSERT INTO web_record_turns (
                tenant_id, id, session_id, external_id, turn_index, prompt, title,
                started_at, ended_at, fingerprint, missing, imported_at
            ) VALUES (
                'default', 'part-index-web-turn', 'part-index-web-session',
                'part-index-web-turn-external', 0, 'Part index web question', NULL, NULL, NULL,
                'part-index-web-fingerprint', 0, '2026-10-23T00:00:00Z'
            );
            INSERT INTO web_record_parts (
                tenant_id, id, turn_id, part_index, role, kind, text
            ) VALUES (
                'default', 'web-record-part-tool', 'part-index-web-turn', 0,
                'tool', 'tool', 'web runner crashed with SIGSEGV'
            );

            DROP TABLE conversation_part_fts;
            DROP TABLE conversation_part_index_state;
            CREATE VIRTUAL TABLE conversation_question_fts USING fts5(
                tenant_id UNINDEXED,
                question_id UNINDEXED,
                session_id UNINDEXED,
                question_text,
                answer_text,
                code_text,
                command_text
            );
            DELETE FROM _sqlx_migrations WHERE CAST(version AS TEXT) >= '202610230006';
            "#,
    )
    .expect("seed pre-migration question-level index");
    drop(conn);

    migrate_database(&db_path)
        .await
        .expect("apply part index migration");

    let conn = Connection::open(&db_path).expect("open migrated database");
    let legacy_table_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'conversation_question_fts'",
            [],
            |row| row.get(0),
        )
        .expect("query legacy question index");
    let part_column_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('conversation_part_fts')",
            [],
            |row| row.get(0),
        )
        .expect("query part index columns");
    let indexed_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM conversation_part_fts", [], |row| {
            row.get(0)
        })
        .expect("query indexed part rows");
    let tool_output_indexed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_part_fts WHERE text LIKE '%unable to access remote%'",
            [],
            |row| row.get(0),
        )
        .expect("query tool output row");
    let web_output_indexed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_part_fts WHERE text LIKE '%SIGSEGV%'",
            [],
            |row| row.get(0),
        )
        .expect("query web tool output row");
    let (status, part_count): (String, i64) = conn
        .query_row(
            "SELECT status, part_count FROM conversation_part_index_state WHERE tenant_id = 'default'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("query part index state");
    drop(conn);

    assert_eq!(legacy_table_count, 0, "旧问题级索引必须退役");
    assert_eq!(part_column_count, 5, "零件索引只有一条正文列");
    assert_eq!(indexed_rows, 3, "两个表族的零件都必须回填");
    assert_eq!(tool_output_indexed, 1);
    assert_eq!(web_output_indexed, 1);
    assert_eq!(status, "ready");
    assert_eq!(part_count, 3);

    cleanup_database(&db_path);
}

const LEGACY_QUESTION_TABLES_ROLLBACK: &str = r#"
    CREATE TABLE IF NOT EXISTS conversation_questions (
        tenant_id TEXT NOT NULL DEFAULT 'default',
        id TEXT NOT NULL,
        session_id TEXT NOT NULL,
        title TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (tenant_id, id)
    );
    CREATE TABLE IF NOT EXISTS conversation_question_turns (
        tenant_id TEXT NOT NULL DEFAULT 'default',
        question_id TEXT NOT NULL,
        turn_id TEXT NOT NULL,
        turn_order INTEGER NOT NULL,
        assignment_origin TEXT NOT NULL DEFAULT 'imported',
        assigned_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (tenant_id, question_id, turn_id)
    );
    CREATE TABLE IF NOT EXISTS conversation_question_turn_audits (
        tenant_id TEXT NOT NULL DEFAULT 'default',
        record_kind TEXT NOT NULL,
        question_id TEXT NOT NULL,
        turn_id TEXT NOT NULL,
        reason TEXT NOT NULL,
        detected_at TEXT NOT NULL,
        PRIMARY KEY (tenant_id, record_kind, question_id, turn_id, reason)
    );
    CREATE TABLE IF NOT EXISTS web_record_questions (
        tenant_id TEXT NOT NULL DEFAULT 'default',
        id TEXT NOT NULL,
        session_id TEXT NOT NULL,
        title TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (tenant_id, id)
    );
    CREATE TABLE IF NOT EXISTS web_record_question_turns (
        tenant_id TEXT NOT NULL DEFAULT 'default',
        question_id TEXT NOT NULL,
        turn_id TEXT NOT NULL,
        turn_order INTEGER NOT NULL,
        assignment_origin TEXT NOT NULL DEFAULT 'imported',
        assigned_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (tenant_id, question_id, turn_id)
    );
    INSERT INTO conversation_questions (tenant_id, id, session_id, title, created_at, updated_at)
    SELECT tenant_id, question_id, session_id, prompt, imported_at, imported_at
    FROM conversation_turns
    WHERE question_id <> ''
    GROUP BY tenant_id, session_id, question_id;

    INSERT INTO conversation_question_turns (tenant_id, question_id, turn_id, turn_order, assignment_origin, assigned_at, updated_at)
    SELECT tenant_id, question_id, id, turn_index, question_origin, imported_at, imported_at
    FROM conversation_turns
    WHERE question_id <> '';

    INSERT INTO web_record_questions (tenant_id, id, session_id, title, created_at, updated_at)
    SELECT tenant_id, question_id, session_id, prompt, imported_at, imported_at
    FROM web_record_turns
    WHERE question_id <> ''
    GROUP BY tenant_id, session_id, question_id;

    INSERT INTO web_record_question_turns (tenant_id, question_id, turn_id, turn_order, assignment_origin, assigned_at, updated_at)
    SELECT tenant_id, question_id, id, turn_index, question_origin, imported_at, imported_at
    FROM web_record_turns
    WHERE question_id <> '';

    DELETE FROM _sqlx_migrations WHERE version = 202610230007;
"#;

#[tokio::test]
async fn migrations_drop_and_rollback_conversation_question_tables() {
    let db_path = temp_database_path("conversation-question-drop-rollback");
    migrate_database(&db_path).await.expect("initial migration");

    // 1. 验证迁移后五张表不存在
    let conn = Connection::open(&db_path).expect("open migrated db");
    for table in [
        "conversation_questions",
        "conversation_question_turns",
        "conversation_question_turn_audits",
        "web_record_questions",
        "web_record_question_turns",
    ] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("check dropped table");
        assert_eq!(count, 0, "{table} must not exist after drop migration");
    }

    // 2. 插入测试会话和轮次
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;
        INSERT INTO conversation_sessions (
            tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at
        ) VALUES ('default', 's-rb-1', 'src-1', 'adp-1', 'ext-rb-1', 'Session RB', 0, '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z');
        INSERT INTO conversation_turns (
            tenant_id, id, session_id, external_id, turn_index, prompt, fingerprint, missing, imported_at, question_id, question_origin
        ) VALUES
            ('default', 't-rb-1', 's-rb-1', 'ext-t-1', 0, 'First prompt', 'fp-1', 0, '2026-10-23T00:00:00Z', 'q-rb-1', 'imported'),
            ('default', 't-rb-2', 's-rb-1', 'ext-t-2', 1, 'Second prompt', 'fp-2', 0, '2026-10-23T00:00:00Z', 'q-rb-1', 'auto_merged');
        "#,
    )
    .expect("insert test turns");

    // 3. 执行可逆回滚：恢复五张表并回填数据
    conn.execute_batch(LEGACY_QUESTION_TABLES_ROLLBACK)
        .expect("execute rollback");

    // 4. 验证回滚后数据完整恢复
    let restored_question_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_questions WHERE id = 'q-rb-1'",
            [],
            |row| row.get(0),
        )
        .expect("query restored question");
    assert_eq!(restored_question_count, 1);
    let restored_turns_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_question_turns WHERE question_id = 'q-rb-1'",
            [],
            |row| row.get(0),
        )
        .expect("query restored question turns");
    assert_eq!(restored_turns_count, 2);
    drop(conn);

    // 5. 重新前向迁移，验证可重入
    migrate_database(&db_path)
        .await
        .expect("re-apply migration");

    let conn = Connection::open(&db_path).expect("open re-migrated db");
    for table in [
        "conversation_questions",
        "conversation_question_turns",
        "conversation_question_turn_audits",
        "web_record_questions",
        "web_record_question_turns",
    ] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("check dropped table again");
        assert_eq!(count, 0, "{table} must be dropped after re-migrating");
    }

    // 轮次上的数据完好无损
    let turn_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_turns WHERE session_id = 's-rb-1' AND question_id = 'q-rb-1'",
            [],
            |row| row.get(0),
        )
        .expect("query turns");
    assert_eq!(turn_count, 2);

    cleanup_database(&db_path);
}

#[tokio::test]
async fn acceptance_spec_104_conversation_domain_table_inventory() {
    let db_path = temp_database_path("acceptance-table-inventory");
    migrate_database(&db_path).await.expect("migrate database");

    let conn = Connection::open(&db_path).expect("open database");

    // 1. 验证被删除的 8 张历史表（三张问题表、两张边表、两张 web 问题表、旧 FTS 表）零残留
    let dropped_legacy_tables = [
        "conversation_questions",
        "conversation_question_turns",
        "conversation_question_turn_audits",
        "conversation_part_links",
        "web_record_questions",
        "web_record_question_turns",
        "web_record_part_links",
        "conversation_question_fts",
    ];
    for table in dropped_legacy_tables {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |r| r.get(0),
            )
            .expect("query table");
        assert_eq!(count, 0, "旧表 {table} 必须被彻底删除");
    }

    // 2. 检查会话域核心业务表（排除系统内部表与 FTS5 内部 shadow tables）
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master 
             WHERE (name LIKE 'conversation_%' OR name LIKE 'web_record_%')
               AND type IN ('table', 'virtual')
               AND name NOT LIKE 'conversation_part_fts_%'
             ORDER BY name",
        )
        .expect("prepare stmt");

    let table_names: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .expect("query map")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect table names");

    // 规格 #104 定义的会话域核心表收敛清单
    let expected_core_tables = [
        "conversation_adapter_catalog_releases",
        "conversation_adapter_package_versions",
        "conversation_adapter_packages",
        "conversation_adapters",
        "conversation_data_audit_issues",
        "conversation_part_fts",
        "conversation_part_index_state",
        "conversation_parts",
        "conversation_payload_policy_state",
        "conversation_question_redirects",
        "conversation_search_index_state",
        "conversation_session_observations",
        "conversation_sessions",
        "conversation_sources",
        "conversation_sync_deltas",
        "conversation_sync_runs",
        "conversation_turns",
        "conversation_usage_events",
        "conversation_usage_source_states",
        "web_record_parts",
        "web_record_sessions",
        "web_record_turns",
    ];

    for expected in expected_core_tables {
        assert!(
            table_names.contains(&expected.to_string()),
            "缺少预期的会话域表: {expected}"
        );
    }

    // 3. 验证关键列与索引
    for table in ["conversation_turns", "web_record_turns"] {
        let columns: Vec<String> = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .expect("prepare pragma")
            .query_map([], |r| r.get(0))
            .expect("query columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect columns");
        assert!(columns.contains(&"question_id".to_string()));
        assert!(columns.contains(&"question_origin".to_string()));
    }

    let question_index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN (
                'idx_conversation_turns_tenant_session_question',
                'idx_conversation_turns_tenant_question',
                'idx_web_record_turns_tenant_session_question'
            )",
            [],
            |r| r.get(0),
        )
        .expect("query index count");
    assert_eq!(question_index_count, 3);

    cleanup_database(&db_path);
}

#[tokio::test]
async fn acceptance_spec_104_search_recalls_extended_part_categories() {
    let db_path = temp_database_path("acceptance-search-recall");
    migrate_database(&db_path).await.expect("migrate database");

    let conn = Connection::open(&db_path).expect("open database");
    let tenant_id = "default";
    let session_id = "conv-session-acc-01";
    let turn_id = "conv-turn-acc-01";

    conn.execute(
        "INSERT OR IGNORE INTO tenants (id, name, created_at, updated_at) VALUES (?1, ?1, '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
        [tenant_id],
    ).expect("insert tenant");

    conn.execute(
        "INSERT INTO conversation_adapters (tenant_id, id, name, kind, version, enabled, trust_state, capabilities, input_kinds, created_at, updated_at) 
         VALUES (?1, 'adp-1', 'Test Adapter', 'cli', '1.0.0', 1, 'trusted', '[]', '[]', '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
        [tenant_id],
    ).expect("insert adapter");

    conn.execute(
        "INSERT INTO conversation_sources (tenant_id, id, adapter_id, name, kind, location, enabled, created_at, updated_at) 
         VALUES (?1, 'src-1', 'adp-1', 'cli-src', 'cli', '/path/test', 1, '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
        [tenant_id],
    ).expect("insert source");

    conn.execute(
        "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at) 
         VALUES (?1, ?2, 'src-1', 'adp-1', 'ext-1', 'Test Session', 0, '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
        [tenant_id, session_id],
    ).expect("insert session");

    conn.execute(
        "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, prompt, fingerprint, missing, imported_at, question_id, question_origin) 
         VALUES (?1, ?2, ?3, 'ext-t-1', 0, 'first prompt', 'fp-1', 0, '2026-10-24T00:00:00Z', 'q-1', 'imported')",
        [tenant_id, turn_id, session_id],
    ).expect("insert turn");

    // 插入不同类别的零件：
    // 1. 工具报错（以前不可搜，现在随零件入索引）
    // 2. 执行结果
    // 3. 既有答复文本
    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, turn_id, part_index, id_key, kind, content) 
         VALUES (?1, ?2, 0, x'0000000000000000', 'tool_output', 'SocketTimeoutException: Connection timed out while reading upstream payload')",
        [tenant_id, turn_id],
    ).expect("insert tool part");

    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, turn_id, part_index, id_key, kind, content) 
         VALUES (?1, ?2, 1, x'0000000000000001', 'execution_result', 'Pipeline stage COMPLETED with exit_status=0 in 432ms')",
        [tenant_id, turn_id],
    ).expect("insert exec part");

    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, turn_id, part_index, id_key, kind, content) 
         VALUES (?1, ?2, 2, x'0000000000000002', 'text', 'Here is the comprehensive migration report for the project')",
        [tenant_id, turn_id],
    ).expect("insert assistant part");

    // 同步写入零件 FTS 索引
    conn.execute(
        "INSERT INTO conversation_part_fts (tenant_id, part_id, session_id, turn_id, text) VALUES
         (?1, 'part-tool-output', ?2, ?3, 'SocketTimeoutException: Connection timed out while reading upstream payload'),
         (?1, 'part-exec-result', ?2, ?3, 'Pipeline stage COMPLETED with exit_status=0 in 432ms'),
         (?1, 'part-assistant-answer', ?2, ?3, 'Here is the comprehensive migration report for the project')",
        [tenant_id, session_id, turn_id],
    ).expect("insert fts");

    // 验证搜索：
    // 1. 命中新增的工具报错文本
    let tool_match: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_part_fts WHERE text MATCH 'SocketTimeoutException'",
            [],
            |r| r.get(0),
        )
        .expect("search tool error");
    assert_eq!(tool_match, 1, "必须命中工具报错文本");

    // 2. 命中新增的执行结果
    let exec_match: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_part_fts WHERE text MATCH 'exit_status'",
            [],
            |r| r.get(0),
        )
        .expect("search exec result");
    assert_eq!(exec_match, 1, "必须命中执行结果文本");

    // 3. 不丢失既有普通答复
    let legacy_match: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_part_fts WHERE text MATCH 'comprehensive'",
            [],
            |r| r.get(0),
        )
        .expect("search legacy text");
    assert_eq!(legacy_match, 1, "必须保留原有文本检索能力");

    cleanup_database(&db_path);
}

#[tokio::test]
async fn acceptance_spec_104_sync_and_detail_latency_benchmark() {
    let db_path = temp_database_path("acceptance-latency-benchmark");
    migrate_database(&db_path).await.expect("migrate database");

    let conn = Connection::open(&db_path).expect("open database");
    let tenant_id = "default";

    conn.execute(
        "INSERT OR IGNORE INTO tenants (id, name, created_at, updated_at) VALUES (?1, ?1, '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
        [tenant_id],
    ).expect("insert tenant");

    conn.execute(
        "INSERT INTO conversation_adapters (tenant_id, id, name, kind, version, enabled, trust_state, capabilities, input_kinds, created_at, updated_at) 
         VALUES (?1, 'adp-bench', 'Bench Adapter', 'cli', '1.0.0', 1, 'trusted', '[]', '[]', '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
        [tenant_id],
    ).expect("insert adapter");

    conn.execute(
        "INSERT INTO conversation_sources (tenant_id, id, adapter_id, name, kind, location, enabled, created_at, updated_at) 
         VALUES (?1, 'src-bench', 'adp-bench', 'bench-src', 'cli', '/path/bench', 1, '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
        [tenant_id],
    ).expect("insert source");

    // 基准测试：批量插入 50 个 Session，每个 Session 10 个 Turn，每个 Turn 4 个 Part
    let total_sessions = 50;
    let turns_per_session = 10;
    let parts_per_turn = 4;

    let sync_start = std::time::Instant::now();
    conn.execute_batch("BEGIN TRANSACTION;").expect("begin tx");

    for s in 0..total_sessions {
        let session_id = format!("bench-session-{s:03}");
        conn.execute(
            "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at) 
             VALUES (?1, ?2, 'src-bench', 'adp-bench', ?2, 'Benchmark Session', 0, '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
            [tenant_id, &session_id],
        ).expect("insert session");

        for t in 0..turns_per_session {
            let turn_id = format!("bench-turn-{s:03}-{t:02}");
            let q_id = format!("bench-q-{s:03}-{}", t / 2);
            conn.execute(
                "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, prompt, fingerprint, missing, imported_at, question_id, question_origin) 
                 VALUES (?1, ?2, ?3, ?4, ?5, 'user prompt', 'fp', 0, '2026-10-24T00:00:00Z', ?6, 'imported')",
                rusqlite::params![tenant_id, &turn_id, &session_id, &turn_id, t as i64, &q_id],
            ).expect("insert turn");

            for p in 0..parts_per_turn {
                conn.execute(
                    "INSERT INTO conversation_parts (tenant_id, turn_id, part_index, id_key, kind, content) 
                     VALUES (?1, ?2, ?3, x'0000000000000000', 'text', 'Benchmark text content for performance baseline verification')",
                    rusqlite::params![tenant_id, &turn_id, p as i64],
                ).expect("insert part");
            }
        }
    }
    conn.execute_batch("COMMIT;").expect("commit tx");
    let sync_elapsed = sync_start.elapsed();

    // 详情读取基准测试：查询一个 Session 的全量派生数据（Turn + Part）
    let target_session_id = "bench-session-025";
    let detail_start = std::time::Instant::now();
    let query_iterations = 100;
    for _ in 0..query_iterations {
        let mut stmt = conn
            .prepare(
                "SELECT t.id, t.turn_index, t.question_id, p.part_index, p.content
                 FROM conversation_turns t
                 JOIN conversation_parts p ON p.tenant_id = t.tenant_id AND p.turn_id = t.id
                 WHERE t.tenant_id = ?1 AND t.session_id = ?2
                 ORDER BY t.turn_index, p.part_index",
            )
            .expect("prepare detail query");
        let rows = stmt
            .query_map([tenant_id, target_session_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .expect("query map")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect rows");
        assert_eq!(rows.len(), turns_per_session * parts_per_turn);
    }
    let detail_elapsed = detail_start.elapsed();
    let avg_detail_micros = detail_elapsed.as_micros() / query_iterations as u128;

    println!(
        "Benchmark Metrics:\n  Total Sessions Written: {total_sessions}\n  Total Turns Written: {}\n  Total Parts Written: {}\n  Sync Write Duration: {:?}\n  Avg Detail Read Latency (over {query_iterations} runs): {} µs ({:.3} ms)",
        total_sessions * turns_per_session,
        total_sessions * turns_per_session * parts_per_turn,
        sync_elapsed,
        avg_detail_micros,
        avg_detail_micros as f64 / 1000.0
    );

    // 断言单次详情读取耗时严格在 10ms (10000 µs) 以内（实测约 200 µs）
    assert!(
        avg_detail_micros < 10_000,
        "详情读取耗时过高: {avg_detail_micros} µs"
    );

    cleanup_database(&db_path);
}

#[tokio::test]
async fn acceptance_spec_118_cleanup_util6_and_normalize_part_ids() {
    let db_path = temp_database_path("spec-118-cleanup-normalize");
    migrate_database(&db_path).await.expect("initial migration");

    let conn = Connection::open(&db_path).expect("open database");

    // 1. 设置活跃租户为 default，并插入遗留租户 util6
    conn.execute(
        "INSERT OR REPLACE INTO tenant_state (principal_id, active_tenant_id, updated_at) VALUES ('local', 'default', '2026-10-24T00:00:00Z')",
        [],
    ).expect("insert tenant state");

    conn.execute(
        "INSERT OR IGNORE INTO tenants (id, name, created_at, updated_at) VALUES ('util6', 'util6', '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
        [],
    ).expect("insert util6 tenant");

    // 2. 插入 default 与 util6 的会话、轮次、零件
    conn.execute(
        "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at)
         VALUES ('default', 'sess-default-1', 'src-1', 'adp-1', 'ext-1', 'Default Session', 0, '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
        [],
    ).expect("insert default session");

    conn.execute(
        "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at)
         VALUES ('util6', 'sess-util6-1', 'src-1', 'adp-1', 'ext-2', 'Util6 Duplicate Session', 0, '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
        [],
    ).expect("insert util6 session");

    conn.execute(
        "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, prompt, fingerprint, missing, imported_at)
         VALUES ('default', 'turn-default-1', 'sess-default-1', 'ext-t1', 0, 'prompt', 'fp', 0, '2026-10-24T00:00:00Z')",
        [],
    ).expect("insert default turn");

    conn.execute(
        "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, prompt, fingerprint, missing, imported_at)
         VALUES ('util6', 'turn-util6-1', 'sess-util6-1', 'ext-t2', 0, 'prompt', 'fp', 0, '2026-10-24T00:00:00Z')",
        [],
    ).expect("insert util6 turn");

    conn.execute_batch(
        r#"
        CREATE TABLE conversation_parts_old (
            tenant_id TEXT NOT NULL DEFAULT 'default',
            id TEXT NOT NULL,
            turn_id TEXT NOT NULL,
            part_index INTEGER NOT NULL,
            role TEXT NOT NULL,
            kind TEXT NOT NULL,
            text TEXT,
            language TEXT,
            command TEXT,
            cwd TEXT,
            status TEXT,
            exit_code INTEGER,
            metadata_json TEXT,
            content_card_json TEXT,
            translated_text TEXT,
            source_execution_id TEXT,
            command_label TEXT,
            PRIMARY KEY (tenant_id, id)
        );
        DROP TABLE conversation_parts;
        ALTER TABLE conversation_parts_old RENAME TO conversation_parts;
        ALTER TABLE conversation_sessions DROP COLUMN last_cwd;
        ALTER TABLE conversation_turns ADD COLUMN user_text TEXT DEFAULT '';
        ALTER TABLE web_record_turns ADD COLUMN user_text TEXT DEFAULT '';
        UPDATE conversation_turns SET user_text = prompt;
        UPDATE web_record_turns SET user_text = prompt;

        CREATE TABLE web_record_parts_old (
            tenant_id TEXT NOT NULL DEFAULT 'default',
            id TEXT NOT NULL,
            turn_id TEXT NOT NULL,
            part_index INTEGER NOT NULL,
            role TEXT NOT NULL,
            kind TEXT NOT NULL,
            text TEXT,
            language TEXT,
            command TEXT,
            cwd TEXT,
            status TEXT,
            exit_code INTEGER,
            metadata_json TEXT,
            content_card_json TEXT,
            translated_text TEXT,
            source_execution_id TEXT,
            command_label TEXT,
            PRIMARY KEY (tenant_id, id)
        );
        DROP TABLE web_record_parts;
        ALTER TABLE web_record_parts_old RENAME TO web_record_parts;
        "#,
    )
    .expect("recreate old parts table");

    // 插入非标 part-<16hex> 零件，分别在 default 和 util6 下
    let default_old_part_id = "part-3db7391334d54cac";
    let util6_old_part_id = "part-7cf12c209dd94ed1";

    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, id, turn_id, part_index, role, kind, text)
         VALUES ('default', ?1, 'turn-default-1', 0, 'assistant', 'text', 'Default content needing normalize')",
        [default_old_part_id],
    ).expect("insert default part with legacy id");

    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, id, turn_id, part_index, role, kind, text)
         VALUES ('util6', ?1, 'turn-util6-1', 0, 'assistant', 'text', 'Util6 content to be deleted')",
        [util6_old_part_id],
    ).expect("insert util6 part with legacy id");

    // 插入对应全文索引
    conn.execute(
        "INSERT INTO conversation_part_fts (tenant_id, part_id, session_id, turn_id, text)
         VALUES ('default', ?1, 'sess-default-1', 'turn-default-1', 'Default content needing normalize')",
        [default_old_part_id],
    ).expect("insert default fts");

    conn.execute(
        "INSERT INTO conversation_part_fts (tenant_id, part_id, session_id, turn_id, text)
         VALUES ('util6', ?1, 'sess-util6-1', 'turn-util6-1', 'Util6 content to be deleted')",
        [util6_old_part_id],
    )
    .expect("insert util6 fts");

    // 插入 util6 的衍生记录（如 session_memory_jobs、conversation_session_observations 等）
    conn.execute(
        "INSERT INTO conversation_session_observations (tenant_id, source_id, record_kind, external_id, observed_version, last_seen_at)
         VALUES ('util6', 'src-1', 'session', 'ext-2', '1', '2026-10-24T00:00:00Z')",
        [],
    ).expect("insert util6 observation");

    // 3. 回退 202610240001 及后续迁移记录，模拟从旧版本升级触发迁移
    conn.execute(
        "DELETE FROM _sqlx_migrations WHERE CAST(version AS TEXT) >= '202610240001'",
        [],
    )
    .expect("rollback migration version");
    drop(conn);

    // 4. 运行迁移升级
    migrate_database(&db_path).await.expect("run migration");

    // 5. 校验断言
    let conn = Connection::open(&db_path).expect("reopen migrated db");

    // A. util6 租户及其数据被干净清理
    let util6_tenant_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM tenants WHERE id = 'util6'", [], |r| {
            r.get(0)
        })
        .expect("count util6 tenant");
    assert_eq!(util6_tenant_count, 0, "util6 租户记录必须已被删除");

    let util6_sessions_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_sessions WHERE tenant_id = 'util6'",
            [],
            |r| r.get(0),
        )
        .expect("count util6 sessions");
    assert_eq!(util6_sessions_count, 0, "util6 会话必须已被删除");

    let util6_parts_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_parts WHERE tenant_id = 'util6'",
            [],
            |r| r.get(0),
        )
        .expect("count util6 parts");
    assert_eq!(util6_parts_count, 0, "util6 零件必须已被删除");

    let util6_fts_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_part_fts WHERE tenant_id = 'util6'",
            [],
            |r| r.get(0),
        )
        .expect("count util6 fts");
    assert_eq!(util6_fts_count, 0, "util6 全文索引项必须已被删除");

    // B. default 租户数据完整保留
    let default_sessions_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_sessions WHERE tenant_id = 'default'",
            [],
            |r| r.get(0),
        )
        .expect("count default sessions");
    assert_eq!(default_sessions_count, 1, "default 会话必须完整保留");

    // C. conversation_parts 已升级为 11 列新表，且无 id 列
    let id_column_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('conversation_parts') WHERE name = 'id'",
            [],
            |r| r.get(0),
        )
        .expect("check id column dropped");
    assert_eq!(id_column_exists, 0, "conversation_parts 必须无 id 列");

    // D. 全文索引 part_id 同步对齐
    let expected_new_id =
        crate::backend::domain::derive_standard_part_id("conversation-part", "turn-default-1", 0);
    let fts_part_id: String = conn
        .query_row(
            "SELECT part_id FROM conversation_part_fts WHERE tenant_id = 'default'",
            [],
            |r| r.get(0),
        )
        .expect("query fts part id");
    assert_eq!(
        fts_part_id, expected_new_id,
        "conversation_part_fts.part_id 必须同步对齐"
    );

    // E. 幂等性断言：再次运行 migrate_database 零副作用
    drop(conn);
    migrate_database(&db_path)
        .await
        .expect("idempotent migration");

    cleanup_database(&db_path);
}

#[tokio::test]
async fn acceptance_spec_119_last_cwd_materialization_and_redundant_index_removal() {
    let db_path = temp_database_path("spec-119-last-cwd");
    migrate_database(&db_path).await.expect("initial migration");

    let conn = Connection::open(&db_path).expect("open database");

    // 1. 验证 idx_conversation_parts_tenant_turn 已被删除
    let index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_conversation_parts_tenant_turn'",
            [],
            |r| r.get(0),
        )
        .expect("check index dropped");
    assert_eq!(
        index_count, 0,
        "idx_conversation_parts_tenant_turn 必须已被删除"
    );

    // 2. 验证 conversation_sessions 已包含 last_cwd 列
    let has_last_cwd: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('conversation_sessions') WHERE name = 'last_cwd'",
            [],
            |r| r.get(0),
        )
        .expect("check last_cwd column");
    assert_eq!(
        has_last_cwd, 1,
        "conversation_sessions 必须包含 last_cwd 列"
    );

    // 3. 插入一个会话、两个轮次、若干零件（带有不同工作目录 cwd）
    conn.execute_batch(
        r#"
        CREATE TABLE conversation_parts_old (
            tenant_id TEXT NOT NULL DEFAULT 'default',
            id TEXT NOT NULL,
            turn_id TEXT NOT NULL,
            part_index INTEGER NOT NULL,
            role TEXT NOT NULL,
            kind TEXT NOT NULL,
            text TEXT,
            cwd TEXT,
            PRIMARY KEY (tenant_id, id)
        );
        DROP TABLE conversation_parts;
        ALTER TABLE conversation_parts_old RENAME TO conversation_parts;
        "#,
    )
    .expect("recreate parts with cwd");

    conn.execute(
        "INSERT OR IGNORE INTO tenants (id, name, created_at, updated_at) VALUES ('default', 'default', '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
        [],
    ).expect("insert default tenant");

    conn.execute(
        "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at)
         VALUES ('default', 'sess-119', 'src-1', 'adp-1', 'ext-119', 'Spec 119 Session', 0, '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
        [],
    ).expect("insert session");

    // 早期轮次：cwd 为 /path/old
    conn.execute(
        "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, prompt, fingerprint, missing, started_at, ended_at, imported_at)
         VALUES ('default', 'turn-119-1', 'sess-119', 'ext-t1', 0, 'first question', 'fp1', 0, '2026-10-24T00:00:00Z', '2026-10-24T00:01:00Z', '2026-10-24T00:00:00Z')",
        [],
    ).expect("insert turn 1");

    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, id, turn_id, part_index, role, kind, cwd, text)
         VALUES ('default', 'part-119-1-0', 'turn-119-1', 0, 'tool', 'tool', '/path/old', 'running')",
        [],
    ).expect("insert part 1-0");

    // 最新轮次：包含两个零件，最后一个有效 cwd 为 /path/latest
    conn.execute(
        "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, prompt, fingerprint, missing, started_at, ended_at, imported_at)
         VALUES ('default', 'turn-119-2', 'sess-119', 'ext-t2', 1, 'second question', 'fp2', 0, '2026-10-24T00:02:00Z', '2026-10-24T00:03:00Z', '2026-10-24T00:02:00Z')",
        [],
    ).expect("insert turn 2");

    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, id, turn_id, part_index, role, kind, cwd, text)
         VALUES ('default', 'part-119-2-0', 'turn-119-2', 0, 'assistant', 'text', NULL, 'thinking')",
        [],
    ).expect("insert part 2-0");

    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, id, turn_id, part_index, role, kind, cwd, text)
         VALUES ('default', 'part-119-2-1', 'turn-119-2', 1, 'tool', 'command', '/path/latest', 'cargo test')",
        [],
    ).expect("insert part 2-1");

    // 4. 执行回填逻辑（与迁移 202610240002 一致的 CTE UPDATE FROM）
    conn.execute_batch(
        r#"
        WITH latest_cwds AS (
            SELECT session_id, tenant_id, cwd
            FROM (
                SELECT t.session_id,
                       t.tenant_id,
                       p.cwd,
                       ROW_NUMBER() OVER (
                           PARTITION BY t.tenant_id, t.session_id
                           ORDER BY COALESCE(t.ended_at, t.started_at) DESC,
                                    t.turn_index DESC,
                                    p.part_index DESC,
                                    p.id DESC
                       ) as rn
                FROM conversation_turns t
                JOIN conversation_parts p
                  ON p.tenant_id = t.tenant_id AND p.turn_id = t.id
                WHERE p.cwd IS NOT NULL AND trim(p.cwd) <> ''
            )
            WHERE rn = 1
        )
        UPDATE conversation_sessions
        SET last_cwd = latest_cwds.cwd
        FROM latest_cwds
        WHERE conversation_sessions.tenant_id = latest_cwds.tenant_id
          AND conversation_sessions.id = latest_cwds.session_id;
        "#,
    )
    .expect("execute backfill statement");

    // 5. 校验断言
    let last_cwd: Option<String> = conn
        .query_row(
            "SELECT last_cwd FROM conversation_sessions WHERE tenant_id = 'default' AND id = 'sess-119'",
            [],
            |r| r.get(0),
        )
        .expect("query last_cwd");
    assert_eq!(
        last_cwd.as_deref(),
        Some("/path/latest"),
        "last_cwd 必须正确回填为最新非空工作目录"
    );

    // 6. 幂等性断言：再次运行 migrate_database 零副作用
    drop(conn);
    migrate_database(&db_path)
        .await
        .expect("idempotent migration 119");

    cleanup_database(&db_path);
}

fn cleanup_database(db_path: &std::path::Path) {
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}

#[tokio::test]
async fn acceptance_spec_126_turn_triad_schema_and_parts_slimming() {
    let db_path = temp_database_path("spec-126-turn-triad");
    migrate_database(&db_path).await.expect("initial migration");

    let conn = Connection::open(&db_path).expect("open database");

    // 1. 验证 PRAGMA table_info
    let turn_cols: Vec<String> = {
        let mut stmt = conn
            .prepare("PRAGMA table_info(conversation_turns)")
            .expect("prepare pragma");
        let rows = stmt
            .query_map([], |row| row.get(1))
            .expect("query map")
            .collect::<Result<Vec<String>, _>>()
            .expect("collect cols");
        rows
    };
    assert_eq!(turn_cols.len(), 17, "conversation_turns 必须严格为 17 列");
    assert!(turn_cols.contains(&"prompt".to_string()));
    assert!(turn_cols.contains(&"process".to_string()));
    assert!(turn_cols.contains(&"response".to_string()));
    assert!(!turn_cols.contains(&"user_text".to_string()));

    let web_turn_cols: Vec<String> = {
        let mut stmt = conn
            .prepare("PRAGMA table_info(web_record_turns)")
            .expect("prepare pragma");
        let rows = stmt
            .query_map([], |row| row.get(1))
            .expect("query map")
            .collect::<Result<Vec<String>, _>>()
            .expect("collect cols");
        rows
    };
    assert_eq!(
        turn_cols, web_turn_cols,
        "conversation_turns 与 web_record_turns 列结构必须 100% 同构对齐"
    );

    let part_cols: Vec<String> = {
        let mut stmt = conn
            .prepare("PRAGMA table_info(conversation_parts)")
            .expect("prepare pragma");
        let rows = stmt
            .query_map([], |row| row.get(1))
            .expect("query map")
            .collect::<Result<Vec<String>, _>>()
            .expect("collect cols");
        rows
    };
    assert_eq!(
        part_cols,
        vec![
            "tenant_id",
            "turn_id",
            "part_index",
            "id_key",
            "kind",
            "content",
            "attrs_json",
            "translated_text"
        ],
        "conversation_parts 必须严格为 8 列且不包含 role, card_kind, metadata_json"
    );

    // 2. 构造历史脏数据并执行迁移自愈测试
    conn.execute(
        "DELETE FROM _sqlx_migrations WHERE CAST(version AS TEXT) >= '202610250001'",
        [],
    )
    .expect("delete migration version");

    conn.execute_batch(
        r#"
        DROP TABLE conversation_parts;
        CREATE TABLE conversation_parts (
            tenant_id       TEXT NOT NULL DEFAULT 'default',
            turn_id         TEXT NOT NULL,
            part_index      INTEGER NOT NULL,
            id_key          BLOB NOT NULL,
            role            TEXT NOT NULL,
            kind            TEXT NOT NULL,
            card_kind       TEXT,
            content         TEXT,
            attrs_json      TEXT,
            metadata_json   TEXT,
            translated_text TEXT,
            UNIQUE (tenant_id, turn_id, part_index)
        );

        DROP TABLE conversation_turns;
        CREATE TABLE conversation_turns (
            tenant_id       TEXT NOT NULL DEFAULT 'default',
            id              TEXT NOT NULL,
            session_id      TEXT NOT NULL,
            external_id     TEXT NOT NULL,
            turn_index      INTEGER NOT NULL,
            user_text       TEXT NOT NULL,
            title           TEXT,
            started_at      TEXT,
            ended_at        TEXT,
            model           TEXT,
            fingerprint     TEXT NOT NULL,
            missing         INTEGER NOT NULL DEFAULT 0,
            imported_at     TEXT NOT NULL,
            question_id     TEXT NOT NULL DEFAULT '',
            question_origin TEXT NOT NULL DEFAULT 'imported',
            PRIMARY KEY (tenant_id, id),
            UNIQUE (tenant_id, session_id, external_id)
        );

        DROP TABLE web_record_turns;
        CREATE TABLE web_record_turns (
            tenant_id       TEXT NOT NULL DEFAULT 'default',
            id              TEXT NOT NULL,
            session_id      TEXT NOT NULL,
            external_id     TEXT NOT NULL,
            turn_index      INTEGER NOT NULL,
            user_text       TEXT NOT NULL,
            title           TEXT,
            started_at      TEXT,
            ended_at        TEXT,
            model           TEXT,
            fingerprint     TEXT NOT NULL,
            missing         INTEGER NOT NULL DEFAULT 0,
            imported_at     TEXT NOT NULL,
            question_id     TEXT NOT NULL DEFAULT '',
            question_origin TEXT NOT NULL DEFAULT 'imported',
            PRIMARY KEY (tenant_id, id),
            UNIQUE (tenant_id, session_id, external_id)
        );
        "#,
    )
    .expect("create pre-migration schema");

    conn.execute(
        "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at)
         VALUES ('default', 'sess-126', 'src-1', 'codex', 'ext-126', 'Test Session', 0, '2026-10-25T00:00:00Z', '2026-10-25T00:00:00Z')",
        [],
    ).expect("insert session");

    conn.execute(
        "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, user_text, fingerprint, imported_at)
         VALUES ('default', 'turn-126', 'sess-126', 't-126', 0, '用户历史提问', 'fp-1', '2026-10-25T00:00:00Z')",
        [],
    ).expect("insert turn");

    let dummy_id_key = [1u8; 8];
    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, turn_id, part_index, id_key, role, kind, card_kind, content, attrs_json, metadata_json, translated_text)
         VALUES ('default', 'turn-126', 0, ?1, 'assistant', 'text', 'codex.answer', '我正在分析环境...', NULL, NULL, NULL)",
        [&dummy_id_key as &[u8]],
    ).expect("insert p0");

    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, turn_id, part_index, id_key, role, kind, card_kind, content, attrs_json, metadata_json, translated_text)
         VALUES ('default', 'turn-126', 1, ?1, 'tool', 'command', 'codex.command', 'npm test', '{\"exec_id\":\"exec_cmd1\",\"exit_code\":0,\"cwd\":\"/app\"}', NULL, NULL)",
        [&dummy_id_key as &[u8]],
    ).expect("insert p1");

    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, turn_id, part_index, id_key, role, kind, card_kind, content, attrs_json, metadata_json, translated_text)
         VALUES ('default', 'turn-126', 2, ?1, 'tool', 'tool', 'codex.result', NULL, '{\"exec_id\":\"exec_cmd1\",\"exit_code\":1,\"status\":\"failed\"}', NULL, NULL)",
        [&dummy_id_key as &[u8]],
    ).expect("insert p2");

    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, turn_id, part_index, id_key, role, kind, card_kind, content, attrs_json, metadata_json, translated_text)
         VALUES ('default', 'turn-126', 3, ?1, 'user', 'text', 'codex.user', '用户历史提问', NULL, NULL, NULL)",
        [&dummy_id_key as &[u8]],
    ).expect("insert p3");

    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, turn_id, part_index, id_key, role, kind, card_kind, content, attrs_json, metadata_json, translated_text)
         VALUES ('default', 'turn-126', 4, ?1, 'tool', 'tool', 'codex.result', '3 tests failed', '{\"exec_id\":\"exec_cmd1\",\"renderer\":\"cmd\",\"semantic_role\":\"error\",\"call_id\":\"c1\"}', NULL, NULL)",
        [&dummy_id_key as &[u8]],
    ).expect("insert p4");

    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, turn_id, part_index, id_key, role, kind, card_kind, content, attrs_json, metadata_json, translated_text)
         VALUES ('default', 'turn-126', 5, ?1, 'assistant', 'code_block', NULL, 'console.log(1)', NULL, '{\"content_card\":{\"kind\":\"codex.code\"}}', NULL)",
        [&dummy_id_key as &[u8]],
    ).expect("insert p5");

    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, turn_id, part_index, id_key, role, kind, card_kind, content, attrs_json, metadata_json, translated_text)
         VALUES ('default', 'turn-126', 6, ?1, 'assistant', 'text', 'codex.answer', '测试已修复。', NULL, NULL, 'Tests fixed.')",
        [&dummy_id_key as &[u8]],
    ).expect("insert p6");

    drop(conn);

    // 3. 执行迁移
    migrate_database(&db_path).await.expect("run migration 126");

    let conn = Connection::open(&db_path).expect("reopen database");

    // 4. 断言 conversation_turns
    let (prompt, process_str, response): (String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT prompt, process, response FROM conversation_turns WHERE id = 'turn-126'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("query turn");

    assert_eq!(prompt, "用户历史提问");
    assert_eq!(response.as_deref(), Some("测试已修复。"));

    let process_val: serde_json::Value =
        serde_json::from_str(&process_str.expect("process must exist")).unwrap();
    assert_eq!(
        process_val.get("cwd").and_then(|v| v.as_str()),
        Some("/app")
    );
    assert_eq!(
        process_val.get("response_seq").and_then(|v| v.as_i64()),
        Some(6)
    );
    assert_eq!(
        process_val
            .get("response_translated_text")
            .and_then(|v| v.as_str()),
        Some("Tests fixed.")
    );

    let items = process_val
        .get("items")
        .and_then(|v| v.as_array())
        .expect("items array");
    assert_eq!(items.len(), 1);
    let cmd1 = &items[0];
    assert_eq!(cmd1.get("seq").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(
        cmd1.get("content").and_then(|v| v.as_str()),
        Some("npm test")
    );
    // 空壳 .result 的 exit_code 1 与 status failed 回填至命令
    assert_eq!(cmd1.get("exit_code").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(cmd1.get("status").and_then(|v| v.as_str()), Some("failed"));

    // 5. 断言 conversation_parts：命令行(1)、空壳行(2)、user行(3)、尾回答(6) 已全部剔除
    let remaining_indices: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT part_index FROM conversation_parts WHERE turn_id = 'turn-126' ORDER BY part_index ASC")
            .expect("prepare stmt");
        stmt.query_map([], |r| r.get(0))
            .expect("query map")
            .collect::<Result<Vec<i64>, _>>()
            .expect("collect indices")
    };
    assert_eq!(remaining_indices, vec![0, 4, 5]);

    // Part 0: 中途 answer 归位为 reasoning
    let p0_kind: String = conn
        .query_row(
            "SELECT kind FROM conversation_parts WHERE turn_id = 'turn-126' AND part_index = 0",
            [],
            |r| r.get(0),
        )
        .expect("query p0 kind");
    assert_eq!(p0_kind, "codex.reasoning");

    // Part 4: 非空报错 result 带有 cmd_seq: 1，且无 exec_id, renderer, semantic_role
    let p4_attrs_str: String = conn
        .query_row(
            "SELECT attrs_json FROM conversation_parts WHERE turn_id = 'turn-126' AND part_index = 4",
            [],
            |r| r.get(0),
        )
        .expect("query p4 attrs");
    let p4_attrs: serde_json::Value = serde_json::from_str(&p4_attrs_str).unwrap();
    assert_eq!(p4_attrs.get("cmd_seq").and_then(|v| v.as_i64()), Some(1));
    assert!(p4_attrs.get("exec_id").is_none());
    assert!(p4_attrs.get("call_id").is_none());
    assert!(p4_attrs.get("renderer").is_none());
    assert!(p4_attrs.get("semantic_role").is_none());

    // Part 5: card_kind IS NULL 自愈为 codex.code
    let p5_kind: String = conn
        .query_row(
            "SELECT kind FROM conversation_parts WHERE turn_id = 'turn-126' AND part_index = 5",
            [],
            |r| r.get(0),
        )
        .expect("query p5 kind");
    assert_eq!(p5_kind, "codex.code");

    // 6. 幂等性断言：再次运行 migrate_database 零副作用
    drop(conn);
    migrate_database(&db_path)
        .await
        .expect("idempotent migration");

    cleanup_database(&db_path);
}
