use super::*;
use rusqlite::Connection;
use sha2::{Digest, Sha384};
use uuid::Uuid;

#[test]
fn released_memory_domain_migration_remains_immutable() {
    let migration = include_str!("../../migrations/20260723000001_memory_domain.sql");
    assert_eq!(
        format!("{:x}", Sha384::digest(migration.as_bytes())),
        "bdc84ed0a2c958b8bcd86ed8ffe364b9cf5954002531ac9e015503bab702ba2850586cacbdfcd291fcdc97b41bc8db8b"
    );
}

#[test]
fn released_conversation_question_contract_migration_remains_immutable() {
    let migration =
        include_str!("../../migrations/20260825000005_rebuild_conversation_questions.sql");
    assert_eq!(
        format!("{:x}", Sha384::digest(migration.as_bytes())),
        "85bfcaa1edbb892e90fb943086a77885e6a6c7d349106049ca71a43f9655a60682b18a9d69cce576eea3ccdcee1e0cf2"
    );
}

#[test]
fn released_session_memory_durable_jobs_migration_remains_immutable() {
    let migration = include_str!("../../migrations/20260831000004_session_memory_durable_jobs.sql");
    assert_eq!(
        format!("{:x}", Sha384::digest(migration.as_bytes())),
        "d786fd5e32cc28a7864c23b44c332452a50d78c7fd78ae4cc1958105ac710b0b5a7bbbd5db6a57f69f0510ceaeaf91d4"
    );
}

#[test]
fn session_snapshot_positional_turn_identity_migration_remains_immutable() {
    let migration = include_str!(
        "../../migrations/20261106000001_session_snapshot_positional_turn_identity.sql"
    );
    assert_eq!(
        format!("{:x}", Sha384::digest(migration.as_bytes())),
        "3687dfbe3a28a580281f87cc5728a634fa5cf18ae9e31d8b9ecd0f5f376ecb31b66c4544d8a783fe43d6d1bea0b49371"
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
            "SELECT lower(hex(checksum)) FROM _sqlx_migrations WHERE version = 20260723000001",
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
        "conversation_question_redirects",
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
    let expected_turn_cols = vec![
        "tenant_id",
        "id",
        "session_id",
        "turn_index",
        "prompt",
        "process",
        "response",
        "started_at",
        "ended_at",
        "model",
        "imported_at",
    ];
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
        assert_eq!(columns.len(), 11, "{table} 必须严格为 11 列终态结构");
        assert_eq!(
            columns, expected_turn_cols,
            "{table} 列名与顺序必须严格与 11 列终态一致"
        );
        assert!(!columns.contains(&"title".to_string()));
        assert!(!columns.contains(&"turn_kind".to_string()));
        assert!(!columns.contains(&"agent_role".to_string()));
        assert!(!columns.contains(&"parent_turn_id".to_string()));
        assert!(!columns.contains(&"question_id".to_string()));
        assert!(!columns.contains(&"question_origin".to_string()));
        assert!(!columns.contains(&"external_id".to_string()));
        assert!(!columns.contains(&"fingerprint".to_string()));
        assert!(!columns.contains(&"missing".to_string()));
    }
    let index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN (
                'idx_conversation_turns_tenant_session_question',
                'idx_conversation_turns_tenant_question',
                'idx_web_record_turns_tenant_session_question',
                'idx_web_record_turns_tenant_question'
            )",
            [],
            |row| row.get(0),
        )
        .expect("query question indexes");
    assert_eq!(index_count, 0, "所有旧版 question 相关索引必须已全部清除");

    let session_idx_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN (
                'idx_conversation_turns_tenant_session',
                'idx_web_record_turns_tenant_session'
            )",
            [],
            |row| row.get(0),
        )
        .expect("query session indexes");
    assert_eq!(
        session_idx_count, 2,
        "轮次表上的 tenant_session 索引必须保留"
    );

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
                tenant_id, id, session_id, turn_index, prompt, user_text,
                started_at, ended_at, imported_at
            ) VALUES (
                'default', 'legacy-turn', 'legacy-session', 0,
                'Legacy question snapshot', 'Legacy question snapshot',
                '2026-08-25T00:00:00Z', NULL, '2026-08-25T00:00:00Z'
            );
            INSERT INTO conversation_question_turns (
                tenant_id, question_id, turn_id, turn_order, assignment_origin,
                assigned_at, updated_at
            ) VALUES (
                'default', 'legacy-question', 'legacy-turn', 0, 'auto_merged',
                '2026-08-25T00:00:00Z', '2026-08-25T00:00:00Z'
            );
            INSERT INTO web_record_turns (
                tenant_id, id, session_id, turn_index, prompt, user_text,
                started_at, ended_at, imported_at
            ) VALUES (
                'default', 'legacy-web-turn', 'legacy-web-session', 0,
                'Legacy web question snapshot', 'Legacy web question snapshot',
                '2026-08-25T00:00:00Z', NULL, '2026-08-25T00:00:00Z'
            );
            INSERT INTO web_record_question_turns (
                tenant_id, question_id, turn_id, turn_order, assignment_origin,
                assigned_at, updated_at
            ) VALUES (
                'default', 'legacy-web-question', 'legacy-web-turn', 0, 'imported',
                '2026-08-25T00:00:00Z', '2026-08-25T00:00:00Z'
            );
            DELETE FROM _sqlx_migrations
            WHERE version IN (20260825000004, 20260825000005, 20260826000001);
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

#[test]
fn all_migration_files_have_14_digit_timestamps() {
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let entries = std::fs::read_dir(&migrations_dir).expect("read migrations directory");
    let mut count = 0;
    for entry in entries {
        let entry = entry.expect("valid entry");
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("sql") {
            let filename = path.file_name().unwrap().to_str().unwrap();
            assert!(
                filename.len() > 15,
                "migration file name too short: {filename}"
            );
            let prefix = &filename[..14];
            assert!(
                prefix.chars().all(|c| c.is_ascii_digit()),
                "migration file prefix must be 14 ASCII digits: {filename}"
            );
            assert_eq!(
                filename.chars().nth(14),
                Some('_'),
                "migration filename must have '_' delimiter at index 14: {filename}"
            );
            count += 1;
        }
    }
    assert!(
        count >= 85,
        "expected at least 85 migration files, found {count}"
    );
}

