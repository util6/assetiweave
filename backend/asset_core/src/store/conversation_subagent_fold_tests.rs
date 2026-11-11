use super::*;
use crate::backend::domain::derive_turn_id;
use crate::backend::store::{load_conversation_session_detail_sqlx, open_migrated_pool, Database};
use uuid::Uuid;

const TENANT: &str = "default";
const ADAPTER: &str = "legacy-adapter";
const SOURCE: &str = "legacy-source";
const PARENT_EXTERNAL_ID: &str = "parent-session";

fn temp_db_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "assetiweave-subagent-fold-{tag}-{}.sqlite",
        Uuid::new_v4()
    ))
}

fn cleanup_database(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
}

/// 一条存量伪轮次的物理描述，按 `turn_index` 顺序喂给 fixture。
struct LegacyTurnFixture {
    suffix: &'static str,
    prompt: &'static str,
    process: serde_json::Value,
    response: Option<&'static str>,
}

fn subagent_process(title: &str, role: &str, commands: &[(&str, &str)]) -> serde_json::Value {
    let items: Vec<serde_json::Value> = commands
        .iter()
        .enumerate()
        .map(|(index, (content, summary))| {
            serde_json::json!({
                "seq": index,
                "kind": "codex.command",
                "content": content,
                "summary": summary,
                "status": "completed",
            })
        })
        .collect();
    serde_json::json!({
        "title": title,
        "turn_kind": "subagent",
        "agent_role": role,
        "items": items,
    })
}

fn continuation_process() -> serde_json::Value {
    serde_json::json!({
        "turn_kind": "continuation",
        "items": [{
            "seq": 0,
            "kind": "codex.command",
            "content": "git status --short",
            "summary": "检查工作区",
            "status": "completed",
        }],
    })
}

async fn seed_legacy_parent(pool: &sqlx::SqlitePool, turns: &[LegacyTurnFixture]) -> Vec<String> {
    sqlx::query(
        "INSERT INTO conversation_adapters (tenant_id, id, name, kind, version, enabled, \
            trust_state, capabilities, input_kinds, created_at, updated_at) \
         VALUES (?1, ?2, ?2, 'external', '1.0.0', 1, 'trusted', '[]', '[]', ?3, ?3)",
    )
    .bind(TENANT)
    .bind(ADAPTER)
    .bind("2026-11-01T00:00:00Z")
    .execute(pool)
    .await
    .expect("insert adapter");

    sqlx::query(
        "INSERT INTO conversation_sources (tenant_id, id, adapter_id, name, kind, location, \
            enabled, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 'Legacy source', 'directory', '/tmp/legacy', 1, ?4, ?4)",
    )
    .bind(TENANT)
    .bind(SOURCE)
    .bind(ADAPTER)
    .bind("2026-11-01T00:00:00Z")
    .execute(pool)
    .await
    .expect("insert source");

    sqlx::query(
        "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, \
            title, project_path, started_at, updated_at, missing, created_at, imported_at, \
            execution_origin, user_visible, last_cwd) \
         VALUES (?1, 'parent-session-id', ?2, ?3, ?4, 'Parent', '~/work/demo', ?5, ?5, 0, ?5, ?5, \
            'user', 1, '~/work/demo')",
    )
    .bind(TENANT)
    .bind(SOURCE)
    .bind(ADAPTER)
    .bind(PARENT_EXTERNAL_ID)
    .bind("2026-11-01T00:00:00Z")
    .execute(pool)
    .await
    .expect("insert parent session");

    let mut turn_ids = Vec::new();
    for (index, fixture) in turns.iter().enumerate() {
        let turn_id = format!("legacy-turn-{}", fixture.suffix);
        sqlx::query(
            "INSERT INTO conversation_turns (tenant_id, id, session_id, turn_index, prompt, \
                process, response, started_at, ended_at, model, imported_at) \
             VALUES (?1, ?2, 'parent-session-id', ?3, ?4, ?5, ?6, ?7, ?8, 'gpt-5', ?7)",
        )
        .bind(TENANT)
        .bind(&turn_id)
        .bind(index as i64)
        .bind(fixture.prompt)
        .bind(serde_json::to_string(&fixture.process).expect("serialize process"))
        .bind(fixture.response)
        .bind(format!("2026-11-01T00:0{index}:00Z"))
        .bind(format!("2026-11-01T00:0{index}:30Z"))
        .execute(pool)
        .await
        .expect("insert turn");
        turn_ids.push(turn_id);
    }
    // 建库时钩子已经跑过一次并落了标记；清掉它，模拟"标记出现之前的存量库"。
    clear_fold_marker(pool).await;
    turn_ids
}

