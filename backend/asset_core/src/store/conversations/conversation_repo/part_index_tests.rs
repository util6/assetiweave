use super::*;
use crate::backend::domain::{derive_part_id, derive_part_id_key};
use std::path::PathBuf;

const TENANT_ID: &str = "default";

struct Fixture {
    pool: SqlitePool,
    db_path: PathBuf,
}

impl Fixture {
    fn cleanup(self) {
        drop(self.pool);
        let _ = std::fs::remove_file(self.db_path);
    }
}

async fn fixture() -> Fixture {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-part-index-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let pool = crate::backend::store::open_migrated_pool(&db_path)
        .await
        .expect("open migrated pool");
    Fixture { pool, db_path }
}

/// 直接写入会话 / 轮次 / 零件三表，绕开适配器：零件索引只依赖零件本身。
async fn seed_session(pool: &SqlitePool, record_kind: ConversationRecordKind, session_id: &str) {
    let sessions = match record_kind {
        ConversationRecordKind::Session => "conversation_sessions",
        ConversationRecordKind::Web => "web_record_sessions",
    };
    let session_sql = format!(
        r#"
        INSERT INTO {sessions} (
            tenant_id, id, source_id, adapter_id, external_id, title,
            started_at, updated_at, source_locator, source_fingerprint, missing,
            created_at, imported_at
        ) VALUES (
            ?1, ?2, 'fixture-source', 'fixture-adapter', ?3, 'Fixture',
            NULL, NULL, NULL, NULL, 0, '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z'
        )
        "#
    );
    sqlx::query(AssertSqlSafe(session_sql))
        .bind(TENANT_ID)
        .bind(session_id)
        .bind(format!("{session_id}-external"))
        .execute(pool)
        .await
        .expect("seed session");
}

async fn seed_turn(
    pool: &SqlitePool,
    record_kind: ConversationRecordKind,
    session_id: &str,
    turn_id: &str,
    turn_index: i64,
) {
    let turns = match record_kind {
        ConversationRecordKind::Session => "conversation_turns",
        ConversationRecordKind::Web => "web_record_turns",
    };
    let turn_sql = format!(
        r#"
        INSERT INTO {turns} (
            tenant_id, id, session_id, external_id, turn_index, user_text,
            title, started_at, ended_at, fingerprint, missing, imported_at
        ) VALUES (?1, ?2, ?3, ?2, ?4, 'how does indexing work', NULL, NULL, NULL,
                  'fixture-fingerprint', 0, '2026-10-23T00:00:00Z')
        "#
    );
    sqlx::query(AssertSqlSafe(turn_sql))
        .bind(TENANT_ID)
        .bind(turn_id)
        .bind(session_id)
        .bind(turn_index)
        .execute(pool)
        .await
        .expect("seed turn");
}

struct SeedPart<'a> {
    card_kind: &'a str,
    text: Option<&'a str>,
    command: Option<&'a str>,
    role: &'a str,
    kind: &'a str,
    content_card_json: Option<&'a str>,
}

impl<'a> SeedPart<'a> {
    fn card(card_kind: &'a str, text: &'a str) -> Self {
        Self {
            card_kind,
            text: Some(text),
            command: None,
            role: "tool",
            kind: "text",
            content_card_json: None,
        }
    }

    fn command(command: &'a str) -> Self {
        Self {
            card_kind: "command",
            text: None,
            command: Some(command),
            role: "tool",
            kind: "command",
            content_card_json: Some(
                r#"{"schema_version":1,"kind":"fixture.command","renderer":"command"}"#,
            ),
        }
    }
}