#[tokio::test]
async fn migrations_self_heal_12_digit_legacy_ledger_versions() {
    let db_path = temp_database_path("self-heal-legacy-versions");
    migrate_database(&db_path)
        .await
        .expect("create current 14-digit migrated database");

    // 1. 模拟存量 12 位历史版本号记录（将已应用的 14 位版本号退行更新为 12 位格式）
    let conn = Connection::open(&db_path).expect("open database to simulate legacy ledger");
    conn.execute_batch(
        r#"
        UPDATE _sqlx_migrations
        SET version = (version / 1000000) * 10000 + (version % 10000)
        WHERE version < 20261028000000
          AND version NOT IN (20260909000001, 20260909000002);
        "#,
    )
    .expect("simulate legacy 12-digit versions in _sqlx_migrations");

    let legacy_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version < 10000000000000",
            [],
            |row| row.get(0),
        )
        .expect("count legacy versions");
    assert_eq!(
        legacy_count, 76,
        "expected exactly 76 simulated legacy 12-digit versions"
    );
    drop(conn);

    // 2. 再次执行 migrate_database，触发 open_migrated_pool 中的 repair_legacy_migration_ledger_versions
    migrate_database(&db_path)
        .await
        .expect("self-heal legacy ledger versions and complete migration");

    // 3. 断言校验：所有存量版本均已升级为 14 位，无遗留 12 位版本，总数匹配 MIGRATOR
    let conn = Connection::open(&db_path).expect("open upgraded database");
    let remaining_legacy: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version < 10000000000000",
            [],
            |row| row.get(0),
        )
        .expect("query remaining legacy versions");
    assert_eq!(remaining_legacy, 0, "no legacy versions should remain");

    let total_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM _sqlx_migrations", [], |row| {
            row.get(0)
        })
        .expect("query total migration count");
    assert_eq!(total_count, MIGRATOR.migrations.len() as i64);

    // 验证特定抽样版本已恢复为 14 位且保留 success=1
    for version in [
        20260618000001i64,
        20260723000001i64,
        20260825000005i64,
        20261026000002i64,
    ] {
        let success: bool = conn
            .query_row(
                "SELECT success FROM _sqlx_migrations WHERE version = ?",
                [version],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| panic!("version {version} should exist in ledger"));
        assert!(success, "migration version {version} should be successful");
    }
    drop(conn);

    // 4. 幂等性验证：重复调用 migrate_database 仍平滑成功
    migrate_database(&db_path)
        .await
        .expect("idempotent migration invocation on self-healed database");

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
            WHERE version = 20260825000005;
            DELETE FROM _sqlx_migrations WHERE version = 20260826000001;
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
            "SELECT lower(hex(checksum)) FROM _sqlx_migrations WHERE version = 20260825000005",
            [],
            |row| row.get(0),
        )
        .expect("query repaired question contract checksum");
    let repair_migration_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 20260826000001",
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
    assert_eq!(dirty_index_count, 0);
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
            INSERT INTO conversation_adapters (tenant_id, id, name, kind, version, enabled, trust_state, capabilities, input_kinds, created_at, updated_at) 
            VALUES ('default', 'legacy-adp', 'Test', 'cli', '1.0.0', 1, 'trusted', '[]', '[]', '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z');
            INSERT INTO conversation_sources (tenant_id, id, adapter_id, name, kind, location, enabled, created_at, updated_at)
            VALUES ('default', 'legacy-source', 'legacy-adp', 'Test Source', 'cli', '/path', 1, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z');
            ALTER TABLE conversation_sources DROP COLUMN synced_adapter_hash;
            ALTER TABLE conversation_sources DROP COLUMN synced_card_contract_version;
            ALTER TABLE conversation_sources DROP COLUMN synced_payload_policy_version;
            ALTER TABLE conversation_sources DROP COLUMN synced_projection_version;
            DROP TABLE conversation_session_observations;
            CREATE TABLE conversation_session_observations (
                tenant_id TEXT NOT NULL,
                source_id TEXT NOT NULL,
                record_kind TEXT NOT NULL CHECK (record_kind IN ('session', 'web')),
                external_id TEXT NOT NULL,
                observed_version TEXT NOT NULL,
                hydrated_version TEXT,
                last_seen_at TEXT NOT NULL,
                dirty INTEGER NOT NULL DEFAULT 0,
                hydrated_adapter_hash TEXT,
                hydrated_card_contract_version INTEGER,
                hydrated_payload_policy_version INTEGER,
                hydrated_projection_version INTEGER,
                PRIMARY KEY (tenant_id, source_id, record_kind, external_id)
            );
            INSERT INTO conversation_session_observations (
                tenant_id, source_id, record_kind, external_id, observed_version,
                hydrated_version, last_seen_at, dirty,
                hydrated_adapter_hash, hydrated_card_contract_version,
                hydrated_payload_policy_version, hydrated_projection_version
            ) VALUES (
                'default', 'legacy-source', 'session', 'legacy-external', 'observed-v2',
                'hydrated-v1', '2026-10-01T00:00:00Z', 0,
                'adapter-hash-v1', 7, 3, 5
            );
            DELETE FROM _sqlx_migrations WHERE version = 20261028000001;
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
    let synced_version: String = conn
        .query_row(
            r#"
            SELECT synced_version
            FROM conversation_session_observations
            WHERE tenant_id = 'default' AND source_id = 'legacy-source'
              AND record_kind = 'session' AND external_id = 'legacy-external'
            "#,
            [],
            |row| row.get(0),
        )
        .expect("query rebuilt observation row");
    assert_eq!(synced_version, "hydrated-v1");

    let (hash, card_ver, policy_ver, proj_ver): (
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    ) = conn
        .query_row(
            r#"
            SELECT synced_adapter_hash, synced_card_contract_version,
                   synced_payload_policy_version, synced_projection_version
            FROM conversation_sources
            WHERE tenant_id = 'default' AND id = 'legacy-source'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("query source contract");
    assert_eq!(hash.as_deref(), Some("adapter-hash-v1"));
    assert_eq!(card_ver, Some(7));
    assert_eq!(policy_ver, Some(3));
    assert_eq!(proj_ver, Some(5));

    cleanup_database(&db_path);
}

fn expected_conversation_session_observation_columns() -> Vec<&'static str> {
    vec![
        "tenant_id",
        "source_id",
        "record_kind",
        "external_id",
        "synced_version",
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
async fn migrations_drop_turn_question_columns_and_redirects_preserves_data() {
    let db_path = temp_database_path("drop-turn-question-columns");
    migrate_database(&db_path)
        .await
        .expect("initial database creation");

    let conn = Connection::open(&db_path).expect("open database");

    // 1. 模拟迁移 20261026000001 执行前的 17 列 Schema 与跳转表
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;
        CREATE TABLE IF NOT EXISTS conversation_question_redirects (
            tenant_id TEXT NOT NULL DEFAULT 'default',
            source_question_id TEXT NOT NULL,
            target_question_id TEXT NOT NULL,
            operation_kind TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (tenant_id, source_question_id)
        );
        INSERT INTO conversation_question_redirects (
            tenant_id, source_question_id, target_question_id, operation_kind, created_at, updated_at
        ) VALUES (
            'default', 'q-legacy-src', 'q-legacy-target', 'merge', '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z'
        );

        CREATE TABLE conversation_turns_pre (
            tenant_id       TEXT NOT NULL DEFAULT 'default',
            id              TEXT NOT NULL,
            session_id      TEXT NOT NULL,
            external_id     TEXT NOT NULL,
            turn_index      INTEGER NOT NULL,
            prompt          TEXT NOT NULL DEFAULT '',
            process         TEXT,
            response        TEXT,
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
        INSERT INTO conversation_turns_pre (
            tenant_id, id, session_id, external_id, turn_index,
            prompt, process, response, title, started_at, ended_at,
            model, fingerprint, missing, imported_at, question_id, question_origin
        ) VALUES (
            'default', 'turn-preserve-1', 'session-preserve-1', 'ext-1', 0,
            'Hello AI', '{"items":[]}', 'Hello User', 'Turn 1',
            '2026-10-25T10:00:00Z', '2026-10-25T10:01:00Z', 'gpt-4o',
            'fp-preserve-1', 0, '2026-10-25T10:01:00Z', 'q-obsolete-1', 'imported'
        );
        DROP TABLE conversation_turns;
        ALTER TABLE conversation_turns_pre RENAME TO conversation_turns;
        CREATE INDEX idx_conversation_turns_tenant_session_question
        ON conversation_turns (tenant_id, session_id, question_id, turn_index);
        CREATE INDEX idx_conversation_turns_tenant_question
        ON conversation_turns (tenant_id, question_id);

        CREATE TABLE web_record_turns_pre (
            tenant_id       TEXT NOT NULL DEFAULT 'default',
            id              TEXT NOT NULL,
            session_id      TEXT NOT NULL,
            external_id     TEXT NOT NULL,
            turn_index      INTEGER NOT NULL,
            prompt          TEXT NOT NULL DEFAULT '',
            process         TEXT,
            response        TEXT,
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
        INSERT INTO web_record_turns_pre (
            tenant_id, id, session_id, external_id, turn_index,
            prompt, process, response, title, started_at, ended_at,
            model, fingerprint, missing, imported_at, question_id, question_origin
        ) VALUES (
            'default', 'web-turn-preserve-1', 'web-session-preserve-1', 'web-ext-1', 0,
            'Web prompt', NULL, 'Web response', 'Web Turn 1',
            '2026-10-25T11:00:00Z', '2026-10-25T11:01:00Z', 'claude-3-5',
            'web-fp-1', 0, '2026-10-25T11:01:00Z', 'wq-obsolete-1', 'imported'
        );
        DROP TABLE web_record_turns;
        ALTER TABLE web_record_turns_pre RENAME TO web_record_turns;
        CREATE INDEX idx_web_record_turns_tenant_session_question
        ON web_record_turns (tenant_id, session_id, question_id, turn_index);
        CREATE INDEX idx_web_record_turns_tenant_question
        ON web_record_turns (tenant_id, question_id);

        DELETE FROM _sqlx_migrations WHERE version = 20261026000001;
        PRAGMA foreign_keys = ON;
        "#,
    )
    .expect("setup pre-migration state");
    drop(conn);

    // 2. 运行 20261026000001 迁移
    migrate_database(&db_path)
        .await
        .expect("run slimming migration");

    // 3. 校验终态 Schema 与数据无损
    let conn = Connection::open(&db_path).expect("open migrated database");

    // A. PRAGMA 15 列同构校验
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
        assert_eq!(columns.len(), 15, "{table} 必须严格为 15 列");
        assert!(!columns.contains(&"question_id".to_string()));
        assert!(!columns.contains(&"question_origin".to_string()));
    }

    // B. 跳转表已被清除
    let redirect_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'conversation_question_redirects'",
            [],
            |row| row.get(0),
        )
        .expect("query redirect table");
    assert_eq!(
        redirect_count, 0,
        "conversation_question_redirects 表已被 DROP"
    );

    // C. 3 个 question 相关索引均已清除
    let question_idx_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN (
                'idx_conversation_turns_tenant_session_question',
                'idx_conversation_turns_tenant_question',
                'idx_web_record_turns_tenant_session_question',
                'idx_web_record_turns_tenant_question'
            )",
            [],
            |row| row.get(0),
        )
        .expect("query question indexes");
    assert_eq!(question_idx_count, 0);

    // D. 轮次数据 15 列完全无损
    let turn_row = conn
        .query_row(
            "SELECT tenant_id, id, session_id, external_id, turn_index, prompt, process, response, title, started_at, ended_at, model, fingerprint, missing, imported_at FROM conversation_turns WHERE id = 'turn-preserve-1'",
            [],
            |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, Option<String>>(9)?,
                r.get::<_, Option<String>>(10)?,
                r.get::<_, Option<String>>(11)?,
                r.get::<_, String>(12)?,
                r.get::<_, i64>(13)?,
                r.get::<_, String>(14)?,
            )),
        )
        .expect("query conversation turn");
    assert_eq!(turn_row.0, "default");
    assert_eq!(turn_row.1, "turn-preserve-1");
    assert_eq!(turn_row.2, "session-preserve-1");
    assert_eq!(turn_row.3, "ext-1");
    assert_eq!(turn_row.4, 0);
    assert_eq!(turn_row.5, "Hello AI");
    assert_eq!(turn_row.6.as_deref(), Some("{\"items\":[]}"));
    assert_eq!(turn_row.7.as_deref(), Some("Hello User"));
    assert_eq!(turn_row.8.as_deref(), Some("Turn 1"));
    assert_eq!(turn_row.9.as_deref(), Some("2026-10-25T10:00:00Z"));
    assert_eq!(turn_row.10.as_deref(), Some("2026-10-25T10:01:00Z"));
    assert_eq!(turn_row.11.as_deref(), Some("gpt-4o"));
    assert_eq!(turn_row.12, "fp-preserve-1");
    assert_eq!(turn_row.13, 0);
    assert_eq!(turn_row.14, "2026-10-25T10:01:00Z");

    let web_turn_row = conn
        .query_row(
            "SELECT tenant_id, id, session_id, external_id, turn_index, prompt, process, response, title, started_at, ended_at, model, fingerprint, missing, imported_at FROM web_record_turns WHERE id = 'web-turn-preserve-1'",
            [],
            |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, Option<String>>(9)?,
                r.get::<_, Option<String>>(10)?,
                r.get::<_, Option<String>>(11)?,
                r.get::<_, String>(12)?,
                r.get::<_, i64>(13)?,
                r.get::<_, String>(14)?,
            )),
        )
        .expect("query web turn");
    assert_eq!(web_turn_row.0, "default");
    assert_eq!(web_turn_row.1, "web-turn-preserve-1");
    assert_eq!(web_turn_row.2, "web-session-preserve-1");
    assert_eq!(web_turn_row.3, "web-ext-1");
    assert_eq!(web_turn_row.4, 0);
    assert_eq!(web_turn_row.5, "Web prompt");
    assert_eq!(web_turn_row.6, None);
    assert_eq!(web_turn_row.7.as_deref(), Some("Web response"));
    assert_eq!(web_turn_row.8.as_deref(), Some("Web Turn 1"));
    assert_eq!(web_turn_row.9.as_deref(), Some("2026-10-25T11:00:00Z"));
    assert_eq!(web_turn_row.10.as_deref(), Some("2026-10-25T11:01:00Z"));
    assert_eq!(web_turn_row.11.as_deref(), Some("claude-3-5"));
    assert_eq!(web_turn_row.12, "web-fp-1");
    assert_eq!(web_turn_row.13, 0);
    assert_eq!(web_turn_row.14, "2026-10-25T11:01:00Z");

    cleanup_database(&db_path);
}

