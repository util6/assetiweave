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

fn table_names(record_kind: ConversationRecordKind) -> (&'static str, &'static str) {
    match record_kind {
        ConversationRecordKind::Session => ("conversation_sessions", "conversation_turns"),
        ConversationRecordKind::Web => ("web_record_sessions", "web_record_turns"),
    }
}

async fn seed_session(pool: &SqlitePool, record_kind: ConversationRecordKind, session_id: &str) {
    let (sessions, _) = table_names(record_kind);
    let sql = format!(
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
    sqlx::query(AssertSqlSafe(sql))
        .bind(TENANT_ID)
        .bind(session_id)
        .bind(format!("{session_id}-external"))
        .execute(pool)
        .await
        .expect("seed session");
}

#[allow(clippy::too_many_arguments)]
async fn seed_turn(
    pool: &SqlitePool,
    record_kind: ConversationRecordKind,
    session_id: &str,
    turn_id: &str,
    turn_index: i64,
    prompt: &str,
    response: Option<&str>,
    process: Option<&str>,
) {
    let (_, turns) = table_names(record_kind);
    let sql = format!(
        r#"
        INSERT INTO {turns} (
            tenant_id, id, session_id, turn_index, prompt, process, response,
            started_at, ended_at, imported_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, '2026-10-23T00:00:00Z', NULL,
                  '2026-10-23T00:00:00Z')
        "#
    );
    sqlx::query(AssertSqlSafe(sql))
        .bind(TENANT_ID)
        .bind(turn_id)
        .bind(session_id)
        .bind(turn_index)
        .bind(prompt)
        .bind(process)
        .bind(response)
        .execute(pool)
        .await
        .expect("seed turn");
}

#[allow(clippy::too_many_arguments)]
async fn seed_part(
    pool: &SqlitePool,
    record_kind: ConversationRecordKind,
    turn_id: &str,
    index: i64,
    kind: &str,
    content: Option<&str>,
    attrs_json: Option<&str>,
) {
    match record_kind {
        ConversationRecordKind::Session => {
            let part_id = derive_part_id("conversation-part", turn_id, index);
            let id_key = derive_part_id_key(&part_id).expect("derive id_key");
            sqlx::query(
                r#"
                INSERT INTO conversation_parts (
                    tenant_id, turn_id, part_index, id_key, kind, content, attrs_json, translated_text
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)
                "#,
            )
            .bind(TENANT_ID)
            .bind(turn_id)
            .bind(index)
            .bind(id_key.as_slice())
            .bind(kind)
            .bind(content)
            .bind(attrs_json)
            .execute(pool)
            .await
            .expect("seed part");
        }
        ConversationRecordKind::Web => {
            let part_id = derive_part_id("web-record-part", turn_id, index);
            let id_key = derive_part_id_key(&part_id).unwrap_or([0u8; 8]);
            sqlx::query(
                r#"
                INSERT INTO web_record_parts (
                    tenant_id, turn_id, part_index, id_key, kind, content, translated_text
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)
                "#,
            )
            .bind(TENANT_ID)
            .bind(turn_id)
            .bind(index)
            .bind(id_key.to_vec())
            .bind(kind)
            .bind(content)
            .execute(pool)
            .await
            .expect("seed part");
        }
    }
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
async fn corpus_keeps_only_prompt_and_final_answer() {
    let fixture = fixture().await;
    let session_id = "conversation-session-signal";
    seed_session(&fixture.pool, ConversationRecordKind::Session, session_id).await;
    seed_turn(
        &fixture.pool,
        ConversationRecordKind::Session,
        session_id,
        "conversation-turn-signal",
        0,
        "如何把会话同步从 29 分钟降到 2 秒？",
        Some("把搜索索引移出事实事务即可。"),
        None,
    )
    .await;
    seed_part(
        &fixture.pool,
        ConversationRecordKind::Session,
        "conversation-turn-signal",
        10,
        "tool",
        Some("connection refused by upstream"),
        None,
    )
    .await;
    seed_part(
        &fixture.pool,
        ConversationRecordKind::Session,
        "conversation-turn-signal",
        11,
        "code",
        Some("diff --git a/x b/x\n@@ -1 +1 @@"),
        None,
    )
    .await;

    let indexed = replace_session_search_corpus_sqlx(&fixture.pool, TENANT_ID, session_id)
        .await
        .expect("replace corpus");

    assert_eq!(indexed, 2, "只有提问与最终回答允许进入语料");
    let texts = indexed_texts(&fixture.pool, session_id).await;
    assert!(texts.iter().any(|text| text.contains("29 分钟")));
    assert!(texts.iter().any(|text| text.contains("移出事实事务")));
    assert!(!texts.iter().any(|text| text.contains("connection refused")));
    assert!(!texts.iter().any(|text| text.contains("diff --git")));

    fixture.cleanup();
}

#[tokio::test]
async fn corpus_collects_changed_paths_and_command_labels() {
    let fixture = fixture().await;
    let session_id = "conversation-session-exec";
    seed_session(&fixture.pool, ConversationRecordKind::Session, session_id).await;
    seed_turn(
        &fixture.pool,
        ConversationRecordKind::Session,
        session_id,
        "conversation-turn-exec",
        0,
        "重建索引",
        None,
        Some(r#"{"items":[{"seq":0,"kind":"command","label":"编译构建","content":"pnpm build"}]}"#),
    )
    .await;
    seed_part(
        &fixture.pool,
        ConversationRecordKind::Session,
        "conversation-turn-exec",
        10,
        "tool",
        Some("patched 42 lines"),
        Some(r#"{"file_path":"~/project/src/snapshot.rs","command_label":"Edit"}"#),
    )
    .await;

    replace_session_search_corpus_sqlx(&fixture.pool, TENANT_ID, session_id)
        .await
        .expect("replace corpus");

    let texts = indexed_texts(&fixture.pool, session_id).await;
    assert!(
        texts.iter().any(|text| text == "~/project/src/snapshot.rs"),
        "变更文件路径必须进入语料: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text.contains("编译构建")),
        "命令语义标签必须进入语料: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text.contains("pnpm build")),
        "原始命令不得进入语料"
    );

    fixture.cleanup();
}

#[tokio::test]
async fn replacement_is_idempotent() {
    let fixture = fixture().await;
    let session_id = "conversation-session-idempotent";
    seed_session(&fixture.pool, ConversationRecordKind::Session, session_id).await;
    seed_turn(
        &fixture.pool,
        ConversationRecordKind::Session,
        session_id,
        "conversation-turn-idempotent",
        0,
        "提问",
        Some("回答"),
        None,
    )
    .await;

    let first = replace_session_search_corpus_sqlx(&fixture.pool, TENANT_ID, session_id)
        .await
        .expect("first replace");
    let second = replace_session_search_corpus_sqlx(&fixture.pool, TENANT_ID, session_id)
        .await
        .expect("second replace");

    assert_eq!(first, second);
    assert_eq!(indexed_texts(&fixture.pool, session_id).await.len(), second);

    fixture.cleanup();
}

#[tokio::test]
async fn replaced_snapshot_drops_withdrawn_corpus_rows() {
    let fixture = fixture().await;
    let session_id = "conversation-session-withdraw";
    seed_session(&fixture.pool, ConversationRecordKind::Session, session_id).await;
    seed_turn(
        &fixture.pool,
        ConversationRecordKind::Session,
        session_id,
        "conversation-turn-withdraw",
        0,
        "第一版提问",
        Some("第一版回答"),
        None,
    )
    .await;

    replace_session_search_corpus_sqlx(&fixture.pool, TENANT_ID, session_id)
        .await
        .expect("replace corpus");

    // 模拟宿主侧回滚：同一会话被整包替换为新快照，旧轮次不再存在。
    sqlx::query("DELETE FROM conversation_turns WHERE tenant_id = ?1 AND session_id = ?2")
        .bind(TENANT_ID)
        .bind(session_id)
        .execute(&fixture.pool)
        .await
        .expect("drop turns");
    seed_turn(
        &fixture.pool,
        ConversationRecordKind::Session,
        session_id,
        "conversation-turn-withdraw-2",
        0,
        "第二版提问",
        Some("第二版回答"),
        None,
    )
    .await;

    replace_session_search_corpus_sqlx(&fixture.pool, TENANT_ID, session_id)
        .await
        .expect("replace corpus again");

    let texts = indexed_texts(&fixture.pool, session_id).await;
    assert_eq!(texts.len(), 2);
    assert!(!texts.iter().any(|text| text.contains("第一版")));

    fixture.cleanup();
}

#[tokio::test]
async fn web_record_sessions_use_the_same_corpus_policy() {
    let fixture = fixture().await;
    let session_id = "web-record-session-corpus";
    seed_session(&fixture.pool, ConversationRecordKind::Web, session_id).await;
    seed_turn(
        &fixture.pool,
        ConversationRecordKind::Web,
        session_id,
        "web-record-turn-corpus",
        0,
        "网页提问",
        Some("网页回答"),
        None,
    )
    .await;
    seed_part(
        &fixture.pool,
        ConversationRecordKind::Web,
        "web-record-turn-corpus",
        5,
        "process",
        Some("搜索了 12 个页面"),
        None,
    )
    .await;

    let indexed = replace_session_search_corpus_sqlx(&fixture.pool, TENANT_ID, session_id)
        .await
        .expect("replace web corpus");

    assert_eq!(indexed, 2);
    let texts = indexed_texts(&fixture.pool, session_id).await;
    assert!(!texts.iter().any(|text| text.contains("搜索了 12 个页面")));

    fixture.cleanup();
}

#[tokio::test]
async fn tenant_rebuild_reports_row_count_and_optimizes() {
    let fixture = fixture().await;
    for (session_id, turn_id) in [
        ("conversation-session-a", "conversation-turn-a"),
        ("conversation-session-b", "conversation-turn-b"),
    ] {
        seed_session(&fixture.pool, ConversationRecordKind::Session, session_id).await;
        seed_turn(
            &fixture.pool,
            ConversationRecordKind::Session,
            session_id,
            turn_id,
            0,
            "提问",
            None,
            None,
        )
        .await;
    }

    let mut progress = Vec::new();
    let count = rebuild_tenant_part_index_sqlx(&fixture.pool, TENANT_ID, |done, total| {
        progress.push((done, total));
    })
    .await
    .expect("rebuild tenant corpus");

    assert_eq!(count, 2);
    assert_eq!(progress.first(), Some(&(0, 2)));
    assert_eq!(progress.last(), Some(&(2, 2)));

    fixture.cleanup();
}

#[tokio::test]
async fn corpus_drift_is_closed_by_a_controlled_rebuild() {
    let fixture = fixture().await;
    let session_id = "conversation-session-drift";
    seed_session(&fixture.pool, ConversationRecordKind::Session, session_id).await;
    seed_turn(
        &fixture.pool,
        ConversationRecordKind::Session,
        session_id,
        "conversation-turn-drift",
        0,
        "提问",
        Some("回答"),
        None,
    )
    .await;
    // 模拟迁移留下的旧版索引：状态就绪但语料口径落后。
    set_conversation_part_index_status_sqlx(&fixture.pool, TENANT_ID, PART_INDEX_STALE, 0, None)
        .await
        .expect("mark stale");

    assert!(
        !conversation_part_index_ready_sqlx(&fixture.pool, TENANT_ID)
            .await
            .expect("read ready flag")
    );

    assert!(
        conversation_search_corpus_needs_rebuild_sqlx(&fixture.pool, TENANT_ID)
            .await
            .expect("detect corpus drift")
    );

    // 受控重建：按会话分批推进，收尾统一发布就绪状态。
    let part_count = rebuild_tenant_part_index_sqlx(&fixture.pool, TENANT_ID, |_, _| {})
        .await
        .expect("rebuild tenant corpus");
    set_conversation_part_index_status_sqlx(
        &fixture.pool,
        TENANT_ID,
        PART_INDEX_READY,
        part_count,
        None,
    )
    .await
    .expect("publish index generation");

    assert!(conversation_part_index_ready_sqlx(&fixture.pool, TENANT_ID)
        .await
        .expect("read ready flag"));
    assert_eq!(indexed_texts(&fixture.pool, session_id).await.len(), 2);
    assert!(
        !conversation_search_corpus_needs_rebuild_sqlx(&fixture.pool, TENANT_ID)
            .await
            .expect("detect corpus drift")
    );

    fixture.cleanup();
}

#[tokio::test]
async fn source_scoped_replacement_covers_every_session() {
    let fixture = fixture().await;
    for (session_id, turn_id) in [
        ("conversation-session-s1", "conversation-turn-s1"),
        ("conversation-session-s2", "conversation-turn-s2"),
    ] {
        seed_session(&fixture.pool, ConversationRecordKind::Session, session_id).await;
        seed_turn(
            &fixture.pool,
            ConversationRecordKind::Session,
            session_id,
            turn_id,
            0,
            "提问",
            Some("回答"),
            None,
        )
        .await;
    }

    let indexed = replace_source_search_corpus_sqlx(&fixture.pool, TENANT_ID, "fixture-source")
        .await
        .expect("replace source corpus");

    assert_eq!(indexed, 4);

    fixture.cleanup();
}
