use super::*;
use crate::backend::store::test_support::create_test_pool;
use sqlx::AssertSqlSafe;

const TENANT: &str = "default";
const ADAPTER: &str = "adp-1";
const SOURCE: &str = "src-1";

async fn seed(pool: &SqlitePool, statements: &[&str]) {
    for statement in statements {
        sqlx::query(AssertSqlSafe(statement.to_string()))
            .execute(pool)
            .await
            .unwrap_or_else(|error| panic!("seed statement failed: {error}\n{statement}"));
    }
}

fn source_fixture(adapter_id: &str) -> String {
    format!(
        "INSERT INTO conversation_sources (tenant_id, id, adapter_id, name, kind, location, enabled, created_at, updated_at) \
         VALUES ('{TENANT}', '{SOURCE}', '{adapter_id}', 'Source', 'fs', '/path', 1, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')"
    )
}

fn session_fixture(session_id: &str, imported_at: &str) -> String {
    format!(
        "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, project_path, missing, created_at, imported_at) \
         VALUES ('{TENANT}', '{session_id}', '{SOURCE}', '{ADAPTER}', 'ext-{session_id}', 'Title {session_id}', '/path/project', 0, '{imported_at}', '{imported_at}')"
    )
}

fn web_session_fixture(session_id: &str, imported_at: &str) -> String {
    format!(
        "INSERT INTO web_record_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at) \
         VALUES ('{TENANT}', '{session_id}', '{SOURCE}', '{ADAPTER}', 'ext-{session_id}', 'Title {session_id}', 0, '{imported_at}', '{imported_at}')"
    )
}

fn turn_fixture(
    turn_id: &str,
    session_id: &str,
    turn_index: i64,
    _question_id: &str,
    started_at: Option<&str>,
    imported_at: &str,
) -> String {
    let started_at = match started_at {
        Some(value) => format!("'{value}'"),
        None => "NULL".to_string(),
    };
    format!(
        "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, prompt, started_at, fingerprint, missing, imported_at) \
         VALUES ('{TENANT}', '{turn_id}', '{session_id}', 'ext-{turn_id}', {turn_index}, 'prompt {turn_id}', {started_at}, 'fp-{turn_id}', 0, '{imported_at}')"
    )
}

fn web_turn_fixture(
    turn_id: &str,
    session_id: &str,
    turn_index: i64,
    _question_id: &str,
    started_at: Option<&str>,
    imported_at: &str,
) -> String {
    let started_at = match started_at {
        Some(value) => format!("'{value}'"),
        None => "NULL".to_string(),
    };
    format!(
        "INSERT INTO web_record_turns (tenant_id, id, session_id, external_id, turn_index, prompt, started_at, fingerprint, missing, imported_at) \
         VALUES ('{TENANT}', '{turn_id}', '{session_id}', 'ext-{turn_id}', {turn_index}, 'prompt {turn_id}', {started_at}, 'fp-{turn_id}', 0, '{imported_at}')"
    )
}

fn scope() -> MemoryScope {
    MemoryScope {
        app_id: None,
        source_id: None,
        project_path: None,
        session_id: None,
    }
}

async fn recall(
    pool: &SqlitePool,
    scope: &MemoryScope,
    since: Option<&str>,
    until: Option<&str>,
    limit: usize,
    offset: usize,
) -> (usize, Vec<MemoryRecallQuestionRef>) {
    list_memory_recall_question_refs_sqlx(pool, TENANT, scope, since, until, true, limit, offset)
        .await
        .expect("list recall refs")
}

#[tokio::test]
async fn recall_units_locate_by_first_turn_started_at_not_import_time() {
    let pool = create_test_pool().await;
    seed(
        &pool,
        &[
            &source_fixture(ADAPTER),
            &session_fixture("s-1", "2026-10-23T00:00:00Z"),
            &turn_fixture(
                "q-1",
                "s-1",
                0,
                "q-1",
                Some("2026-10-20T09:00:00Z"),
                "2026-10-23T00:00:00Z",
            ),
        ],
    )
    .await;

    // 导入于 10-23，但真实发生在 10-20：以 10-21 起算的窗口不应命中。
    let (total, refs) = recall(&pool, &scope(), Some("2026-10-21T00:00:00Z"), None, 10, 0).await;
    assert_eq!(total, 0);
    assert!(refs.is_empty());

    let (total, refs) = recall(&pool, &scope(), Some("2026-10-20T09:00:00Z"), None, 10, 0).await;
    assert_eq!(total, 1);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].question_id, "q-1");
}