/// assets 表历史记录存的是扫描当刻的主机绝对路径，换 OS 或换用户名后必然解析失败。
/// 启动阶段必须把它收敛为便携锚点，且锚点要能展开回当前主机上的真实路径。
#[tokio::test]
async fn legacy_host_absolute_asset_paths_converge_to_portable_anchors() {
    let db_path = temp_database_path("asset-portable-stored-path");
    migrate_database(&db_path).await.expect("run migrations");

    let home = dirs::home_dir().expect("resolve home directory");
    let legacy_path = home.join("legacy-assets").join("skill-a");
    let legacy_text = legacy_path.to_string_lossy().to_string();
    {
        let conn = Connection::open(&db_path).expect("open migrated database");
        conn.execute(
            "INSERT INTO assets (
                tenant_id, id, source_id, name, kind, format, relative_path, stored_path,
                entry_file, description, content_hash, discovered_at, updated_at
            ) VALUES ('default', 'asset-a', 'source-a', 'skill-a', 'skill', 'directory', 'skill-a',
                ?1, 'skill-a/SKILL.md', NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [legacy_text.as_str()],
        )
        .expect("insert legacy asset row");
    }

    let pool = open_migrated_pool(&db_path)
        .await
        .expect("reopen database to converge stored paths");
    let stored: String = sqlx::query_scalar("SELECT stored_path FROM assets WHERE id = 'asset-a'")
        .fetch_one(&pool)
        .await
        .expect("query converged stored path");
    pool.close().await;

    assert_eq!(stored, "~/legacy-assets/skill-a");
    let expanded = crate::backend::infrastructure::path_utils::expand_path(&stored)
        .expect("expand portable anchor");
    assert_eq!(expanded, legacy_path);

    cleanup_database(&db_path);
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
                tenant_id, id, session_id, turn_index, prompt,
                started_at, ended_at, imported_at
            ) VALUES (
                'default', 'part-index-turn', 'part-index-session',
                0, 'Part index question', '2026-10-23T00:00:00Z', NULL,
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
                tenant_id, id, session_id, turn_index, prompt,
                started_at, ended_at, imported_at
            ) VALUES (
                'default', 'part-index-web-turn', 'part-index-web-session',
                0, 'Part index web question', '2026-10-23T00:00:00Z', NULL,
                '2026-10-23T00:00:00Z'
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
            ALTER TABLE conversation_turns ADD COLUMN question_id TEXT NOT NULL DEFAULT '';
            ALTER TABLE conversation_turns ADD COLUMN question_origin TEXT NOT NULL DEFAULT 'imported';
            ALTER TABLE web_record_turns ADD COLUMN question_id TEXT NOT NULL DEFAULT '';
            ALTER TABLE web_record_turns ADD COLUMN question_origin TEXT NOT NULL DEFAULT 'imported';
            CREATE TABLE IF NOT EXISTS conversation_question_redirects (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                source_question_id TEXT NOT NULL,
                target_question_id TEXT NOT NULL,
                operation_kind TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (tenant_id, source_question_id)
            );
            DELETE FROM _sqlx_migrations WHERE version IN (20261023000006, 20261023000007);
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

#[tokio::test]
async fn production_code_has_zero_turn_question_remnants() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("locate repo root");

    let prod_dirs = [
        repo_root.join("backend/asset_core/src"),
        repo_root.join("backend/agent_engine/src"),
        repo_root.join("backend/cli/src"),
        repo_root.join("desktop/src"),
        repo_root.join("frontend/src"),
    ];

    let mut forbidden_hits = Vec::new();

    fn scan_dir(dir: &std::path::Path, forbidden_hits: &mut Vec<String>) {
        if !dir.exists() {
            return;
        }
        for entry in std::fs::read_dir(dir).expect("read dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                scan_dir(&path, forbidden_hits);
            } else if path.is_file() {
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                // 排除测试代码与夹具文件
                if file_name.ends_with("_test.rs")
                    || file_name.ends_with("_tests.rs")
                    || file_name.ends_with(".test.ts")
                    || file_name.ends_with(".spec.ts")
                    || path.components().any(|c| c.as_os_str() == "tests")
                {
                    continue;
                }
                // 只扫描源码文件：.rs, .ts, .tsx, .js
                if !file_name.ends_with(".rs")
                    && !file_name.ends_with(".ts")
                    && !file_name.ends_with(".tsx")
                    && !file_name.ends_with(".js")
                {
                    continue;
                }
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                for (line_idx, line) in content.lines().enumerate() {
                    let line_num = line_idx + 1;
                    let trimmed = line.trim();
                    if trimmed.starts_with("//")
                        || trimmed.starts_with("/*")
                        || trimmed.starts_with('*')
                    {
                        continue;
                    }
                    if line.contains("question_origin") {
                        forbidden_hits.push(format!(
                            "{}:{}: 出现遗留列名 'question_origin'",
                            path.display(),
                            line_num
                        ));
                    }
                    if line.contains("conversation_question_redirects") {
                        forbidden_hits.push(format!(
                            "{}:{}: 出现遗留跳转表 'conversation_question_redirects'",
                            path.display(),
                            line_num
                        ));
                    }
                    let lower = line.to_lowercase();
                    if (lower.contains("conversation_turns") || lower.contains("web_record_turns"))
                        && lower.contains("question_id")
                    {
                        forbidden_hits.push(format!(
                            "{}:{}: 轮次表 SQL 中引用了 question_id 列: {}",
                            path.display(),
                            line_num,
                            trimmed
                        ));
                    }
                    // 匹配独立的 SQL 表别名引用：t.question_id 或 turns.question_id
                    let has_t_question_id = {
                        let target = "t.question_id";
                        let mut start = 0;
                        let mut found = false;
                        while let Some(idx) = lower[start..].find(target) {
                            let abs_idx = start + idx;
                            let before_is_word = if abs_idx > 0 {
                                let ch = lower[..abs_idx].chars().last().unwrap();
                                ch.is_alphanumeric() || ch == '_'
                            } else {
                                false
                            };
                            if !before_is_word {
                                found = true;
                                break;
                            }
                            start = abs_idx + target.len();
                        }
                        found
                    };
                    if has_t_question_id || lower.contains("turns.question_id") {
                        forbidden_hits.push(format!(
                            "{}:{}: 生产代码中出现 t.question_id / turns.question_id 引用: {}",
                            path.display(),
                            line_num,
                            trimmed
                        ));
                    }
                }
            }
        }
    }

    for dir in &prod_dirs {
        scan_dir(dir, &mut forbidden_hits);
    }

    assert!(
        forbidden_hits.is_empty(),
        "生产代码中存在未清零的 Question 遗留项:\n{}",
        forbidden_hits.join("\n")
    );
}

#[tokio::test]
async fn acceptance_spec_104_conversation_domain_table_inventory() {
    let db_path = temp_database_path("acceptance-table-inventory");
    migrate_database(&db_path).await.expect("migrate database");

    let conn = Connection::open(&db_path).expect("open database");

    // 1. 验证被删除的历史表零残留（涵盖全部 8 张新旧市场分裂表及 16 张历史会话/记忆表）
    let dropped_legacy_tables = [
        "conversation_questions",
        "conversation_question_turns",
        "conversation_question_turn_audits",
        "conversation_part_links",
        "web_record_questions",
        "web_record_question_turns",
        "web_record_part_links",
        "conversation_question_fts",
        "conversation_question_redirects",
        "conversation_sync_runs",
        "conversation_sync_deltas",
        "session_memories",
        "legacy_memory_items",
        "legacy_memory_item_evidence",
        "legacy_memory_item_revisions",
        "memory_maintenance_jobs",
        // ADR-0019 / Issue #159 / #163 零僵尸表闭环：清退全部 8 张历史新旧分裂表
        "agent_installations",
        "conversation_adapter_packages",
        "conversation_adapter_package_versions",
        "conversation_adapter_catalog_releases",
        "app_agent_installations",
        "app_conversation_adapter_packages",
        "app_conversation_adapter_package_versions",
        "app_conversation_adapter_catalog_releases",
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

    // 显式断言全部 8 张历史分裂僵尸表在数据库中彻底绝迹
    let legacy_market_zombie_tables = [
        "agent_installations",
        "conversation_adapter_packages",
        "conversation_adapter_package_versions",
        "conversation_adapter_catalog_releases",
        "app_agent_installations",
        "app_conversation_adapter_packages",
        "app_conversation_adapter_package_versions",
        "app_conversation_adapter_catalog_releases",
    ];
    for table in legacy_market_zombie_tables {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |r| r.get(0),
            )
            .expect("query legacy market table");
        assert_eq!(
            count, 0,
            "8 大废弃市场表之一 {table} 必须在数据库中完全绝迹"
        );
    }

    // 2. 检查会话、记录、Agent 与应用域核心业务表（覆盖 conversation_%, web_record_%, agent_%, app_%）
    // 排除 FTS5 内部 shadow tables (conversation_part_fts_%)
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master 
             WHERE (name LIKE 'conversation_%' 
                 OR name LIKE 'web_record_%' 
                 OR name LIKE 'agent_%' 
                 OR name LIKE 'app_%')
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

    // 规格 #104 与 ADR-0019 定义的 22 张核心表收敛白名单
    let expected_core_tables = [
        "agent_execution_bindings",
        "app_extension_catalog_releases",
        "app_extension_versions",
        "app_extensions",
        "app_settings",
        "app_shortcut_items",
        "conversation_adapters",
        "conversation_data_audit_issues",
        "conversation_part_fts",
        "conversation_part_index_state",
        "conversation_parts",
        "conversation_payload_policy_state",
        "conversation_search_index_state",
        "conversation_session_observations",
        "conversation_sessions",
        "conversation_sources",
        "conversation_turns",
        "conversation_usage_events",
        "conversation_usage_source_states",
        "web_record_parts",
        "web_record_sessions",
        "web_record_turns",
    ];

    assert_eq!(
        table_names,
        expected_core_tables
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        "4 大前缀表清单必须与收敛核心白名单（22 表）完全一致，杜绝任何僵尸表残留"
    );

    let market_index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN (
                'idx_app_extensions_capability_target',
                'idx_app_extensions_ready',
                'idx_app_extension_versions_lookup',
                'idx_app_extension_catalog_releases_lookup'
            )",
            [],
            |r| r.get(0),
        )
        .expect("query index count");
    assert_eq!(market_index_count, 4, "统一三表核心索引必须全部存在");

    assert!(
        !table_names.contains(&"conversation_question_redirects".to_string()),
        "conversation_question_redirects 表必须彻底删除"
    );

    // 3. 验证关键列与索引
    for table in ["conversation_turns", "web_record_turns"] {
        let columns: Vec<String> = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .expect("prepare pragma")
            .query_map([], |r| r.get(0))
            .expect("query columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect columns");
        assert_eq!(
            columns.len(),
            11,
            "{table} 必须为 11 列终态 (轮次位置身份，稀疏字段内聚至 process JSON)"
        );
        assert!(!columns.contains(&"title".to_string()));
        assert!(!columns.contains(&"turn_kind".to_string()));
        assert!(!columns.contains(&"agent_role".to_string()));
        assert!(!columns.contains(&"parent_turn_id".to_string()));
        assert!(!columns.contains(&"question_id".to_string()));
        assert!(!columns.contains(&"question_origin".to_string()));
    }

    let question_index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN (
                'idx_conversation_turns_tenant_session_question',
                'idx_conversation_turns_tenant_question',
                'idx_web_record_turns_tenant_session_question',
                'idx_web_record_turns_tenant_question'
            )",
            [],
            |r| r.get(0),
        )
        .expect("query index count");
    assert_eq!(question_index_count, 0, "旧版 question 相关索引必须为 0");

    cleanup_database(&db_path);
}

#[test]
fn zero_zombie_tables() {
    acceptance_spec_104_conversation_domain_table_inventory();
}

#[test]
fn acceptance_spec_104_schema_slimming_drops_deprecated_tables_and_question_columns() {
    acceptance_spec_104_conversation_domain_table_inventory();
}