async fn clear_fold_marker(pool: &sqlx::SqlitePool) {
    sqlx::query("DELETE FROM app_settings WHERE settings_id = ?1")
        .bind(FOLD_MARKER_SETTINGS_ID)
        .execute(pool)
        .await
        .expect("clear fold marker");
}

async fn insert_part(
    pool: &sqlx::SqlitePool,
    turn_id: &str,
    part_index: i64,
    kind: &str,
    content: &str,
) {
    sqlx::query(
        "INSERT INTO conversation_parts (tenant_id, turn_id, part_index, id_key, kind, content) \
         VALUES (?1, ?2, ?3, X'0000000000000000', ?4, ?5)",
    )
    .bind(TENANT)
    .bind(turn_id)
    .bind(part_index)
    .bind(kind)
    .bind(content)
    .execute(pool)
    .await
    .expect("insert part");
}

/// 8 个物理轮次的存量会话：1 次提问被 3 个子代理与 3 个续接轮切成 7 段，第 2 次提问被迫偏到 `#T8`。
fn legacy_acceptance_turns() -> Vec<LegacyTurnFixture> {
    vec![
        LegacyTurnFixture {
            suffix: "human-1",
            prompt: "第一次提问：请落地父工单",
            process: serde_json::json!({}),
            response: None,
        },
        LegacyTurnFixture {
            suffix: "subagent-a",
            prompt: "You are teamwork_orchestrator. 落地父工单 #159。",
            process: subagent_process(
                "Teamwork Orchestra",
                "teamwork_orchestrator",
                &[("cargo test -p asset_core", "跑后端测试")],
            ),
            response: Some("编排完成：3 个子工单已关闭。"),
        },
        LegacyTurnFixture {
            suffix: "continuation-1",
            prompt: CONTINUATION_PROMPT,
            process: continuation_process(),
            response: Some("过渡回答：子代理 A 已完成。"),
        },
        LegacyTurnFixture {
            suffix: "subagent-b",
            prompt: "You are teamwork_explorer. 调查会话折叠。",
            process: subagent_process(
                "Codebase Investigation",
                "teamwork_explorer",
                &[("rg -n legacy_fold", "检索折叠实现")],
            ),
            response: Some("调查报告：仅在存储层有折算入口。"),
        },
        LegacyTurnFixture {
            suffix: "continuation-2",
            prompt: CONTINUATION_PROMPT,
            process: continuation_process(),
            response: None,
        },
        LegacyTurnFixture {
            suffix: "subagent-c",
            prompt: "You are teamwork_reviewer. 复查折算正确性。",
            process: subagent_process(
                "Code Reviewer",
                "teamwork_reviewer",
                &[("cargo test -p asset_core -- fold", "跑折算测试")],
            ),
            response: Some("复查结论：幂等通过。"),
        },
        LegacyTurnFixture {
            suffix: "continuation-3",
            prompt: CONTINUATION_PROMPT,
            process: continuation_process(),
            response: Some("最终回答：父工单已完整落地。"),
        },
        LegacyTurnFixture {
            suffix: "human-2",
            prompt: "第二次提问：帮我补一份回归测试",
            process: serde_json::json!({}),
            response: Some("第二次回答。"),
        },
    ]
}

fn anchor_parts(
    parts: &[crate::backend::domain::ConversationPart],
) -> Vec<&crate::backend::domain::ConversationPart> {
    parts
        .iter()
        .filter(|part| part.kind == crate::backend::domain::ConversationPartKind::Subagent)
        .collect()
}