#[tokio::test]
async fn recall_units_fall_back_to_import_time_and_count_each_group_once() {
    let pool = create_test_pool().await;
    seed(
        &pool,
        &[
            &source_fixture(ADAPTER),
            &session_fixture("s-1", "2026-10-22T10:00:00Z"),
            &turn_fixture("q-a", "s-1", 0, "q-a", None, "2026-10-22T10:00:00Z"),
            &turn_fixture(
                "q-b",
                "s-1",
                1,
                "q-b",
                Some("2026-10-22T11:00:00Z"),
                "2026-10-22T11:00:00Z",
            ),
            // 空白开始时间与缺失同义，同样回退到导入时间。
            &turn_fixture("q-c", "s-1", 2, "q-c", Some("   "), "2026-10-22T12:00:00Z"),
        ],
    )
    .await;

    let (total, refs) = recall(
        &pool,
        &scope(),
        Some("2026-10-22T00:00:00Z"),
        Some("2026-10-23T00:00:00Z"),
        10,
        0,
    )
    .await;
    // 三个分组各计一次，组内多轮不重复计入。
    assert_eq!(total, 3);
    assert_eq!(refs.len(), 3);
    assert_eq!(refs[0].question_id, "q-c");
    assert_eq!(refs[0].question_index, 2);
    assert_eq!(refs[1].question_id, "q-b");
    assert_eq!(refs[1].question_index, 1);
    assert_eq!(refs[2].question_id, "q-a");
    assert_eq!(refs[2].question_index, 0);

    // 落在导入时间之前一秒的窗口不应命中回退分组。
    let (total, refs) = recall(&pool, &scope(), None, Some("2026-10-22T09:59:59Z"), 10, 0).await;
    assert_eq!(total, 0);
    assert!(refs.is_empty());
}

#[tokio::test]
async fn recall_window_bounds_are_inclusive_on_both_ends() {
    let pool = create_test_pool().await;
    seed(
        &pool,
        &[
            &source_fixture(ADAPTER),
            &session_fixture("s-1", "2026-10-23T00:00:00Z"),
            &turn_fixture(
                "q-1",
                "s-1",
                0,
                "q-1",
                Some("2026-10-20T09:00:00Z"),
                "2026-10-23T00:00:00Z",
            ),
        ],
    )
    .await;

    let (total, _) = recall(
        &pool,
        &scope(),
        Some("2026-10-20T09:00:00Z"),
        Some("2026-10-20T09:00:00Z"),
        10,
        0,
    )
    .await;
    assert_eq!(total, 1, "边界值本身应命中");

    let (total, _) = recall(&pool, &scope(), Some("2026-10-20T09:00:01Z"), None, 10, 0).await;
    assert_eq!(total, 0, "窗口起点之后不命中");

    let (total, _) = recall(&pool, &scope(), None, Some("2026-10-20T08:59:59Z"), 10, 0).await;
    assert_eq!(total, 0, "窗口终点之前不命中");
}

#[tokio::test]
async fn recall_window_compares_normalized_utc_instants() {
    let pool = create_test_pool().await;
    seed(
        &pool,
        &[
            &source_fixture(ADAPTER),
            &session_fixture("s-utc", "2026-10-23T00:00:00Z"),
            &session_fixture("s-offset", "2026-10-23T00:00:00Z"),
            // 09:00+08:00 == 01:00Z
            &turn_fixture(
                "q-utc",
                "s-utc",
                0,
                "q-utc",
                Some("2026-10-20T09:00:00+08:00"),
                "2026-10-23T00:00:00Z",
            ),
            // 00:30+08:00 == 前一日 16:30Z
            &turn_fixture(
                "q-offset",
                "s-offset",
                0,
                "q-offset",
                Some("2026-10-20T00:30:00+08:00"),
                "2026-10-23T00:00:00Z",
            ),
        ],
    )
    .await;

    let (total, refs) = recall(
        &pool,
        &scope(),
        Some("2026-10-20T01:00:00Z"),
        Some("2026-10-20T01:00:00Z"),
        10,
        0,
    )
    .await;
    assert_eq!(total, 1);
    assert_eq!(refs[0].question_id, "q-utc");

    // 文本序会把 00:30+08:00 误判进 10-20 的窗口；按真实时刻它属于前一日。
    let (total, refs) = recall(
        &pool,
        &scope(),
        Some("2026-10-19T16:30:00Z"),
        Some("2026-10-19T16:30:00Z"),
        10,
        0,
    )
    .await;
    assert_eq!(total, 1);
    assert_eq!(refs[0].question_id, "q-offset");

    let (total, _) = recall(&pool, &scope(), Some("2026-10-20T00:00:00Z"), None, 10, 0).await;
    assert_eq!(total, 1, "偏移时刻归一后只有 s-utc 落在 10-20 之后");
}