#[tokio::test]
async fn conversation_adapter_flexible_boolean_deserialization_from_sqlite_integers() {
    let db_path = temp_database_path("flexible-bool-deser");
    let pool = open_migrated_pool(&db_path)
        .await
        .expect("open migrated pool");

    // 1. Insert an extension row with integer runtime_ready = 1 in capability_meta_json
    let meta_json_with_int_bool = serde_json::json!({
        "record_kind": "session",
        "manifest_path": "/fake/manifest.json",
        "adapter_manifest_path": "/fake/adapter.json",
        "runtime_protocol": "acp",
        "runtime_ready": 1, // SQLite integer generated by json_object
        "origin": "managed_release",
        "update_policy": "manual",
        "runtime_gate_status": "ready"
    })
    .to_string();

    sqlx::query(
        r#"
        INSERT INTO app_extensions (
            extension_id, capability_kind, target_id, display_name, active_version,
            distribution_type, install_dir, enabled, installation_status, runtime_status,
            error_code, error_message, checked_at, installed_at, updated_at, capability_meta_json
        ) VALUES (
            'test-pkg-int-bool', 'conversation.adapter', 'test-adapter-int-bool', 'Test Adapter', '1.0.0',
            'archive', '/tmp/install', 1, 'ready', 'ready',
            NULL, NULL, '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', ?1
        )
        "#
    )
    .bind(&meta_json_with_int_bool)
    .execute(&pool)
    .await
    .expect("insert extension with integer bool");

    // Verify loading package correctly deserializes runtime_ready to true without error
    let pkg = crate::store::load_conversation_adapter_package_sqlx(&pool, "test-pkg-int-bool")
        .await
        .expect("load package")
        .expect("package exists");
    assert!(
        pkg.runtime_ready,
        "runtime_ready must deserialize 1 to true"
    );

    // 2. Insert a catalog release with integer breaking_change = 1 in release_meta_json
    let release_meta_with_int_bool = serde_json::json!({
        "core_compatibility": ">=1.0.0",
        "changelog_markdown": "test changes",
        "breaking_change": 1, // SQLite integer generated by json_object
        "runtime_protocol": "acp",
        "record_kind": "session",
        "package_manifest_file": "manifest.json",
        "adapter_manifest_file": "adapter.json"
    })
    .to_string();

    sqlx::query(
        r#"
        INSERT INTO app_extension_catalog_releases (
            catalog_source, extension_id, version, capability_kind, target_id,
            display_name, publisher, channel, released_at, artifact_url,
            artifact_size, artifact_sha256, release_meta_json, fetched_at
        ) VALUES (
            'https://test.catalog/repo', 'test-pkg-int-bool', '1.0.0', 'conversation.adapter', 'test-adapter-int-bool',
            'Test Adapter', 'Publisher', 'stable', '2026-10-31T00:00:00Z', 'https://test.catalog/art.zip',
            1024, 'abc123sha', ?1, '2026-10-31T00:00:00Z'
        )
        "#
    )
    .bind(&release_meta_with_int_bool)
    .execute(&pool)
    .await
    .expect("insert release with integer bool");

    // Verify listing releases correctly deserializes breaking_change to true without error
    let releases = crate::store::list_conversation_adapter_catalog_releases_sqlx(
        &pool,
        "https://test.catalog/repo",
        Some("test-pkg-int-bool"),
    )
    .await
    .expect("list releases");
    assert_eq!(releases.len(), 1);
    assert!(
        releases[0].breaking_change,
        "breaking_change must deserialize 1 to true"
    );

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
        "INSERT INTO conversation_turns (tenant_id, id, session_id, turn_index, prompt, started_at, imported_at) 
         VALUES (?1, ?2, ?3, 0, 'first prompt', '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
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
                "INSERT INTO conversation_turns (tenant_id, id, session_id, turn_index, prompt, started_at, imported_at) 
                 VALUES (?1, ?2, ?3, ?4, 'user prompt', '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
                rusqlite::params![tenant_id, &turn_id, &session_id, t as i64],
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
                "SELECT t.id, t.turn_index, p.part_index, p.content
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
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
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
        "INSERT INTO conversation_turns (tenant_id, id, session_id, turn_index, prompt, started_at, imported_at)
         VALUES ('default', 'turn-default-1', 'sess-default-1', 0, 'prompt', '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
        [],
    ).expect("insert default turn");

    conn.execute(
        "INSERT INTO conversation_turns (tenant_id, id, session_id, turn_index, prompt, started_at, imported_at)
         VALUES ('util6', 'turn-util6-1', 'sess-util6-1', 0, 'prompt', '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
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
        ALTER TABLE conversation_turns ADD COLUMN question_id TEXT NOT NULL DEFAULT '';
        ALTER TABLE conversation_turns ADD COLUMN question_origin TEXT NOT NULL DEFAULT 'imported';
        ALTER TABLE web_record_turns ADD COLUMN user_text TEXT DEFAULT '';
        ALTER TABLE web_record_turns ADD COLUMN question_id TEXT NOT NULL DEFAULT '';
        ALTER TABLE web_record_turns ADD COLUMN question_origin TEXT NOT NULL DEFAULT 'imported';
        UPDATE conversation_turns SET user_text = prompt;
        UPDATE web_record_turns SET user_text = prompt;
        CREATE TABLE IF NOT EXISTS conversation_question_redirects (
            tenant_id TEXT NOT NULL,
            source_question_id TEXT NOT NULL,
            target_question_id TEXT NOT NULL,
            operation_kind TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (tenant_id, source_question_id)
        );

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
        "INSERT INTO conversation_session_observations (tenant_id, source_id, record_kind, external_id, synced_version)
         VALUES ('util6', 'src-1', 'session', 'ext-2', '1')",
        [],
    ).expect("insert util6 observation");

    // 3. 回退 20261024000001~20261024000005 迁移记录，模拟从旧版本升级触发迁移
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS conversation_sync_deltas (tenant_id TEXT);
        CREATE TABLE IF NOT EXISTS conversation_sync_runs (tenant_id TEXT);
        CREATE TABLE IF NOT EXISTS session_memory_source_references (tenant_id TEXT);
        CREATE TABLE IF NOT EXISTS memory_extractions (tenant_id TEXT, id TEXT, PRIMARY KEY (tenant_id, id));
        CREATE TABLE IF NOT EXISTS conversation_adapter_packages (tenant_id TEXT);
        CREATE TABLE IF NOT EXISTS conversation_adapter_catalog_releases (tenant_id TEXT);
        DELETE FROM _sqlx_migrations WHERE version IN (20261024000001, 20261024000002, 20261024000003, 20261024000004);
        "#,
    )
    .expect("rollback migration version");
    drop(conn);

    // 4. 运行迁移升级
    migrate_database(&db_path).await.expect("run migration");

    // 5. 校验断言
    let conn = Connection::open(&db_path).expect("reopen migrated db");
    conn.execute_batch(
        r#"
        DROP TABLE IF EXISTS conversation_sync_deltas;
        DROP TABLE IF EXISTS conversation_sync_runs;
        DROP TABLE IF EXISTS session_memory_source_references;
        DROP TABLE IF EXISTS memory_extractions;
        DROP TABLE IF EXISTS conversation_adapter_packages;
        DROP TABLE IF EXISTS conversation_adapter_catalog_releases;
        "#,
    )
    .expect("clean up temp tables");

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

    // D. 轮次主键已重算为位置身份；索引行保留，正文归属与命中不变
    let repositioned_turn_id =
        crate::backend::domain::derive_conversation_turn_id("sess-default-1", 0);
    let stored_turn_id: String = conn
        .query_row(
            "SELECT id FROM conversation_turns WHERE tenant_id = 'default'",
            [],
            |r| r.get(0),
        )
        .expect("query repositioned turn id");
    assert_eq!(
        stored_turn_id, repositioned_turn_id,
        "轮次主键必须重算为 sha256(session_id, turn_index)"
    );
    // 索引表按会话聚合正文，`turn_id` / `part_id` 是 UNINDEXED 且无消费方的派生列，
    // 按它们过滤必然全表扫描，因此启动期修复不动索引行——检索能力不因此受损。
    let indexed_session: String = conn
        .query_row(
            "SELECT session_id FROM conversation_part_fts WHERE tenant_id = 'default'",
            [],
            |r| r.get(0),
        )
        .expect("query fts session id");
    assert_eq!(
        indexed_session, "sess-default-1",
        "索引行必须仍挂在原会话下，正文匹配不受轮次主键变化影响"
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
        "INSERT INTO conversation_turns (tenant_id, id, session_id, turn_index, prompt, started_at, ended_at, imported_at)
         VALUES ('default', 'turn-119-1', 'sess-119', 0, 'first question', '2026-10-24T00:00:00Z', '2026-10-24T00:01:00Z', '2026-10-24T00:00:00Z')",
        [],
    ).expect("insert turn 1");

    conn.execute(
        "INSERT INTO conversation_parts (tenant_id, id, turn_id, part_index, role, kind, cwd, text)
         VALUES ('default', 'part-119-1-0', 'turn-119-1', 0, 'tool', 'tool', '/path/old', 'running')",
        [],
    ).expect("insert part 1-0");

    // 最新轮次：包含两个零件，最后一个有效 cwd 为 /path/latest
    conn.execute(
        "INSERT INTO conversation_turns (tenant_id, id, session_id, turn_index, prompt, started_at, ended_at, imported_at)
         VALUES ('default', 'turn-119-2', 'sess-119', 1, 'second question', '2026-10-24T00:02:00Z', '2026-10-24T00:03:00Z', '2026-10-24T00:02:00Z')",
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

    // 4. 执行回填逻辑（与迁移 20261024000002 一致的 CTE UPDATE FROM）
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
    assert_eq!(
        turn_cols.len(),
        11,
        "conversation_turns 必须严格为 11 列终态结构"
    );
    assert!(turn_cols.contains(&"prompt".to_string()));
    assert!(turn_cols.contains(&"process".to_string()));
    assert!(turn_cols.contains(&"response".to_string()));
    assert!(!turn_cols.contains(&"title".to_string()));
    assert!(!turn_cols.contains(&"turn_kind".to_string()));
    assert!(!turn_cols.contains(&"agent_role".to_string()));
    assert!(!turn_cols.contains(&"parent_turn_id".to_string()));
    assert!(!turn_cols.contains(&"user_text".to_string()));
    assert!(!turn_cols.contains(&"question_id".to_string()));
    assert!(!turn_cols.contains(&"question_origin".to_string()));

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
        "DELETE FROM _sqlx_migrations WHERE version = 20261025000001",
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

#[tokio::test]
async fn acceptance_spec_157_turn_sparse_columns_convergence_and_14_col_slimming() {
    let db_path = temp_database_path("spec-157-sparse-turns-convergence");
    migrate_database(&db_path).await.expect("initial migration");

    let conn = Connection::open(&db_path).expect("open database");

    // 1. 验证 PRAGMA table_info 为 11 列纯净终态
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
    assert_eq!(
        turn_cols.len(),
        11,
        "conversation_turns 必须严格为 11 列终态结构"
    );
    assert!(!turn_cols.contains(&"title".to_string()));
    assert!(!turn_cols.contains(&"turn_kind".to_string()));
    assert!(!turn_cols.contains(&"agent_role".to_string()));
    assert!(!turn_cols.contains(&"parent_turn_id".to_string()));

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

    // 2. 模拟 18 列存量历史数据场景并验证迁移无损转换
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;
        CREATE TABLE conversation_turns_pre_157 (
            tenant_id       TEXT NOT NULL DEFAULT 'default',
            id              TEXT NOT NULL,
            session_id      TEXT NOT NULL,
            external_id     TEXT NOT NULL,
            turn_index      INTEGER NOT NULL,
            prompt          TEXT NOT NULL DEFAULT '',
            process         TEXT,
            response        TEXT,
            title           TEXT,
            started_at      TEXT,
            ended_at        TEXT,
            model           TEXT,
            fingerprint     TEXT NOT NULL,
            missing         INTEGER NOT NULL DEFAULT 0,
            imported_at     TEXT NOT NULL,
            turn_kind       TEXT,
            agent_role      TEXT,
            parent_turn_id  TEXT,
            PRIMARY KEY (tenant_id, id),
            UNIQUE (tenant_id, session_id, external_id)
        );

        INSERT INTO conversation_turns_pre_157 VALUES
          ('default', 'turn-normal', 'sess-1', 'ext-normal', 0, 'Hi', NULL, 'Hello', NULL, NULL, NULL, 'gpt', 'fp-1', 0, '2026-10-29', NULL, NULL, NULL),
          ('default', 'turn-cont', 'sess-1', 'ext-cont', 1, 'Continue', '{"cwd":"/app"}', 'Done', NULL, NULL, NULL, 'gpt', 'fp-2', 0, '2026-10-29', 'continuation', NULL, NULL),
          ('default', 'turn-sub-cmd', 'sess-1', 'ext-sub-1', 2, 'Do subtask', '{"cwd":"/repo","items":[{"seq":1,"kind":"cmd"}]}', 'Subdone', 'Subtask 1', NULL, NULL, 'gpt', 'fp-3', 0, '2026-10-29', 'subagent', 'auditor', 'turn-normal'),
          ('default', 'turn-sub-txt', 'sess-1', 'ext-sub-2', 3, 'Pure text sub', NULL, 'Text report', 'Pure Text', NULL, NULL, 'gpt', 'fp-4', 0, '2026-10-29', 'subagent', 'reviewer', 'turn-normal');

        DROP TABLE conversation_turns;
        ALTER TABLE conversation_turns_pre_157 RENAME TO conversation_turns;
        CREATE INDEX idx_conversation_turns_tenant_session ON conversation_turns (tenant_id, session_id);

        CREATE TABLE web_record_turns_pre_157 (
            tenant_id       TEXT NOT NULL DEFAULT 'default',
            id              TEXT NOT NULL,
            session_id      TEXT NOT NULL,
            external_id     TEXT NOT NULL,
            turn_index      INTEGER NOT NULL,
            prompt          TEXT NOT NULL DEFAULT '',
            process         TEXT,
            response        TEXT,
            title           TEXT,
            started_at      TEXT,
            ended_at        TEXT,
            model           TEXT,
            fingerprint     TEXT NOT NULL,
            missing         INTEGER NOT NULL DEFAULT 0,
            imported_at     TEXT NOT NULL,
            turn_kind       TEXT,
            agent_role      TEXT,
            parent_turn_id  TEXT,
            PRIMARY KEY (tenant_id, id),
            UNIQUE (tenant_id, session_id, external_id)
        );

        INSERT INTO web_record_turns_pre_157 VALUES
          ('default', 'web-turn-normal', 'web-sess-1', 'web-ext-normal', 0, 'Hi web', NULL, 'Hello web', NULL, NULL, NULL, 'gpt', 'fp-w1', 0, '2026-10-29', NULL, NULL, NULL),
          ('default', 'web-turn-sub', 'web-sess-1', 'web-ext-sub', 1, 'Web subagent', '{"cwd":"/web"}', 'Web done', 'Web Task', NULL, NULL, 'gpt', 'fp-w2', 0, '2026-10-29', 'subagent', 'browser', 'web-turn-normal');

        DROP TABLE web_record_turns;
        ALTER TABLE web_record_turns_pre_157 RENAME TO web_record_turns;
        CREATE INDEX idx_web_record_turns_tenant_session ON web_record_turns (tenant_id, session_id);
        PRAGMA foreign_keys = ON;
        "#,
    ).expect("setup pre-157 18-col table");

    // 运行迁移脚本
    let migration_sql =
        include_str!("../../migrations/20261029000001_converge_turn_sparse_columns_to_process.sql");
    conn.execute_batch(migration_sql)
        .expect("run migration 157");

    // 3. 验证迁移后的行内容
    // 普通轮次：process 保持 NULL，严禁出现空对象 "{}"
    let normal_process: Option<String> = conn
        .query_row(
            "SELECT process FROM conversation_turns WHERE id = 'turn-normal'",
            [],
            |r| r.get(0),
        )
        .expect("query normal process");
    assert_eq!(normal_process, None, "普通轮次 process 必须保持 NULL");

    // 续接轮次：保留原有 cwd，并补齐 turn_kind
    let cont_process_str: String = conn
        .query_row(
            "SELECT process FROM conversation_turns WHERE id = 'turn-cont'",
            [],
            |r| r.get(0),
        )
        .expect("query cont process");
    let cont_val: serde_json::Value = serde_json::from_str(&cont_process_str).unwrap();
    assert_eq!(cont_val.get("cwd").and_then(|v| v.as_str()), Some("/app"));
    assert_eq!(
        cont_val.get("turn_kind").and_then(|v| v.as_str()),
        Some("continuation")
    );
    assert!(cont_val.get("title").is_none());
    assert!(cont_val.get("agent_role").is_none());
    assert!(cont_val.get("parent_turn_id").is_none());

    // 子代理有命令项轮次：原有 items/cwd 与稀疏字段协同保留
    let sub_cmd_str: String = conn
        .query_row(
            "SELECT process FROM conversation_turns WHERE id = 'turn-sub-cmd'",
            [],
            |r| r.get(0),
        )
        .expect("query sub cmd process");
    let sub_cmd_val: serde_json::Value = serde_json::from_str(&sub_cmd_str).unwrap();
    assert_eq!(
        sub_cmd_val.get("cwd").and_then(|v| v.as_str()),
        Some("/repo")
    );
    assert_eq!(
        sub_cmd_val.get("title").and_then(|v| v.as_str()),
        Some("Subtask 1")
    );
    assert_eq!(
        sub_cmd_val.get("turn_kind").and_then(|v| v.as_str()),
        Some("subagent")
    );
    assert_eq!(
        sub_cmd_val.get("agent_role").and_then(|v| v.as_str()),
        Some("auditor")
    );
    assert_eq!(
        sub_cmd_val.get("parent_turn_id").and_then(|v| v.as_str()),
        Some("turn-normal")
    );
    assert_eq!(
        sub_cmd_val
            .get("items")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(1)
    );

    // 纯文本子代理轮次（原本 process 为 NULL）：精简生成包含非空元数据的 process JSON
    let sub_txt_str: String = conn
        .query_row(
            "SELECT process FROM conversation_turns WHERE id = 'turn-sub-txt'",
            [],
            |r| r.get(0),
        )
        .expect("query sub txt process");
    let sub_txt_val: serde_json::Value = serde_json::from_str(&sub_txt_str).unwrap();
    assert_eq!(
        sub_txt_val.get("title").and_then(|v| v.as_str()),
        Some("Pure Text")
    );
    assert_eq!(
        sub_txt_val.get("turn_kind").and_then(|v| v.as_str()),
        Some("subagent")
    );
    assert_eq!(
        sub_txt_val.get("agent_role").and_then(|v| v.as_str()),
        Some("reviewer")
    );
    assert_eq!(
        sub_txt_val.get("parent_turn_id").and_then(|v| v.as_str()),
        Some("turn-normal")
    );
    assert!(sub_txt_val.get("cwd").is_none());
    assert!(sub_txt_val.get("items").is_none());

    // web record 验证：包含旧 title 和子代理字段的行被无损内聚至 process JSON
    let web_sub_str: String = conn
        .query_row(
            "SELECT process FROM web_record_turns WHERE id = 'web-turn-sub'",
            [],
            |r| r.get(0),
        )
        .expect("query web sub process");
    let web_sub_val: serde_json::Value = serde_json::from_str(&web_sub_str).unwrap();
    assert_eq!(
        web_sub_val.get("cwd").and_then(|v| v.as_str()),
        Some("/web")
    );
    assert_eq!(
        web_sub_val.get("title").and_then(|v| v.as_str()),
        Some("Web Task")
    );
    assert_eq!(
        web_sub_val.get("turn_kind").and_then(|v| v.as_str()),
        Some("subagent")
    );
    assert_eq!(
        web_sub_val.get("agent_role").and_then(|v| v.as_str()),
        Some("browser")
    );
    assert_eq!(
        web_sub_val.get("parent_turn_id").and_then(|v| v.as_str()),
        Some("web-turn-normal")
    );

    // web record 普通轮次：process 保持 NULL
    let web_normal_process: Option<String> = conn
        .query_row(
            "SELECT process FROM web_record_turns WHERE id = 'web-turn-normal'",
            [],
            |r| r.get(0),
        )
        .expect("query web normal process");
    assert_eq!(
        web_normal_process, None,
        "Web record 普通轮次 process 必须保持 NULL"
    );

    // 4. 幂等性断言：再次运行 migrate_database 零副作用
    drop(conn);
    migrate_database(&db_path)
        .await
        .expect("idempotent migration");

    cleanup_database(&db_path);
}

// ============================================================================
// Adversarial Penetration Challenge Tests (Milestone 4 / Issue #159 / #163)
// ============================================================================

// 1. Fresh Database vs Incremental Migration & Foreign Key Integrity
#[tokio::test]
async fn adversarial_challenge_1_fresh_and_incremental_migration_foreign_key_integrity() {
    let db_path = temp_database_path("adv-fresh-db-fk");

    // Fresh run: run full migration sequence (1 to 85) on a brand-new SQLite database
    let pool = open_migrated_pool(&db_path)
        .await
        .expect("full migration sequence (1 to 85) on fresh database must succeed");

    // Verify foreign keys are enabled in pool connection
    let fk_enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await
        .expect("query foreign_keys pragma");
    assert_eq!(fk_enabled, 1, "foreign keys must be enabled");

    // Run PRAGMA foreign_key_check to verify ZERO foreign key violations across all tables
    let conn = Connection::open(&db_path).expect("open connection for pragma check");
    let violations = {
        let mut stmt = conn
            .prepare("PRAGMA foreign_key_check")
            .expect("prepare fk check");
        stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .expect("query fk check")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect fk violations")
    };
    assert!(
        violations.is_empty(),
        "database must have 0 foreign key violations after migration, found: {:?}",
        violations
    );

    // Run PRAGMA integrity_check to verify database page and index integrity
    let integrity_result: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .expect("PRAGMA integrity_check");
    assert_eq!(integrity_result, "ok", "database integrity must be ok");

    drop(conn);
    pool.close().await;

    // Incremental / Idempotent test: re-run migrate_database multiple times
    for i in 1..=3 {
        migrate_database(&db_path)
            .await
            .unwrap_or_else(|e| panic!("incremental/idempotent migration pass {i} failed: {e:?}"));
    }

    cleanup_database(&db_path);
}

// 2. Pre-populated Dirty Legacy Data & Foreign Key Resilience
#[tokio::test]
async fn adversarial_challenge_2_prepopulated_dirty_legacy_data_resilience() {
    let db_path = temp_database_path("adv-dirty-legacy-data");

    // Migrate database to establish initial schema
    let pool = open_migrated_pool(&db_path)
        .await
        .expect("open migrated database");

    // Recreate the 3 legacy tables with dirty/orphan/adversarial data to simulate pre-migration state
    let conn = Connection::open(&db_path).expect("open connection for dirty data injection");
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS app_conversation_adapter_packages (
            package_id TEXT PRIMARY KEY,
            adapter_id TEXT NOT NULL,
            name TEXT NOT NULL,
            version TEXT NOT NULL,
            origin TEXT NOT NULL,
            install_dir TEXT,
            manifest_path TEXT NOT NULL,
            adapter_manifest_path TEXT NOT NULL,
            runtime_protocol TEXT NOT NULL,
            runtime_ready INTEGER NOT NULL,
            installed_content_hash TEXT,
            trusted_package_hash TEXT,
            source_url TEXT,
            git_ref TEXT,
            git_commit TEXT,
            catalog_url TEXT,
            update_policy TEXT NOT NULL,
            latest_version TEXT,
            last_checked_at TEXT,
            runtime_gate_status TEXT,
            runtime_validated_at TEXT,
            record_kind TEXT NOT NULL,
            error_message TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS app_conversation_adapter_package_versions (
            package_id TEXT NOT NULL,
            version TEXT NOT NULL,
            install_dir TEXT NOT NULL,
            artifact_hash TEXT,
            content_hash TEXT NOT NULL,
            runtime_gate_status TEXT NOT NULL,
            installed_at TEXT NOT NULL,
            PRIMARY KEY (package_id, version),
            FOREIGN KEY (package_id) REFERENCES app_conversation_adapter_packages(package_id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS app_conversation_adapter_catalog_releases (
            catalog_url TEXT NOT NULL,
            package_id TEXT NOT NULL,
            version TEXT NOT NULL,
            adapter_id TEXT NOT NULL,
            name TEXT NOT NULL,
            publisher TEXT NOT NULL,
            channel TEXT NOT NULL,
            released_at TEXT,
            artifact_url TEXT NOT NULL,
            artifact_size INTEGER,
            artifact_sha256 TEXT,
            core_compatibility TEXT,
            changelog_markdown TEXT,
            breaking_change INTEGER NOT NULL,
            runtime_protocol TEXT NOT NULL,
            record_kind TEXT NOT NULL,
            package_manifest_file TEXT NOT NULL,
            adapter_manifest_file TEXT NOT NULL,
            adapter_manifest_json TEXT,
            source_json TEXT,
            etag TEXT,
            fetched_at TEXT NOT NULL,
            PRIMARY KEY (catalog_url, package_id, version)
        );

        -- Insert clean and adversarial dirty records:
        -- Record 1: Clean standard package
        INSERT INTO app_conversation_adapter_packages VALUES (
            'pkg-clean-1', 'adapter-clean-1', 'Clean Adapter', '1.0.0',
            'local_directory', '/tmp/clean', '/tmp/clean/pkg.json', '/tmp/clean/adapter.json',
            'acp', 1, 'hash1', 'trusted1', NULL, NULL, NULL, 'https://example.com/catalog',
            'manual', '1.0.0', '2026-10-31T00:00:00Z', 'ready', '2026-10-31T00:00:00Z',
            'session', NULL, '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z'
        );

        -- Record 2: Dirty record with NULLs in optional fields, odd origin, non-standard runtime_ready = 0
        INSERT INTO app_conversation_adapter_packages VALUES (
            'pkg-dirty-nulls', 'adapter-dirty-nulls', 'Dirty Nulls Adapter "''` -- DROP TABLE', '0.0.1-alpha.0+build.42',
            'exotic_remote_unknown', NULL, '/tmp/dirty/pkg.json', '/tmp/dirty/adapter.json',
            'acp', 0, NULL, NULL, NULL, NULL, NULL, NULL,
            'auto', NULL, NULL, 'core_incompatible', NULL,
            'agent', 'Failed with: <error line 1\nline 2> 🚀', '2026-10-31T01:00:00Z', '2026-10-31T01:00:00Z'
        );

        -- Record 3: Valid package with versions
        INSERT INTO app_conversation_adapter_packages VALUES (
            'pkg-multi-version', 'adapter-multi-version', 'Multi Version Adapter 🚀 测试', '2.0.0',
            'dev_override', '/tmp/multi', '/tmp/multi/pkg.json', '/tmp/multi/adapter.json',
            'acp', 1, 'hash2', 'trusted2', 'https://github.com/test', 'main', 'abc123', 'https://catalog.org',
            'manual', '2.1.0', '2026-10-31T02:00:00Z', 'ready', '2026-10-31T02:00:00Z',
            'session', NULL, '2026-10-31T02:00:00Z', '2026-10-31T02:00:00Z'
        );

        -- Package versions (including valid versions)
        INSERT INTO app_conversation_adapter_package_versions VALUES
            ('pkg-multi-version', '1.0.0', '/tmp/multi/v1', 'art1', 'cnt1', 'ready', '2026-10-31T02:00:00Z'),
            ('pkg-multi-version', '2.0.0', '/tmp/multi/v2', 'art2', 'cnt2', 'ready', '2026-10-31T02:00:00Z');

        -- Adversarial: Disable FK momentarily to inject ORPHAN version referencing NON-EXISTENT package
        PRAGMA foreign_keys = OFF;
        INSERT INTO app_conversation_adapter_package_versions VALUES
            ('pkg-ghost-nonexistent', '0.9.9', '/tmp/ghost', 'arthash', 'cnthash', 'broken', '2026-10-31T03:00:00Z');
        PRAGMA foreign_keys = ON;

        -- Catalog releases with corrupted/malformed JSON, unicode, and quotes
        INSERT INTO app_conversation_adapter_catalog_releases VALUES (
            'https://catalog.org', 'pkg-multi-version', '2.0.0', 'adapter-multi-version',
            'Multi Version Adapter', 'Test Publisher', 'stable', '2026-10-31T02:00:00Z',
            'https://art.org/v2.tar.gz', 1024, 'sha256-abc', '>=1.0.0',
            '### Changelog\n- Fixes ''quotes'' & "escapes"', 0, 'acp', 'session',
            'pkg.json', 'adapter.json',
            '{"valid": true, "nested": {"key": "value", "arr": [1, 2, 3]}}',
            '{"source": "git"}', 'etag123', '2026-10-31T02:00:00Z'
        );

        -- Release with malformed / corrupted JSON (must be safely handled by json_valid in migration)
        INSERT INTO app_conversation_adapter_catalog_releases VALUES (
            'https://catalog.org', 'pkg-corrupted-json', '1.0.0', 'adapter-corrupted',
            'Corrupted JSON Release', 'Publisher Bad', 'beta', '2026-10-31T04:00:00Z',
            'https://art.org/bad.tar.gz', 2048, 'sha256-bad', NULL,
            NULL, 1, 'acp', 'agent',
            'pkg.json', 'adapter.json',
            '{corrupted unclosed json: [1, 2,',
            'not even json {}}', NULL, '2026-10-31T04:00:00Z'
        );
        "#
    )
    .expect("inject dirty legacy data");
    drop(conn);

    // Now execute migration 20261031000002 logic via Rusqlite connection with PRAGMA foreign_keys = ON
    let migration_sql = include_str!(
        "../../migrations/20261031000002_converge_conversation_adapters_and_drop_legacy_tables.sql"
    );
    let conn = Connection::open(&db_path).expect("open connection to execute migration");
    conn.execute_batch(&format!("PRAGMA foreign_keys = ON;\n{}", migration_sql))
        .expect("migration 20261031000002 must execute successfully against dirty legacy data with FK enabled");

    // Assert:
    // 1. The 3 legacy tables are completely dropped
    for table in [
        "app_conversation_adapter_packages",
        "app_conversation_adapter_package_versions",
        "app_conversation_adapter_catalog_releases",
    ] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |r| r.get(0),
            )
            .expect("check table dropped");
        assert_eq!(count, 0, "legacy table {table} must be dropped");
    }

    // 2. The orphan version ('pkg-ghost-nonexistent') was NOT inserted into app_extension_versions
    let ghost_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM app_extension_versions WHERE extension_id = 'pkg-ghost-nonexistent'",
            [],
            |r| r.get(0),
        )
        .expect("query ghost version count");
    assert_eq!(
        ghost_count, 0,
        "orphan version must not exist in app_extension_versions"
    );

    // 3. Valid versions exist and foreign key check passes
    let version_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM app_extension_versions WHERE extension_id = 'pkg-multi-version'",
            [],
            |r| r.get(0),
        )
        .expect("query valid versions count");
    assert!(
        version_count >= 2,
        "multi-version package must have at least 2 versions"
    );

    // 4. Dirty nulls package was migrated safely with proper fallbacks
    let dirty_ext: (String, String, i64, String, Option<String>) = conn
        .query_row(
            "SELECT capability_kind, distribution_type, enabled, installation_status, error_code FROM app_extensions WHERE extension_id = 'pkg-dirty-nulls'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .expect("query dirty ext");
    assert_eq!(dirty_ext.0, "conversation.adapter");
    assert_eq!(
        dirty_ext.1, "archive",
        "unknown origin must fall back to 'archive'"
    );
    assert_eq!(dirty_ext.2, 0, "runtime_ready = 0 must map to enabled = 0");
    assert_eq!(
        dirty_ext.3, "incompatible",
        "runtime_gate_status = 'core_incompatible' must map to 'incompatible'"
    );
    assert_eq!(dirty_ext.4.as_deref(), Some("core_incompatible"));

    // 5. Corrupted JSON release was handled gracefully: json_valid filtered it out to NULL
    let corrupted_meta: String = conn
        .query_row(
            "SELECT release_meta_json FROM app_extension_catalog_releases WHERE extension_id = 'pkg-corrupted-json'",
            [],
            |r| r.get(0),
        )
        .expect("query corrupted release meta");
    let parsed_meta: serde_json::Value =
        serde_json::from_str(&corrupted_meta).expect("release_meta_json must be valid JSON");
    assert!(
        parsed_meta.get("adapter_manifest_json").unwrap().is_null(),
        "malformed adapter_manifest_json must become null"
    );
    assert!(
        parsed_meta.get("source_json").unwrap().is_null(),
        "malformed source_json must become null"
    );

    // 6. PRAGMA foreign_key_check passes with 0 violations
    let violations = {
        let mut stmt = conn
            .prepare("PRAGMA foreign_key_check")
            .expect("prepare fk check");
        stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .expect("query fk check")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect fk violations")
    };
    assert!(
        violations.is_empty(),
        "0 foreign key violations expected after dirty migration: {:?}",
        violations
    );

    // 7. PRAGMA integrity_check passes
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .expect("integrity check");
    assert_eq!(integrity, "ok");

    drop(conn);
    pool.close().await;
    cleanup_database(&db_path);
}

// 3. Zero Zombie Tables Strictness & Oracle Failure Injection
#[tokio::test]
async fn adversarial_challenge_3_zero_zombie_tables_oracle_failure_injection() {
    let db_path = temp_database_path("adv-zero-zombie-oracle");
    migrate_database(&db_path).await.expect("migrate database");

    // A. Verify baseline clean state: all 8 legacy market tables are 0
    let conn = Connection::open(&db_path).expect("open database");
    for table in [
        "agent_installations",
        "conversation_adapter_packages",
        "conversation_adapter_package_versions",
        "conversation_adapter_catalog_releases",
        "app_agent_installations",
        "app_conversation_adapter_packages",
        "app_conversation_adapter_package_versions",
        "app_conversation_adapter_catalog_releases",
    ] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |r| r.get(0),
            )
            .expect("query legacy market table");
        assert_eq!(count, 0, "legacy table {table} must be 0");
    }

    let clean_tables: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master 
                 WHERE (name LIKE 'conversation_%' 
                     OR name LIKE 'web_record_%' 
                     OR name LIKE 'agent_%' 
                     OR name LIKE 'app_%')
                   AND type IN ('table', 'virtual')
                   AND name NOT LIKE 'conversation_part_fts_%'
                 ORDER BY name",
            )
            .expect("prepare stmt");

        stmt.query_map([], |r| r.get(0))
            .expect("query map")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect table names")
    };
    drop(conn);

    let expected_core_tables = [
        "agent_execution_bindings",
        "app_extension_catalog_releases",
        "app_extension_versions",
        "app_extensions",
        "app_settings",
        "app_shortcut_items",
        "conversation_adapters",
        "conversation_data_audit_issues",
        "conversation_part_fts",
        "conversation_part_index_state",
        "conversation_parts",
        "conversation_payload_policy_state",
        "conversation_search_index_state",
        "conversation_session_observations",
        "conversation_sessions",
        "conversation_sources",
        "conversation_turns",
        "conversation_usage_events",
        "conversation_usage_source_states",
        "web_record_parts",
        "web_record_sessions",
        "web_record_turns",
    ];
    assert_eq!(
        clean_tables,
        expected_core_tables
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        "baseline clean database must match exactly 22 core tables"
    );

    // Helper to test oracle rejection
    let check_table_inventory_rejection = |zombie_table_sql: &str, zombie_table_name: &str| {
        let test_db_path = temp_database_path(&format!("zombie-test-{}", zombie_table_name));
        std::fs::copy(&db_path, &test_db_path).expect("copy base db");
        let test_conn = Connection::open(&test_db_path).expect("open test db");
        test_conn
            .execute_batch(zombie_table_sql)
            .expect("inject zombie table");

        // Verify the zombie table was created
        let count: i64 = test_conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [zombie_table_name],
                |r| r.get(0),
            )
            .expect("check table created");
        assert_eq!(count, 1);

        // Run inventory assertion logic against test_conn
        let table_names: Vec<String> = {
            let mut stmt = test_conn
                .prepare(
                    "SELECT name FROM sqlite_master 
                 WHERE (name LIKE 'conversation_%' 
                     OR name LIKE 'web_record_%' 
                     OR name LIKE 'agent_%' 
                     OR name LIKE 'app_%')
                   AND type IN ('table', 'virtual')
                   AND name NOT LIKE 'conversation_part_fts_%'
                 ORDER BY name",
                )
                .expect("prepare stmt");

            stmt.query_map([], |r| r.get(0))
                .expect("query map")
                .collect::<Result<Vec<_>, _>>()
                .expect("collect table names")
        };

        let expected_core_tables = [
            "agent_execution_bindings",
            "app_extension_catalog_releases",
            "app_extension_versions",
            "app_extensions",
            "app_settings",
            "app_shortcut_items",
            "conversation_adapters",
            "conversation_data_audit_issues",
            "conversation_part_fts",
            "conversation_part_index_state",
            "conversation_parts",
            "conversation_payload_policy_state",
            "conversation_search_index_state",
            "conversation_session_observations",
            "conversation_sessions",
            "conversation_sources",
            "conversation_turns",
            "conversation_usage_events",
            "conversation_usage_source_states",
            "web_record_parts",
            "web_record_sessions",
            "web_record_turns",
        ];

        let oracle_passed = table_names
            == expected_core_tables
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();
        assert!(
            !oracle_passed,
            "Oracle MUST fail when zombie table '{zombie_table_name}' is injected! But it passed unexpectedly."
        );

        drop(test_conn);
        cleanup_database(&test_db_path);
    };

    // Test rejection on each prefix:
    // 1. app_% prefix zombie
    check_table_inventory_rejection(
        "CREATE TABLE app_zombie_ghost (id TEXT PRIMARY KEY)",
        "app_zombie_ghost",
    );
    // 2. agent_% prefix zombie
    check_table_inventory_rejection(
        "CREATE TABLE agent_leak_table (id TEXT PRIMARY KEY)",
        "agent_leak_table",
    );
    // 3. conversation_% prefix zombie
    check_table_inventory_rejection(
        "CREATE TABLE conversation_phantom (id TEXT PRIMARY KEY)",
        "conversation_phantom",
    );
    // 4. web_record_% prefix zombie
    check_table_inventory_rejection(
        "CREATE TABLE web_record_remnant (id TEXT PRIMARY KEY)",
        "web_record_remnant",
    );
    // 5. One of the 8 dropped legacy tables: app_conversation_adapter_packages
    check_table_inventory_rejection(
        "CREATE TABLE app_conversation_adapter_packages (id TEXT PRIMARY KEY)",
        "app_conversation_adapter_packages",
    );
    // 6. Another legacy table: agent_installations
    check_table_inventory_rejection(
        "CREATE TABLE agent_installations (id TEXT PRIMARY KEY)",
        "agent_installations",
    );

    cleanup_database(&db_path);
}