async fn seed_part(
    pool: &SqlitePool,
    record_kind: ConversationRecordKind,
    turn_id: &str,
    index: i64,
    part: SeedPart<'_>,
) -> String {
    let part_prefix = match record_kind {
        ConversationRecordKind::Session => "conversation-part",
        ConversationRecordKind::Web => "web-record-part",
    };
    let part_id = derive_part_id(part_prefix, turn_id, index);
    let metadata_json = format!(r#"{{"content_card":{{"type":"{}"}}}}"#, part.card_kind);
    match record_kind {
        ConversationRecordKind::Session => {
            let id_key = derive_part_id_key(&part_id).expect("derive id_key");
            let content = part.text.or(part.command);
            sqlx::query(
                r#"
                INSERT INTO conversation_parts (
                    tenant_id, turn_id, part_index, id_key, role, kind, card_kind, content,
                    attrs_json, metadata_json, translated_text
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, NULL)
                "#,
            )
            .bind(TENANT_ID)
            .bind(turn_id)
            .bind(index)
            .bind(id_key.as_slice())
            .bind(part.role)
            .bind(part.kind)
            .bind(part.card_kind)
            .bind(content)
            .bind(metadata_json)
            .execute(pool)
            .await
            .expect("seed part");
        }
        ConversationRecordKind::Web => {
            sqlx::query(
                r#"
                INSERT INTO web_record_parts (
                    tenant_id, id, turn_id, part_index, role, kind, text, language, command,
                    cwd, status, exit_code, metadata_json, content_card_json, translated_text,
                    source_execution_id, command_label
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, NULL, NULL, NULL, ?9, ?10, NULL,
                          NULL, NULL)
                "#,
            )
            .bind(TENANT_ID)
            .bind(&part_id)
            .bind(turn_id)
            .bind(index)
            .bind(part.role)
            .bind(part.kind)
            .bind(part.text)
            .bind(part.command)
            .bind(metadata_json)
            .bind(part.content_card_json)
            .execute(pool)
            .await
            .expect("seed part");
        }
    }
    part_id
}

async fn indexed_texts(pool: &SqlitePool, session_id: &str) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT text FROM conversation_part_fts WHERE tenant_id = ?1 AND session_id = ?2 ORDER BY text ASC",
    )
    .bind(TENANT_ID)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .expect("load indexed texts")
}

#[tokio::test]
async fn every_card_body_is_indexed_without_role_classification() {
    let fixture = fixture().await;
    let session_id = "conversation-session-tools";
    seed_session(&fixture.pool, ConversationRecordKind::Session, session_id).await;
    seed_turn(
        &fixture.pool,
        ConversationRecordKind::Session,
        session_id,
        "conversation-turn-tools",
        0,
    )
    .await;
    for (index, (card_kind, text)) in [
        ("answer", "the answer body"),
        ("tool_result", "connection refused by upstream"),
        ("file_change", "patched src/main.rs and added retry"),
        ("subagent_text", "sub agent finished the migration"),
        ("execution_result", "exit code 1 with stack trace"),
    ]
    .into_iter()
    .enumerate()
    {
        seed_part(
            &fixture.pool,
            ConversationRecordKind::Session,
            "conversation-turn-tools",
            index as i64,
            SeedPart::card(card_kind, text),
        )
        .await;
    }

    let mut tx = fixture.pool.begin().await.expect("begin transaction");
    let indexed = replace_turn_part_index_sqlx_tx(
        &mut tx,
        TENANT_ID,
        "conversation-turn-tools",
        ConversationRecordKind::Session,
    )
    .await
    .expect("index turn parts");
    tx.commit().await.expect("commit transaction");

    assert_eq!(indexed, 5);
    let texts = indexed_texts(&fixture.pool, session_id).await;
    assert_eq!(texts.len(), 5);
    for expected in [
        "the answer body",
        "connection refused by upstream",
        "patched src/main.rs and added retry",
        "sub agent finished the migration",
        "exit code 1 with stack trace",
    ] {
        assert!(
            texts.iter().any(|text| text == expected),
            "missing indexed body: {expected}"
        );
    }

    fixture.cleanup();
}