#[tokio::test]
async fn recall_pagination_is_stable_for_same_second_units() {
    let pool = create_test_pool().await;
    seed(
        &pool,
        &[
            &source_fixture(ADAPTER),
            &session_fixture("s-b", "2026-10-23T00:00:00Z"),
            &session_fixture("s-a", "2026-10-23T00:00:00Z"),
            &session_fixture("s-c", "2026-10-23T00:00:00Z"),
            &turn_fixture(
                "t-b",
                "s-b",
                0,
                "q-b",
                Some("2026-10-20T09:00:00Z"),
                "2026-10-23T00:00:00Z",
            ),
            &turn_fixture(
                "t-a",
                "s-a",
                0,
                "q-a",
                Some("2026-10-20T09:00:00Z"),
                "2026-10-23T00:00:00Z",
            ),
            &turn_fixture("t-c", "s-c", 0, "q-c", None, "2026-10-20T09:00:00Z"),
        ],
    )
    .await;

    let (total, first) = recall(&pool, &scope(), None, None, 2, 0).await;
    assert_eq!(total, 3);
    let (_, second) = recall(&pool, &scope(), None, None, 2, 2).await;
    let (_, repeated) = recall(&pool, &scope(), None, None, 2, 0).await;

    let order = |refs: &[MemoryRecallQuestionRef]| {
        refs.iter()
            .map(|item| item.session_id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(order(&first), vec!["s-a".to_string(), "s-b".to_string()]);
    assert_eq!(order(&second), vec!["s-c".to_string()]);
    assert_eq!(
        order(&first),
        order(&repeated),
        "同一秒并列时分页顺序必须稳定"
    );
}

#[tokio::test]
async fn recall_enumerates_web_records_symmetrically_and_skips_them_for_project_scope() {
    let pool = create_test_pool().await;
    seed(
        &pool,
        &[
            &source_fixture(ADAPTER),
            &session_fixture("s-1", "2026-10-23T00:00:00Z"),
            &turn_fixture(
                "q-1",
                "s-1",
                0,
                "q-1",
                Some("2026-10-20T09:00:00Z"),
                "2026-10-23T00:00:00Z",
            ),
            &web_session_fixture("w-1", "2026-10-23T00:00:00Z"),
            &web_turn_fixture(
                "q-web",
                "w-1",
                0,
                "q-web",
                Some("2026-10-21T09:00:00Z"),
                "2026-10-23T00:00:00Z",
            ),
        ],
    )
    .await;

    let (total, refs) = recall(&pool, &scope(), None, None, 10, 0).await;
    assert_eq!(total, 2);
    assert_eq!(refs[0].record_kind, MemoryRecordKind::Web);
    assert_eq!(refs[0].question_id, "q-web");
    assert_eq!(refs[1].record_kind, MemoryRecordKind::Session);

    let project_scope = MemoryScope {
        project_path: Some("/path/project".to_string()),
        ..scope()
    };
    let (total, refs) = recall(&pool, &project_scope, None, None, 10, 0).await;
    assert_eq!(total, 1);
    assert_eq!(refs[0].session_id, "s-1");
}

#[tokio::test]
async fn load_recall_parts_derives_matching_question_id_for_both_session_and_web() {
    let pool = create_test_pool().await;
    seed(
        &pool,
        &[
            &source_fixture(ADAPTER),
            &session_fixture("s-parts", "2026-10-23T00:00:00Z"),
            &turn_fixture(
                "conversation-turn-part-001",
                "s-parts",
                0,
                "ignored",
                Some("2026-10-20T09:00:00Z"),
                "2026-10-23T00:00:00Z",
            ),
            "INSERT INTO conversation_parts (tenant_id, turn_id, part_index, id_key, kind, content, attrs_json) \
             VALUES ('default', 'conversation-turn-part-001', 0, X'0102030405060708', 'text', 'hello world', '{}')",
            &web_session_fixture("w-parts", "2026-10-23T00:00:00Z"),
            &web_turn_fixture(
                "web-record-turn-part-001",
                "w-parts",
                0,
                "ignored",
                Some("2026-10-21T09:00:00Z"),
                "2026-10-23T00:00:00Z",
            ),
            "INSERT INTO web_record_parts (tenant_id, turn_id, part_index, id_key, kind, content) \
             VALUES ('default', 'web-record-turn-part-001', 0, X'0102030405060708', 'answer', 'web response')",
        ],
    )
    .await;

    // 1. Session parts lookup by question id
    let parts = load_recall_parts_for_questions_sqlx(
        &pool,
        TENANT,
        MemoryRecordKind::Session,
        &["conversation-question-part-001".to_string()],
    )
    .await
    .expect("load session recall parts");
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].question_id, "conversation-question-part-001");
    assert_eq!(parts[0].text.as_deref(), Some("hello world"));

    // 2. Web parts lookup by question id
    let web_parts = load_recall_parts_for_questions_sqlx(
        &pool,
        TENANT,
        MemoryRecordKind::Web,
        &["web-record-question-part-001".to_string()],
    )
    .await
    .expect("load web recall parts");
    assert_eq!(web_parts.len(), 1);
    assert_eq!(web_parts[0].question_id, "web-record-question-part-001");
    assert_eq!(web_parts[0].text.as_deref(), Some("web response"));

    // 3. Question ref list derives matching question id
    let (_, refs) = recall(&pool, &scope(), None, None, 10, 0).await;
    let question_ids: Vec<_> = refs.iter().map(|r| r.question_id.as_str()).collect();
    assert!(question_ids.contains(&"conversation-question-part-001"));
    assert!(question_ids.contains(&"web-record-question-part-001"));
}