// 4. Migration Ledger Self-Healing & Checksum Consistency
#[tokio::test]
async fn adversarial_challenge_4_migration_ledger_self_healing_and_checksum_consistency() {
    let db_path = temp_database_path("adv-ledger-self-healing");
    migrate_database(&db_path).await.expect("initial migration");

    let conn = Connection::open(&db_path).expect("open database");

    // Demote ALL migrations with version < 20261028000000 to 12-digit versions
    // (excluding 20260909000001, 20260909000002 which were always 14-digit)
    conn.execute_batch(
        r#"
        UPDATE _sqlx_migrations
        SET version = (version / 1000000) * 10000 + (version % 10000)
        WHERE version < 20261028000000
          AND version NOT IN (20260909000001, 20260909000002);
        "#,
    )
    .expect("downgrade to 12-digit versions");

    // Verify exactly 76 migrations are 12-digit
    let count_12_digit: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version < 10000000000000",
            [],
            |r| r.get(0),
        )
        .expect("count 12-digit");
    assert_eq!(
        count_12_digit, 76,
        "expected exactly 76 12-digit migrations"
    );

    drop(conn);

    // Re-open with open_migrated_pool, triggering repair_legacy_migration_ledger_versions
    let pool = open_migrated_pool(&db_path)
        .await
        .expect("open_migrated_pool must self-heal 12-digit ledger records to 14 digits");

    // Assert zero 12-digit records remain
    let remaining_legacy: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version < 10000000000000")
            .fetch_one(&pool)
            .await
            .expect("query remaining legacy");
    assert_eq!(
        remaining_legacy, 0,
        "all 12-digit versions must be upgraded to 14-digit"
    );

    // Assert total count matches MIGRATOR.migrations.len()
    let total_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .expect("query total migrations");
    assert_eq!(total_count, MIGRATOR.migrations.len() as i64);

    // Assert that all migrations in _sqlx_migrations are marked success = 1
    let failed_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success != 1")
            .fetch_one(&pool)
            .await
            .expect("query failed count");
    assert_eq!(
        failed_count, 0,
        "all migrations in ledger must have success = 1"
    );

    pool.close().await;

    // Idempotency test: re-run migrate_database multiple times without duplicate key errors
    for _ in 0..2 {
        migrate_database(&db_path)
            .await
            .expect("idempotent migration invocation on self-healed database");
    }

    cleanup_database(&db_path);
}