#[tokio::test]
async fn folds_legacy_pseudo_turns_into_anchors_and_subagent_sessions() {
    let db_path = temp_db_path("acceptance");
    let database = Database::open_async(&db_path).await.expect("open database");
    let pool = database.pool();
    let turn_ids = seed_legacy_parent(pool, &legacy_acceptance_turns()).await;
    // 子代理 A 有留存零件；B 完全没拆出 parts（存量库里有 61 个这样的轮次）；C 只有过程节点。
    insert_part(pool, &turn_ids[1], 1, "codex.reasoning", "先读一下父工单").await;
    insert_part(pool, &turn_ids[2], 1, "codex.result", "命令输出：ok").await;
    insert_part(pool, &turn_ids[6], 1, "codex.result", "最终命令输出").await;

    let report = fold_legacy_subagent_pseudo_turns(pool)
        .await
        .expect("fold legacy turns");
    assert_eq!(report.folded_session_count, 1, "只有含伪轮次的会话会被折算");
    assert_eq!(report.folded_subagent_session_count, 3);
    assert_eq!(report.folded_pseudo_turn_count, 6);
    assert_eq!(report.failed_session_count, 0);

    // 1. 父会话从 8 个物理轮次收敛为 2 个真实人类轮次。
    let detail = load_conversation_session_detail_sqlx(pool, TENANT, "parent-session-id")
        .await
        .expect("load parent detail");
    let parent_turns: Vec<_> = detail
        .questions
        .iter()
        .flat_map(|question| question.turns.iter())
        .collect();
    assert_eq!(parent_turns.len(), 2, "1 问 = 1 轮");
    assert_eq!(parent_turns[0].turn_index, 0);
    assert_eq!(parent_turns[1].turn_index, 1);
    assert_eq!(parent_turns[0].prompt, "第一次提问：请落地父工单");
    assert_eq!(parent_turns[1].prompt, "第二次提问：帮我补一份回归测试");
    assert!(
        parent_turns
            .iter()
            .all(|turn| turn.turn_kind.is_none() && turn.parent_turn_id.is_none()),
        "折算后的父轮次不得残留伪轮次标记"
    );
    assert_eq!(
        parent_turns[0].response.as_deref(),
        Some("最终回答：父工单已完整落地。"),
        "最后一个续接轮的回答必须提升为父轮次的正式回答"
    );
    assert_eq!(
        parent_turns[0].id,
        derive_turn_id("conversation-turn", "parent-session-id", 0)
    );

    // 2. 父轮次内嵌 3 个委派锚点，且各自绑定有效的子会话主键。
    let first_turn_parts = detail
        .questions
        .iter()
        .find(|question| question.turns.iter().any(|turn| turn.turn_index == 0))
        .map(|question| question.parts.as_slice())
        .expect("first question");
    let anchors = anchor_parts(first_turn_parts);
    assert_eq!(anchors.len(), 3, "3 个子代理各留一个锚点");
    let mut child_ids = Vec::new();
    for (index, anchor) in anchors.iter().enumerate() {
        let attrs: serde_json::Value = sqlx::query_scalar(
            "SELECT attrs_json FROM conversation_parts WHERE tenant_id = ?1 AND turn_id = ?2 \
             AND part_index = ?3",
        )
        .bind(TENANT)
        .bind(&parent_turns[0].id)
        .bind(anchor.part_index)
        .fetch_one(pool)
        .await
        .map(|raw: Option<String>| {
            serde_json::from_str(raw.as_deref().unwrap_or("{}")).expect("anchor attrs")
        })
        .expect("load anchor attrs");
        let child_id = attrs
            .get("child_session_id")
            .and_then(|value| value.as_str())
            .expect("anchor must carry child_session_id")
            .to_string();
        assert!(child_id.starts_with("conversation-session-"));
        assert_eq!(
            attrs
                .get("child_external_id")
                .and_then(|value| value.as_str()),
            Some(format!("{PARENT_EXTERNAL_ID}:subagent:legacy-{}", index + 1).as_str())
        );
        assert_eq!(
            attrs.get("spawn_anchor").and_then(|value| value.as_str()),
            Some("#T1")
        );
        assert!(anchor
            .text
            .as_deref()
            .is_some_and(|text| text.contains("[委派任务]") && text.contains("[汇报结论]")));
        assert_eq!(
            anchor.content_card.as_ref().map(|card| card.kind.as_str()),
            Some("subagent")
        );
        child_ids.push(child_id);
    }
    assert_eq!(
        child_ids
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3,
        "锚点必须指向 3 个互不相同的子会话"
    );

    // 3. 每个子会话都是隐藏的独立会话，第 1 轮完整保留委派 Prompt / 工具链 / 汇报。
    let expected_prompts = [
        "You are teamwork_orchestrator. 落地父工单 #159。",
        "You are teamwork_explorer. 调查会话折叠。",
        "You are teamwork_reviewer. 复查折算正确性。",
    ];
    let expected_responses = [
        "编排完成：3 个子工单已关闭。",
        "调查报告：仅在存储层有折算入口。",
        "复查结论：幂等通过。",
    ];
    for (index, child_id) in child_ids.iter().enumerate() {
        let child = load_conversation_session_detail_sqlx(pool, TENANT, child_id)
            .await
            .expect("load child detail");
        assert!(!child.session.user_visible, "子代理会话不得出现在顶层列表");
        assert_eq!(child.session.execution_origin, "subagent");
        assert_eq!(child.session.source_id, SOURCE);
        assert_eq!(child.session.project_path.as_deref(), Some("~/work/demo"));
        assert_eq!(child.session.last_cwd.as_deref(), Some("~/work/demo"));
        let origin: serde_json::Value = serde_json::from_str(
            child
                .session
                .execution_purpose
                .as_deref()
                .expect("child execution_purpose"),
        )
        .expect("origin json");
        assert_eq!(
            origin
                .get("parent_session_id")
                .and_then(|value| value.as_str()),
            Some("parent-session-id")
        );

        let child_turns: Vec<_> = child
            .questions
            .iter()
            .flat_map(|question| question.turns.iter())
            .collect();
        assert_eq!(child_turns.len(), 1, "子会话只有 #T1");
        assert_eq!(child_turns[0].turn_index, 0);
        assert_eq!(child_turns[0].prompt, expected_prompts[index]);
        assert_eq!(
            child_turns[0].response.as_deref(),
            Some(expected_responses[index])
        );
        assert!(
            child_turns[0].turn_kind.is_none(),
            "子会话首轮不再标记为伪 subagent 轮次"
        );
        let child_parts: Vec<_> = child
            .questions
            .iter()
            .flat_map(|question| question.parts.iter())
            .collect();
        assert!(
            child_parts.iter().any(|part| {
                part.command.as_deref().is_some_and(|command| {
                    command.contains("cargo test") || command.contains("rg -n")
                })
            }),
            "子会话必须保留工具调用链"
        );
    }

    // 4. 顶层会话列表只返回父会话。
    let sessions = crate::backend::store::list_conversation_sessions_filtered_sqlx(
        pool,
        TENANT,
        crate::backend::store::ConversationSessionListFilter {
            limit: 50,
            ..Default::default()
        },
    )
    .await
    .expect("list sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session.id, "parent-session-id");
    assert_eq!(sessions[0].turn_count, 2);

    // 5. 折算只改事实：父会话留下真实人类提问（turn_count == 2），
    //    每个子会话保留自己的轮次与零件；派生索引由消费者按需重算，
    //    折算本身不写任何索引（ADR-0023）。
    for child_id in &child_ids {
        let child_turns: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM conversation_turns WHERE tenant_id = ?1 AND session_id = ?2",
        )
        .bind(TENANT)
        .bind(child_id)
        .fetch_one(pool)
        .await
        .expect("count child turns");
        assert!(child_turns > 0, "子会话必须保留自己的轮次: {child_id}");
    }

    // 6. 幂等：抹掉一次性标记后重跑，轮次主键、子会话数量与零件数量完全不变。
    let snapshot = snapshot_shape(pool).await;
    clear_fold_marker(pool).await;
    let second = fold_legacy_subagent_pseudo_turns(pool)
        .await
        .expect("second fold");
    assert_eq!(second.folded_session_count, 0);
    assert_eq!(second.folded_subagent_session_count, 0);
    assert_eq!(snapshot_shape(pool).await, snapshot);

    drop(database);
    cleanup_database(&db_path);
}