#[tokio::test]
async fn raw_shell_command_part_stays_searchable() {
    let fixture = fixture().await;
    let session_id = "conversation-session-shell";
    seed_session(&fixture.pool, ConversationRecordKind::Session, session_id).await;
    seed_turn(
        &fixture.pool,
        ConversationRecordKind::Session,
        session_id,
        "conversation-turn-shell",
        0,
    )
    .await;
    seed_part(
        &fixture.pool,
        ConversationRecordKind::Session,
        "conversation-turn-shell",
        0,
        SeedPart::command("raw shell command"),
    )
    .await;

    let mut tx = fixture.pool.begin().await.expect("begin transaction");
    replace_turn_part_index_sqlx_tx(
        &mut tx,
        TENANT_ID,
        "conversation-turn-shell",
        ConversationRecordKind::Session,
    )
    .await
    .expect("index command part");
    tx.commit().await.expect("commit transaction");

    let texts = indexed_texts(&fixture.pool, session_id).await;
    assert_eq!(texts.len(), 1);
    assert!(
        texts[0].contains("raw shell command"),
        "command body must stay searchable, got {:?}",
        texts[0]
    );

    fixture.cleanup();
}

#[tokio::test]
async fn reindexing_one_turn_leaves_other_turns_and_drops_stale_rows() {
    let fixture = fixture().await;
    let session_id = "conversation-session-turns";
    seed_session(&fixture.pool, ConversationRecordKind::Session, session_id).await;
    seed_turn(
        &fixture.pool,
        ConversationRecordKind::Session,
        session_id,
        "conversation-turn-first",
        0,
    )
    .await;
    seed_turn(
        &fixture.pool,
        ConversationRecordKind::Session,
        session_id,
        "conversation-turn-second",
        1,
    )
    .await;
    let first_part_id = seed_part(
        &fixture.pool,
        ConversationRecordKind::Session,
        "conversation-turn-first",
        0,
        SeedPart::card("answer", "first turn body"),
    )
    .await;
    seed_part(
        &fixture.pool,
        ConversationRecordKind::Session,
        "conversation-turn-second",
        0,
        SeedPart::card("answer", "second turn body"),
    )
    .await;

    for turn_id in ["conversation-turn-first", "conversation-turn-second"] {
        let mut tx = fixture.pool.begin().await.expect("begin transaction");
        replace_turn_part_index_sqlx_tx(
            &mut tx,
            TENANT_ID,
            turn_id,
            ConversationRecordKind::Session,
        )
        .await
        .expect("index turn");
        tx.commit().await.expect("commit transaction");
    }

    // 只改第一轮并重建该轮索引：第二轮索引行必须原样保留，第一轮不得留重复行。
    let first_part_key = derive_part_id_key(&first_part_id).expect("derive id_key");
    sqlx::query(
        "UPDATE conversation_parts SET content = 'first turn rewritten' WHERE tenant_id = ?1 AND id_key = ?2",
    )
    .bind(TENANT_ID)
    .bind(first_part_key.as_slice())
    .execute(&fixture.pool)
    .await
    .expect("rewrite part body");
    let mut tx = fixture.pool.begin().await.expect("begin transaction");
    replace_turn_part_index_sqlx_tx(
        &mut tx,
        TENANT_ID,
        "conversation-turn-first",
        ConversationRecordKind::Session,
    )
    .await
    .expect("reindex turn");
    tx.commit().await.expect("commit transaction");

    let texts = indexed_texts(&fixture.pool, session_id).await;
    assert_eq!(
        texts,
        vec![
            "first turn rewritten".to_string(),
            "second turn body".to_string()
        ]
    );

    fixture.cleanup();
}

#[tokio::test]
async fn skips_parts_without_projected_body() {
    let fixture = fixture().await;
    let session_id = "conversation-session-empty";
    seed_session(&fixture.pool, ConversationRecordKind::Session, session_id).await;
    seed_turn(
        &fixture.pool,
        ConversationRecordKind::Session,
        session_id,
        "conversation-turn-empty",
        0,
    )
    .await;
    seed_part(
        &fixture.pool,
        ConversationRecordKind::Session,
        "conversation-turn-empty",
        0,
        SeedPart::card("answer", "   "),
    )
    .await;

    let mut tx = fixture.pool.begin().await.expect("begin transaction");
    let indexed = replace_turn_part_index_sqlx_tx(
        &mut tx,
        TENANT_ID,
        "conversation-turn-empty",
        ConversationRecordKind::Session,
    )
    .await
    .expect("index blank part");
    tx.commit().await.expect("commit transaction");

    assert_eq!(indexed, 0);
    assert!(indexed_texts(&fixture.pool, session_id).await.is_empty());

    fixture.cleanup();
}