/// ADR-0021：轮次表收缩为 11 列位置身份，并原地对齐存量 Parts / FTS / 父引用。
#[tokio::test]
async fn acceptance_spec_175_positional_turn_identity_migration_preserves_offline_sessions() {
    let db_path = temp_database_path("spec-175-positional-turn-identity");
    migrate_database(&db_path).await.expect("initial migration");

    let conn = Connection::open(&db_path).expect("open database");
    conn.execute_batch(
        r#"
        INSERT OR IGNORE INTO conversation_adapters (
            tenant_id, id, name, kind, version, enabled, trust_state, capabilities,
            input_kinds, created_at, updated_at
        ) VALUES ('default', 'legacy-adapter', 'Legacy', 'external', '1.0.0', 1, 'trusted',
                  '[]', '[]', '2026-11-06T00:00:00Z', '2026-11-06T00:00:00Z');
        INSERT OR IGNORE INTO conversation_sources (
            tenant_id, id, adapter_id, name, kind, location, enabled, created_at, updated_at
        ) VALUES ('default', 'legacy-source', 'legacy-adapter', 'Legacy source', 'directory',
                  '/tmp/legacy', 1, '2026-11-06T00:00:00Z', '2026-11-06T00:00:00Z');
        INSERT INTO conversation_sessions (
            tenant_id, id, source_id, adapter_id, external_id, title, missing,
            created_at, imported_at
        ) VALUES
        ('default', 'legacy-session', 'legacy-source', 'legacy-adapter', 'ext-1', 'Legacy', 0,
         '2026-11-06T00:00:00Z', '2026-11-06T00:00:00Z'),
        ('default', 'legacy-offline-session', 'legacy-source', 'legacy-adapter', 'ext-2',
         'Offline', 1, '2026-11-06T00:00:00Z', '2026-11-06T00:00:00Z');

        DROP TABLE conversation_turns;
        CREATE TABLE conversation_turns (
            tenant_id   TEXT NOT NULL DEFAULT 'default',
            id          TEXT NOT NULL,
            session_id  TEXT NOT NULL,
            external_id TEXT NOT NULL,
            turn_index  INTEGER NOT NULL,
            prompt      TEXT NOT NULL DEFAULT '',
            process     TEXT,
            response    TEXT,
            started_at  TEXT,
            ended_at    TEXT,
            model       TEXT,
            fingerprint TEXT NOT NULL,
            missing     INTEGER NOT NULL DEFAULT 0,
            imported_at TEXT NOT NULL,
            PRIMARY KEY (tenant_id, id),
            UNIQUE (tenant_id, session_id, external_id)
        );
        INSERT INTO conversation_turns (
            tenant_id, id, session_id, external_id, turn_index, prompt, process, response,
            started_at, ended_at, model, fingerprint, missing, imported_at
        ) VALUES
        ('default', 'legacy-turn-parent', 'legacy-session', 'turn-0', 0, 'parent question',
         NULL, NULL, NULL, NULL, 'gpt', 'fp-parent', 0, '2026-11-06T00:00:00Z'),
        ('default', 'legacy-turn-sub', 'legacy-session', 'turn-1', 0, 'subagent question',
         '{"turn_kind":"subagent","parent_turn_id":"legacy-turn-parent"}',
         NULL, '2026-11-06T01:00:00Z', NULL, 'gpt', 'fp-sub', 0, '2026-11-06T00:00:00Z'),
        ('default', 'legacy-turn-offline', 'legacy-offline-session', 'msg-9f2ab1', 5,
         'offline question', NULL, NULL, NULL, NULL, 'gpt', 'fp-offline', 0,
         '2026-11-06T00:00:00Z'),
        ('default', 'legacy-turn-orphan', 'legacy-missing-session', 'turn-orphan', 2,
         'orphan question', NULL, NULL, NULL, NULL, 'gpt', 'fp-orphan', 0,
         '2026-11-06T00:00:00Z');

        DROP TABLE web_record_turns;
        CREATE TABLE web_record_turns (
            tenant_id   TEXT NOT NULL DEFAULT 'default',
            id          TEXT NOT NULL,
            session_id  TEXT NOT NULL,
            external_id TEXT NOT NULL,
            turn_index  INTEGER NOT NULL,
            prompt      TEXT NOT NULL DEFAULT '',
            process     TEXT,
            response    TEXT,
            started_at  TEXT,
            ended_at    TEXT,
            model       TEXT,
            fingerprint TEXT NOT NULL,
            missing     INTEGER NOT NULL DEFAULT 0,
            imported_at TEXT NOT NULL,
            PRIMARY KEY (tenant_id, id),
            UNIQUE (tenant_id, session_id, external_id)
        );
        INSERT INTO web_record_sessions (
            tenant_id, id, source_id, adapter_id, external_id, title, missing,
            created_at, imported_at
        ) VALUES ('default', 'legacy-web-session', 'legacy-source', 'legacy-adapter',
                  'ext-web', 'Legacy web', 0, '2026-11-06T00:00:00Z', '2026-11-06T00:00:00Z');
        INSERT INTO web_record_turns (
            tenant_id, id, session_id, external_id, turn_index, prompt, process, response,
            started_at, ended_at, model, fingerprint, missing, imported_at
        ) VALUES ('default', 'legacy-web-turn', 'legacy-web-session', 'turn-3', 7,
                  'web question', NULL, NULL, NULL, NULL, 'gpt', 'fp-web', 0,
                  '2026-11-06T00:00:00Z');

        INSERT INTO conversation_parts (
            tenant_id, turn_id, part_index, id_key, kind, content, attrs_json, translated_text
        ) VALUES
        ('default', 'legacy-turn-parent', 0, x'0000000000000001', 'answer',
         'parent answer body', NULL, NULL),
        ('default', 'legacy-turn-offline', 0, x'0000000000000002', 'answer',
         'offline answer body', NULL, NULL);

        INSERT INTO conversation_part_fts (tenant_id, part_id, session_id, turn_id, text) VALUES
        ('default', 'legacy-part-1', 'legacy-session', 'legacy-turn-parent', 'parent answer body'),
        ('default', 'legacy-part-2', 'legacy-offline-session', 'legacy-turn-offline',
         'offline answer body');

        INSERT INTO conversation_usage_events (
            tenant_id, source_id, adapter_id, external_event_id, session_id, turn_id,
            timestamp, provider, model, cost_basis, created_at, updated_at
        ) VALUES
        ('default', 'legacy-source', 'legacy-adapter', 'event-exact', 'ext-1', 'turn-1',
         '2026-11-06T01:00:00Z', 'openai', 'gpt', 'none', '2026-11-06T00:00:00Z',
         '2026-11-06T00:00:00Z'),
        ('default', 'legacy-source', 'legacy-adapter', 'event-unmapped', 'ext-1', 'msg-9f2ab1',
         '2026-11-06T01:00:00Z', 'openai', 'gpt', 'none', '2026-11-06T00:00:00Z',
         '2026-11-06T00:00:00Z');

        -- 迁移已跑过一次，回滚夹具时先撤掉它新增的列与索引，模拟升级前的真实形态。
        DROP INDEX IF EXISTS idx_conversation_usage_events_tenant_session_turn;
        ALTER TABLE conversation_usage_events DROP COLUMN turn_index;
        DELETE FROM _sqlx_migrations WHERE version = 20261106000001;
        "#,
    )
    .expect("seed legacy 14-column turns");
    drop(conn);

    migrate_database(&db_path)
        .await
        .expect("apply positional turn identity migration");

    let conn = Connection::open(&db_path).expect("open migrated database");

    // A. 11 列终态结构
    for table in ["conversation_turns", "web_record_turns"] {
        let columns: Vec<String> = conn
            .prepare(&format!(
                "SELECT name FROM pragma_table_info('{table}') ORDER BY cid"
            ))
            .expect("prepare turn columns")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query turn columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect turn columns");
        assert_eq!(
            columns,
            vec![
                "tenant_id",
                "id",
                "session_id",
                "turn_index",
                "prompt",
                "process",
                "response",
                "started_at",
                "ended_at",
                "model",
                "imported_at",
            ],
            "{table} 必须收缩为 11 列位置身份结构"
        );
    }

    // B. 序号归一化 + 主键重算 + 父引用换轨
    let parent_turn_id = crate::backend::domain::derive_conversation_turn_id("legacy-session", 0);
    let sub_turn_id = crate::backend::domain::derive_conversation_turn_id("legacy-session", 1);
    let offline_turn_id =
        crate::backend::domain::derive_conversation_turn_id("legacy-offline-session", 0);
    let orphan_turn_id =
        crate::backend::domain::derive_conversation_turn_id("legacy-missing-session", 0);

    let rows: Vec<(String, i64, String)> = conn
        .prepare("SELECT id, turn_index, started_at FROM conversation_turns ORDER BY session_id, turn_index")
        .expect("prepare turn rows")
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .expect("query turn rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect turn rows");
    assert_eq!(
        rows.iter()
            .map(|(id, index, _)| (id.clone(), *index))
            .collect::<Vec<_>>(),
        vec![
            (orphan_turn_id.clone(), 0),
            (offline_turn_id.clone(), 0),
            (parent_turn_id.clone(), 0),
            (sub_turn_id.clone(), 1),
        ],
        "存量轮次必须按会话重排为连续 0..N 并重算位置主键，且孤儿轮次不得丢失"
    );
    assert!(
        rows.iter()
            .all(|(_, _, started_at)| !started_at.trim().is_empty()),
        "started_at 必须收紧为非空"
    );

    let parent_ref: Option<String> = conn
        .query_row(
            "SELECT json_extract(process, '$.parent_turn_id') FROM conversation_turns WHERE id = ?1",
            [&sub_turn_id],
            |row| row.get(0),
        )
        .expect("query parent reference");
    assert_eq!(
        parent_ref.as_deref(),
        Some(parent_turn_id.as_str()),
        "子代理父引用必须级联到新的父轮次主键"
    );

    // C. 离线会话与内容事实零丢失，FTS 仍可命中
    let offline_parts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_parts WHERE turn_id = ?1",
            [&offline_turn_id],
            |row| row.get(0),
        )
        .expect("count offline parts");
    assert_eq!(offline_parts, 1, "源文件已移除的离线会话必须完整保留");

    let fts_hits: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_part_fts WHERE conversation_part_fts MATCH 'offline'",
            [],
            |row| row.get(0),
        )
        .expect("query offline fts hit");
    assert_eq!(fts_hits, 1, "存量全文索引必须随轮次主键重算后继续命中");

    // D. 用量事件按 turn_index 归因
    let attributed: Option<i64> = conn
        .query_row(
            "SELECT turn_index FROM conversation_usage_events WHERE external_event_id = 'event-exact'",
            [],
            |row| row.get(0),
        )
        .expect("query attributed usage event");
    assert_eq!(
        attributed,
        Some(1),
        "旧 (session, external_id) 必须回填到轮次序号"
    );
    let unmapped: Option<i64> = conn
        .query_row(
            "SELECT turn_index FROM conversation_usage_events WHERE external_event_id = 'event-unmapped'",
            [],
            |row| row.get(0),
        )
        .expect("query unmapped usage event");
    assert_eq!(unmapped, None, "无法定位的源键必须置空");

    let web_turn_id = crate::backend::domain::derive_web_record_turn_id("legacy-web-session", 0);
    let web_row: (String, i64) = conn
        .query_row("SELECT id, turn_index FROM web_record_turns", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("query web turn");
    assert_eq!(
        web_row,
        (web_turn_id, 0),
        "web 记录族必须与客户端会话族保持同一套位置身份"
    );

    // E. 幂等：再次迁移零写入
    drop(conn);
    migrate_database(&db_path)
        .await
        .expect("idempotent migration");
    let conn = Connection::open(&db_path).expect("reopen database");
    let ids_after: Vec<String> = conn
        .prepare("SELECT id FROM conversation_turns ORDER BY id")
        .expect("prepare idempotent ids")
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query idempotent ids")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect idempotent ids");
    let mut expected = vec![parent_turn_id, sub_turn_id, offline_turn_id, orphan_turn_id];
    expected.sort();
    assert_eq!(ids_after, expected, "二次迁移必须完全幂等");

    cleanup_database(&db_path);
}