#[derive(Debug, PartialEq, Eq)]
struct Shape {
    sessions: Vec<(String, i64, String, i64)>,
    turns: Vec<(String, String, i64)>,
    parts: Vec<(String, i64, String)>,
}

async fn snapshot_shape(pool: &sqlx::SqlitePool) -> Shape {
    let sessions = sqlx::query_as::<_, (String, i64, String, i64)>(
        "SELECT id, user_visible, execution_origin, missing FROM conversation_sessions \
         WHERE tenant_id = ?1 ORDER BY id",
    )
    .bind(TENANT)
    .fetch_all(pool)
    .await
    .expect("shape sessions");
    let turns = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT id, session_id, turn_index FROM conversation_turns \
         WHERE tenant_id = ?1 ORDER BY id",
    )
    .bind(TENANT)
    .fetch_all(pool)
    .await
    .expect("shape turns");
    let parts = sqlx::query_as::<_, (String, i64, String)>(
        "SELECT turn_id, part_index, kind FROM conversation_parts \
         WHERE tenant_id = ?1 ORDER BY turn_id, part_index",
    )
    .bind(TENANT)
    .fetch_all(pool)
    .await
    .expect("shape parts");
    Shape {
        sessions,
        turns,
        parts,
    }
}

#[tokio::test]
async fn fold_is_short_circuited_by_the_once_marker() {
    let db_path = temp_db_path("marker");
    let database = Database::open_async(&db_path).await.expect("open database");
    let pool = database.pool();
    seed_legacy_parent(pool, &legacy_acceptance_turns()).await;

    let first = fold_legacy_subagent_pseudo_turns(pool)
        .await
        .expect("first fold");
    assert_eq!(first.folded_session_count, 1);

    let second = fold_legacy_subagent_pseudo_turns(pool)
        .await
        .expect("second fold");
    assert!(second.skipped_already_done);
    assert_eq!(second.folded_session_count, 0);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn marker_replays_when_new_legacy_turns_arrive_after_the_first_fold() {
    let db_path = temp_db_path("late-arrival");
    let database = Database::open_async(&db_path).await.expect("open database");
    let pool = database.pool();
    seed_legacy_parent(pool, &legacy_acceptance_turns()).await;
    let first = fold_legacy_subagent_pseudo_turns(pool)
        .await
        .expect("first fold");
    assert_eq!(first.folded_subagent_session_count, 3);

    // 模拟标记写入后又有一次旧格式导入：会话导入时刻推进，并带进两个新伪轮次。
    sqlx::query(
        "UPDATE conversation_sessions SET imported_at = ?1 \
         WHERE tenant_id = ?2 AND id = 'parent-session-id'",
    )
    // 子会话在本轮折算时用的是当前时间，因此模拟值必须晚于它才能推进 MAX(imported_at)。
    .bind("2999-01-01T00:00:00Z")
    .bind(TENANT)
    .execute(pool)
    .await
    .expect("advance imported_at");
    for (index, suffix, kind) in [
        (2, "late-subagent", "subagent"),
        (3, "late-continuation", "continuation"),
    ] {
        let process = serde_json::json!({
            "title": "Late arrival",
            "turn_kind": kind,
            "agent_role": "late_worker",
            "items": [],
        });
        sqlx::query(
            "INSERT INTO conversation_turns (tenant_id, id, session_id, turn_index, prompt, \
                process, response, started_at, imported_at) \
             VALUES (?1, ?2, 'parent-session-id', ?3, ?4, ?5, ?6, ?7, ?7)",
        )
        .bind(TENANT)
        .bind(suffix)
        .bind(index)
        .bind(if kind == "continuation" {
            CONTINUATION_PROMPT
        } else {
            "迟到的一次委派"
        })
        .bind(serde_json::to_string(&process).expect("serialize process"))
        .bind(if kind == "continuation" {
            Some("迟到的最终回答")
        } else {
            Some("迟到的汇报")
        })
        .bind("2026-11-02T00:00:00Z")
        .execute(pool)
        .await
        .expect("insert late turn");
    }

    let replay = fold_legacy_subagent_pseudo_turns(pool)
        .await
        .expect("replay fold");
    assert!(
        !replay.skipped_already_done,
        "数据代际变化后必须补折算，不能被一次性标记挡住"
    );
    assert_eq!(replay.folded_session_count, 1);
    assert_eq!(replay.folded_subagent_session_count, 1);

    let child_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_sessions \
         WHERE tenant_id = ?1 AND execution_origin = 'subagent'",
    )
    .bind(TENANT)
    .fetch_one(pool)
    .await
    .expect("count child sessions");
    assert_eq!(
        child_count, 4,
        "旧子会话必须保留，迟到的委派从 legacy-4 继续编号"
    );
    let legacy_four: Option<String> = sqlx::query_scalar(
        "SELECT external_id FROM conversation_sessions \
         WHERE tenant_id = ?1 AND external_id = ?2",
    )
    .bind(TENANT)
    .bind(format!("{PARENT_EXTERNAL_ID}:subagent:legacy-4"))
    .fetch_optional(pool)
    .await
    .expect("lookup legacy-4 child");
    assert!(legacy_four.is_some(), "新子会话不得覆盖旧子会话主键");

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn first_generation_marker_without_data_generation_is_replayed_once() {
    let db_path = temp_db_path("legacy-marker");
    let database = Database::open_async(&db_path).await.expect("open database");
    let pool = database.pool();
    seed_legacy_parent(pool, &legacy_acceptance_turns()).await;
    sqlx::query(
        "INSERT INTO app_settings (settings_id, schema_version, settings_json, updated_at) \
         VALUES (?1, 1, ?2, ?3) \
         ON CONFLICT(settings_id) DO UPDATE SET settings_json = excluded.settings_json",
    )
    .bind(FOLD_MARKER_SETTINGS_ID)
    .bind(
        r#"{"status":"done","folded_sessions":1,"folded_subagent_sessions":0,"folded_pseudo_turns":0,"completed_at":"2026-11-01T00:00:00Z"}"#,
    )
    .bind("2026-11-01T00:00:00Z")
    .execute(pool)
    .await
    .expect("write first-generation marker");

    let report = fold_legacy_subagent_pseudo_turns(pool)
        .await
        .expect("fold with legacy marker");
    assert!(!report.skipped_already_done);
    assert_eq!(report.folded_session_count, 1);
    let marker: String =
        sqlx::query_scalar("SELECT settings_json FROM app_settings WHERE settings_id = ?1")
            .bind(FOLD_MARKER_SETTINGS_ID)
            .fetch_one(pool)
            .await
            .expect("load upgraded marker");
    let upgraded: serde_json::Value = serde_json::from_str(&marker).expect("parse marker");
    assert!(
        upgraded.get("data_generation").is_some(),
        "旧标记补跑后必须升级为代际标记"
    );

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn session_starting_with_a_pseudo_turn_still_folds() {
    let db_path = temp_db_path("orphan-first");
    let database = Database::open_async(&db_path).await.expect("open database");
    let pool = database.pool();
    seed_legacy_parent(
        pool,
        &[
            LegacyTurnFixture {
                suffix: "subagent-only",
                prompt: "You are teamwork_worker. 从首轮就开始委派。",
                process: subagent_process("Worker", "teamwork_worker", &[]),
                response: Some("首轮委派完成。"),
            },
            LegacyTurnFixture {
                suffix: "continuation-only",
                prompt: CONTINUATION_PROMPT,
                process: continuation_process(),
                response: Some("主代理收尾。"),
            },
        ],
    )
    .await;

    let report = fold_legacy_subagent_pseudo_turns(pool)
        .await
        .expect("fold orphan-first session");
    assert_eq!(report.folded_session_count, 1);
    assert_eq!(report.folded_subagent_session_count, 1);

    let detail = load_conversation_session_detail_sqlx(pool, TENANT, "parent-session-id")
        .await
        .expect("load parent detail");
    let turns: Vec<_> = detail
        .questions
        .iter()
        .flat_map(|question| question.turns.iter())
        .collect();
    assert_eq!(turns.len(), 1, "伪轮次不额外制造人类轮次");
    assert!(
        turns[0].prompt.is_empty(),
        "首轮即伪轮次时宿主轮次没有真实提问"
    );
    assert_eq!(turns[0].response.as_deref(), Some("主代理收尾。"));

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn flat_subagent_sessions_are_hidden_and_marked_as_subagent() {
    let db_path = temp_db_path("flat-subagent");
    let database = Database::open_async(&db_path).await.expect("open database");
    let pool = database.pool();
    seed_legacy_parent(
        pool,
        &[LegacyTurnFixture {
            suffix: "plain",
            prompt: "普通提问",
            process: serde_json::json!({}),
            response: Some("普通回答"),
        }],
    )
    .await;
    sqlx::query(
        "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, \
            title, missing, created_at, imported_at, execution_origin, user_visible) \
         VALUES (?1, 'flat-subagent-session', ?2, ?3, 'sess_subagent_agent_legacy', \
            'Legacy flat subagent', 0, '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z', \
            'user', 1)",
    )
    .bind(TENANT)
    .bind(SOURCE)
    .bind(ADAPTER)
    .execute(pool)
    .await
    .expect("insert flat subagent session");
    // 名字里带 subagent 的父会话不能被误伤（CLI outline 单缝测试的真实夹具形态）。
    sqlx::query(
        "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, \
            title, missing, created_at, imported_at, execution_origin, user_visible) \
         VALUES (?1, 'outline-parent-session', ?2, ?3, 'session-subagent-outline', \
            'Subagent outline parent', 0, '2026-11-01T00:00:00Z', \
            '2026-11-01T00:00:00Z', 'user', 1)",
    )
    .bind(TENANT)
    .bind(SOURCE)
    .bind(ADAPTER)
    .execute(pool)
    .await
    .expect("insert decoy parent session");

    let report = fold_legacy_subagent_pseudo_turns(pool)
        .await
        .expect("converge flat subagent");
    assert_eq!(report.converged_subagent_session_count, 1);
    let (origin, visible): (String, i64) = sqlx::query_as(
        "SELECT execution_origin, user_visible FROM conversation_sessions \
         WHERE tenant_id = ?1 AND id = 'flat-subagent-session'",
    )
    .bind(TENANT)
    .fetch_one(pool)
    .await
    .expect("load converged session");
    assert_eq!(origin, "subagent");
    assert_eq!(visible, 0);

    let visible_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_sessions WHERE tenant_id = ?1 AND user_visible = 1",
    )
    .bind(TENANT)
    .fetch_one(pool)
    .await
    .expect("count visible sessions");
    assert_eq!(
        visible_count, 2,
        "顶层列表保留父会话与同名父会话，不泄漏子代理会话"
    );
    let decoy: (String, i64) = sqlx::query_as(
        "SELECT execution_origin, user_visible FROM conversation_sessions \
         WHERE tenant_id = ?1 AND id = 'outline-parent-session'",
    )
    .bind(TENANT)
    .fetch_one(pool)
    .await
    .expect("load decoy parent");
    assert_eq!(decoy, ("user".to_string(), 1), "父会话不能被误判为子代理");

    clear_fold_marker(pool).await;
    let second = fold_legacy_subagent_pseudo_turns(pool)
        .await
        .expect("second converge");
    assert_eq!(second.converged_subagent_session_count, 0, "收敛必须幂等");

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sessions_without_pseudo_turns_are_left_untouched() {
    let db_path = temp_db_path("noop");
    let database = Database::open_async(&db_path).await.expect("open database");
    let pool = database.pool();
    seed_legacy_parent(
        pool,
        &[
            LegacyTurnFixture {
                suffix: "plain-1",
                prompt: "普通提问一",
                process: serde_json::json!({}),
                response: Some("回答一"),
            },
            LegacyTurnFixture {
                suffix: "plain-2",
                prompt: "普通提问二",
                process: serde_json::json!({}),
                response: Some("回答二"),
            },
        ],
    )
    .await;
    let before = snapshot_shape(pool).await;

    let report = fold_legacy_subagent_pseudo_turns(pool)
        .await
        .expect("fold noop");
    assert_eq!(report.folded_session_count, 0);
    assert_eq!(snapshot_shape(pool).await, before);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn startup_hook_folds_legacy_rows_once() {
    let db_path = temp_db_path("startup");
    {
        let database = Database::open_async(&db_path).await.expect("open database");
        seed_legacy_parent(database.pool(), &legacy_acceptance_turns()).await;
        database.pool().close().await;
    }

    let pool = open_migrated_pool(&db_path).await.expect("reopen database");
    let turn_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_turns WHERE tenant_id = ?1 AND session_id = 'parent-session-id'",
    )
    .bind(TENANT)
    .fetch_one(&pool)
    .await
    .expect("count parent turns");
    assert_eq!(turn_count, 2, "启动期钩子必须完成折算");
    let marker: Option<String> =
        sqlx::query_scalar("SELECT settings_json FROM app_settings WHERE settings_id = ?1")
            .bind(FOLD_MARKER_SETTINGS_ID)
            .fetch_optional(&pool)
            .await
            .expect("load marker");
    assert!(marker.is_some(), "折算完成后必须写入一次性标记");
    pool.close().await;

    cleanup_database(&db_path);
}