#[tokio::test]
async fn rebuild_covers_both_record_families_in_one_index() {
    let fixture = fixture().await;
    seed_session(
        &fixture.pool,
        ConversationRecordKind::Session,
        "conversation-session-rebuild",
    )
    .await;
    seed_turn(
        &fixture.pool,
        ConversationRecordKind::Session,
        "conversation-session-rebuild",
        "conversation-turn-rebuild",
        0,
    )
    .await;
    seed_session(
        &fixture.pool,
        ConversationRecordKind::Web,
        "web-record-session-rebuild",
    )
    .await;
    seed_turn(
        &fixture.pool,
        ConversationRecordKind::Web,
        "web-record-session-rebuild",
        "web-record-turn-rebuild",
        0,
    )
    .await;
    seed_part(
        &fixture.pool,
        ConversationRecordKind::Session,
        "conversation-turn-rebuild",
        0,
        SeedPart::card("answer", "session body"),
    )
    .await;
    seed_part(
        &fixture.pool,
        ConversationRecordKind::Web,
        "web-record-turn-rebuild",
        0,
        SeedPart::card("tool_result", "web tool output"),
    )
    .await;

    sqlx::query("DELETE FROM conversation_part_fts")
        .execute(&fixture.pool)
        .await
        .expect("wipe index");

    let mut reported = Vec::new();
    let count = rebuild_tenant_part_index_sqlx(&fixture.pool, TENANT_ID, |done, total| {
        reported.push((done, total));
    })
    .await
    .expect("rebuild tenant part index");

    assert_eq!(count, 2);
    assert_eq!(reported.first(), Some(&(0, 2)));
    assert_eq!(reported.last(), Some(&(2, 2)));
    assert_eq!(
        indexed_texts(&fixture.pool, "conversation-session-rebuild").await,
        vec!["session body".to_string()]
    );
    assert_eq!(
        indexed_texts(&fixture.pool, "web-record-session-rebuild").await,
        vec!["web tool output".to_string()]
    );

    fixture.cleanup();
}

#[tokio::test]
async fn state_without_row_is_ready_and_rebuild_marks_readiness() {
    let fixture = fixture().await;

    assert!(
        conversation_part_index_ready_sqlx(&fixture.pool, TENANT_ID)
            .await
            .expect("read readiness"),
        "missing state row must not degrade search"
    );

    set_conversation_part_index_status_sqlx(&fixture.pool, TENANT_ID, PART_INDEX_BUILDING, 0, None)
        .await
        .expect("mark building");
    assert!(
        !conversation_part_index_ready_sqlx(&fixture.pool, TENANT_ID)
            .await
            .expect("read readiness while building")
    );

    set_conversation_part_index_status_sqlx(
        &fixture.pool,
        TENANT_ID,
        PART_INDEX_STALE,
        0,
        Some("boom"),
    )
    .await
    .expect("mark stale");
    let stale = load_conversation_part_index_state_sqlx(&fixture.pool, TENANT_ID)
        .await
        .expect("load state")
        .expect("state row");
    assert_eq!(stale.status, PART_INDEX_STALE);
    assert_eq!(stale.last_error.as_deref(), Some("boom"));
    assert!(
        !conversation_part_index_ready_sqlx(&fixture.pool, TENANT_ID)
            .await
            .expect("read readiness while stale")
    );

    set_conversation_part_index_status_sqlx(&fixture.pool, TENANT_ID, PART_INDEX_READY, 7, None)
        .await
        .expect("mark ready");
    let ready = load_conversation_part_index_state_sqlx(&fixture.pool, TENANT_ID)
        .await
        .expect("load state")
        .expect("state row");
    assert!(ready.is_ready());
    assert_eq!(ready.part_count, 7);
    assert!(ready.rebuilt_at.is_some());

    fixture.cleanup();
}