/// `20261106000001` 首版（内连接版）已经应用过的库只需对齐账本校验和，不必重跑迁移。
#[tokio::test]
async fn migrations_realign_the_superseded_session_snapshot_checksum() {
    let db_path = temp_database_path("superseded-session-snapshot-checksum");
    migrate_database(&db_path)
        .await
        .expect("create current database");

    let conn = Connection::open(&db_path).expect("open database");
    conn.execute(
        "UPDATE _sqlx_migrations SET checksum = X'0873471DAC6AB5BF57D94A48B78688DF7F91D6CF14A615E63664D3A79476319D7FC7C5B0F9AF64C3F68F0738BE572177' WHERE version = 20261106000001",
        [],
    )
    .expect("rewind ledger checksum to the superseded release");
    drop(conn);

    // 未修复前这一步会以 "previously applied but has been modified" 失败。
    migrate_database(&db_path)
        .await
        .expect("self-heal superseded checksum");

    let conn = Connection::open(&db_path).expect("reopen database");
    let checksum: String = conn
        .query_row(
            "SELECT upper(hex(checksum)) FROM _sqlx_migrations WHERE version = 20261106000001",
            [],
            |row| row.get(0),
        )
        .expect("read realigned checksum");
    assert_eq!(
        checksum,
        "3687DFBE3A28A580281F87CC5728A634FA5CF18AE9E31D8B9ECD0F5F376ECB31B66C4544D8A783FE43D6D1BEA0B49371",
        "账本校验和必须对齐到当前迁移文件"
    );

    // 幂等：再跑一次不会再改任何东西。
    drop(conn);
    migrate_database(&db_path)
        .await
        .expect("idempotent after realignment");
    cleanup_database(&db_path);
}
