//! 存量 `subagent` / `continuation` 伪轮次的原地数据折算（ADR-0022）。
//!
//! ADR-0022 之前，一次人类提问只要派发 $K$ 个子代理，就会在 `conversation_turns`
//! 里裂成 $2K + 1$ 行伪轮次：`turn_kind = "subagent"` 承载整段委派 Prompt，
//! 紧随其后的 `turn_kind = "continuation"` 承载主代理的续接动作与最终回答。
//! 结果是第 2 次提问的物理轮次号被迫偏移到 `#T8`，顶层大纲被数千字委派提示词刷屏。
//!
//! 本模块把这批存量数据**原地**折算回 ADR-0022 的模型，零 DDL、不读任何源文件：
//! 1. 抽离：每个 `subagent` 伪轮次落成一个独立的 `Subagent Session`（`user_visible = 0`、
//!    `execution_origin = "subagent"`、`execution_purpose` 记父子溯源），其 `#T1` 完整保留
//!    委派 Prompt、工具链与最终汇报；
//! 2. 锚定：在所属人类轮次里追加一个 `kind = "subagent"` 的轻量委派锚点（Part + 过程节点），
//!    只留 `[委派任务]` 与 `[汇报结论]` 摘要，并绑定确定性算出的 `child_session_id`；
//! 3. 缝合：把 `continuation` 伪轮次的过程节点与零件按序并回人类轮次，最后一个非空
//!    `response` 提升为人类轮次的最终回答，伪轮次行物理删除；
//! 4. 重排：父会话剩余真实人类轮次重排 `turn_index = 0..N`、重算位置主键并重建零件索引。
//!
//! 幂等性来自"折算后不再含伪轮次"这一事实本身：重跑只会扫到 0 个候选会话。
//! 另在 `app_settings` 写一行标记（`settings_id = conversation.subagent_session_fold_v1`），
//! 记录当时的数据代际（会话数 + 最新导入时刻 + 源修订号）：代际没变就零扫描短路，
//! 变了说明标记之后又有会话落库，会自动补折算一次，且同一父会话的子代理序号继续往后排，
//! 不会覆盖上一次抽离的子会话。
//!
//! 刻意不做的两件事：
//! - **不迁移用量事件**：`conversation_usage_events.turn_id` 是宿主原始 `turn-<序号>`，
//!   而位置重排恰好把轮次号还原成宿主的真实人类轮次序号，折算后归因只会更准；
//! - **不改记忆水位**：现行 `memory_session_watermarks` 以 `source_updated_at` 为水位，
//!   不再记录"已处理轮次数"，轮次折算不改变会话更新时间，因此无需修正。

use std::collections::BTreeMap;

use chrono::Utc;
use sqlx::{FromRow, SqlitePool};

use crate::backend::domain::conversations::triad::{TriadPart, TurnProcess};
use crate::backend::domain::ConversationSession;

use super::conversations::snapshot::replace_session_snapshot_sqlx_tx;
use super::conversations::{
    bump_conversation_search_source_revision_sqlx_tx, map_sqlx_conversation_session,
    update_conversation_session_last_cwd_sqlx_tx, upsert_conversation_session_sqlx_tx,
};
use super::database::table_exists;
use super::{StoreError, StoreResult};

#[path = "conversation_subagent_fold_build.rs"]
mod build;
use build::FoldContext;

/// 存量续接伪轮次的固定 Prompt 文案。
const CONTINUATION_PROMPT: &str = "Continue following subagent execution";

/// 一次性折算标记：写在 `app_settings` 的独立行，不与会话设置（`settings_id = "global"`）混放。
const FOLD_MARKER_SETTINGS_ID: &str = "conversation.subagent_session_fold_v1";

/// 折算执行的观测结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SubagentFoldReport {
    /// 命中一次性标记，本次未做任何扫描。
    pub skipped_already_done: bool,
    pub folded_session_count: usize,
    pub folded_subagent_session_count: usize,
    pub folded_pseudo_turn_count: usize,
    /// 从顶层列表收敛为隐藏子代理会话的存量行数。
    pub converged_subagent_session_count: usize,
    /// 折算失败的会话数；非零时不会写标记，下次启动重试。
    pub failed_session_count: usize,
}

/// 折算完成时的数据代际，用于判断标记之后是否又有新会话落库。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct FoldDataGeneration {
    session_count: i64,
    latest_imported_at: String,
    source_revisions: std::collections::BTreeMap<String, i64>,
}

/// 一次性折算标记的内容。
///
/// `data_generation` 是短路判据：新会话导入会改变会话数 / 最新导入时间 / 源修订号，
/// 所以标记后若又有旧格式会话落库（例如尚未改造完的适配器），下一次启动会自动补折算。
/// 稳态下只查这两张小表，不扫描 `conversation_turns`，符合"零开销短路"。
/// 首版标记没有该字段（`None`），视为需要补跑一次以升级标记格式。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct FoldMarker {
    status: String,
    folded_sessions: usize,
    folded_subagent_sessions: usize,
    folded_pseudo_turns: usize,
    #[serde(default)]
    converged_subagent_sessions: usize,
    completed_at: String,
    #[serde(default)]
    data_generation: Option<FoldDataGeneration>,
}

/// 存量伪轮次折算的唯一入口。
pub(crate) async fn fold_legacy_subagent_pseudo_turns(
    pool: &SqlitePool,
) -> StoreResult<SubagentFoldReport> {
    if !table_exists(pool, "conversation_turns").await? {
        return Ok(SubagentFoldReport::default());
    }
    let marker = load_fold_marker(pool).await?;
    if fold_already_done(pool, marker.as_ref()).await? {
        return Ok(SubagentFoldReport {
            skipped_already_done: true,
            ..Default::default()
        });
    }

    let mut report = SubagentFoldReport::default();
    let mut changed_tenants = Vec::new();
    for (tenant_id, session_id) in load_candidate_sessions(pool).await? {
        match fold_one_session(pool, &tenant_id, &session_id).await {
            Ok(Some(outcome)) => {
                report.folded_session_count += 1;
                report.folded_subagent_session_count += outcome.child_count;
                report.folded_pseudo_turn_count += outcome.pseudo_turn_count;
                if !changed_tenants.contains(&tenant_id) {
                    changed_tenants.push(tenant_id);
                }
            }
            Ok(None) => {}
            Err(error) => {
                // 单会话失败不能拖垮整轮升级；不写标记，下次启动再试。
                report.failed_session_count += 1;
                tracing::warn!(
                    session = %session_id,
                    error = %error,
                    "failed to fold legacy subagent pseudo turns"
                );
            }
        }
    }

    report.converged_subagent_session_count = converge_flat_subagent_sessions(pool).await?;

    for tenant_id in &changed_tenants {
        let mut tx = pool.begin().await.map_err(StoreError::external)?;
        bump_conversation_search_source_revision_sqlx_tx(&mut *tx, tenant_id).await?;
        tx.commit().await.map_err(StoreError::external)?;
    }

    if report.failed_session_count == 0 {
        write_fold_marker(pool, &report).await?;
    }
    Ok(report)
}

async fn load_fold_marker(pool: &SqlitePool) -> StoreResult<Option<FoldMarker>> {
    if !table_exists(pool, "app_settings").await? {
        return Ok(None);
    }
    let raw: Option<String> =
        sqlx::query_scalar("SELECT settings_json FROM app_settings WHERE settings_id = ?1")
            .bind(FOLD_MARKER_SETTINGS_ID)
            .fetch_optional(pool)
            .await
            .map_err(StoreError::external)?;
    Ok(raw
        .as_deref()
        .and_then(|value| serde_json::from_str::<FoldMarker>(value).ok()))
}

async fn fold_already_done(pool: &SqlitePool, marker: Option<&FoldMarker>) -> StoreResult<bool> {
    let Some(marker) = marker else {
        return Ok(false);
    };
    if marker.status != "done" {
        return Ok(false);
    }
    let Some(recorded) = marker.data_generation.as_ref() else {
        // 首版标记只写了 status；补跑一次，顺便把数据代际写进标记。
        return Ok(false);
    };
    Ok(*recorded == current_data_generation(pool).await?)
}

async fn current_data_generation(pool: &SqlitePool) -> StoreResult<FoldDataGeneration> {
    let (session_count, latest_imported_at): (i64, Option<String>) =
        sqlx::query_as("SELECT COUNT(*), MAX(imported_at) FROM conversation_sessions")
            .fetch_one(pool)
            .await
            .map_err(StoreError::external)?;
    let mut source_revisions = std::collections::BTreeMap::<String, i64>::new();
    if table_exists(pool, "conversation_search_index_state").await? {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT tenant_id, source_revision FROM conversation_search_index_state",
        )
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
        source_revisions.extend(rows);
    }
    Ok(FoldDataGeneration {
        session_count,
        latest_imported_at: latest_imported_at.unwrap_or_default(),
        source_revisions,
    })
}

async fn write_fold_marker(pool: &SqlitePool, report: &SubagentFoldReport) -> StoreResult<()> {
    if !table_exists(pool, "app_settings").await? {
        return Ok(());
    }
    let payload = FoldMarker {
        status: "done".to_string(),
        folded_sessions: report.folded_session_count,
        folded_subagent_sessions: report.folded_subagent_session_count,
        folded_pseudo_turns: report.folded_pseudo_turn_count,
        converged_subagent_sessions: report.converged_subagent_session_count,
        completed_at: Utc::now().to_rfc3339(),
        // 必须在会话插入与源修订号推进之后取，否则会把本轮自己造成的变更算成"新数据"。
        data_generation: Some(current_data_generation(pool).await?),
    };
    sqlx::query(
        "INSERT INTO app_settings (settings_id, schema_version, settings_json, updated_at) \
         VALUES (?1, 1, ?2, ?3) \
         ON CONFLICT(settings_id) DO UPDATE SET \
            schema_version = excluded.schema_version, \
            settings_json = excluded.settings_json, \
            updated_at = excluded.updated_at",
    )
    .bind(FOLD_MARKER_SETTINGS_ID)
    .bind(serde_json::to_string(&payload).map_err(StoreError::external)?)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

/// 收敛"已能识别为子代理、却仍以普通顶层会话平铺"的存量行。
///
/// 存量 Codex 派生会话的 `external_id` 形如 `sess_subagent_agent_<uuid>`，
/// 更早的适配器把它们按普通会话写成了 `execution_origin = "user"`、`user_visible = 1`；
/// 另一类则由 `execution_purpose` 里的父子溯源或 `#subagent:` 内联定位器标出。
/// 识别条件必须严格：只写"标题/ID 里出现过 subagent"会误伤父会话
/// （例如 `session-subagent-outline` 其实是主会话）。
/// 这里只改两个标记列，不动任何内容事实，重复执行零写入。
async fn converge_flat_subagent_sessions(pool: &SqlitePool) -> StoreResult<usize> {
    let result = sqlx::query(
        "UPDATE conversation_sessions \
         SET user_visible = 0, execution_origin = 'subagent' \
         WHERE user_visible = 1 \
           AND execution_origin <> 'subagent' \
           AND ( \
                lower(external_id) LIKE '%subagent_agent_%' \
                OR COALESCE(execution_purpose, '') LIKE '%\"parent_session_id\"%' \
                OR COALESCE(execution_purpose, '') LIKE '%\"parent_external_id\"%' \
                OR COALESCE(source_locator, '') LIKE '%#subagent:%' \
           )",
    )
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(usize::try_from(result.rows_affected()).unwrap_or(usize::MAX))
}

/// 候选会话：`process` 里出现过 `turn_kind`，或 Prompt 命中续接文案。
///
/// 刻意不用 `json_extract` 过滤：存量 `process` 里可能有非法 JSON，一条坏行就会让
/// 整条查询报 "malformed JSON"。这里先用 LIKE 粗筛，分类交给 Rust。
async fn load_candidate_sessions(pool: &SqlitePool) -> StoreResult<Vec<(String, String)>> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT DISTINCT t.tenant_id, t.session_id \
         FROM conversation_turns t \
         JOIN conversation_sessions s ON s.tenant_id = t.tenant_id AND s.id = t.session_id \
         WHERE t.process LIKE '%\"turn_kind\"%' OR trim(t.prompt) = ?1 \
         ORDER BY t.tenant_id, t.session_id",
    )
    .bind(CONTINUATION_PROMPT)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)
}

struct FoldOutcome {
    child_count: usize,
    pseudo_turn_count: usize,
}

async fn fold_one_session(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<Option<FoldOutcome>> {
    let Some(parent) = load_session(pool, tenant_id, session_id).await? else {
        return Ok(None);
    };
    let turns = load_turns(pool, tenant_id, session_id).await?;
    if !turns.iter().any(|turn| turn.class.is_pseudo()) {
        return Ok(None);
    }

    let now = Utc::now().to_rfc3339();
    let ordinal_start =
        next_subagent_ordinal(pool, tenant_id, &parent.source_id, &parent.external_id).await?;
    let folded = FoldContext {
        parent: &parent,
        now: &now,
    }
    .fold_turns(turns, ordinal_start);
    if folded.parent_turns.is_empty() {
        return Ok(None);
    }

    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    for child in &folded.children {
        upsert_conversation_session_sqlx_tx(&mut tx, tenant_id, &child.session).await?;
        update_conversation_session_last_cwd_sqlx_tx(
            &mut tx,
            tenant_id,
            &child.session.id,
            child.session.last_cwd.as_deref(),
        )
        .await?;
        replace_session_snapshot_sqlx_tx(
            &mut tx,
            tenant_id,
            crate::backend::domain::ConversationRecordKind::Session,
            &child.session,
            std::slice::from_ref(&child.turn),
        )
        .await?;
    }
    replace_session_snapshot_sqlx_tx(
        &mut tx,
        tenant_id,
        crate::backend::domain::ConversationRecordKind::Session,
        &parent,
        &folded.parent_turns,
    )
    .await?;
    // 折算改写了父会话与全部子会话的事实，但这条路径不经过同步导入、也不会发
    // Outbox 事件。不推进修订号的话，派生索引会一直停留在折算前的快照上，且没人
    // 知道它已经过期。这里只做"标记落后"：检索立刻切到事实回退，孤儿文档由下一次
    // 重建清理——绝不在事实事务里写索引（ADR-0023）。
    super::conversations::bump_conversation_search_source_revision_sqlx_tx(&mut *tx, tenant_id)
        .await?;
    tx.commit().await.map_err(StoreError::external)?;

    Ok(Some(FoldOutcome {
        child_count: folded.children.len(),
        pseudo_turn_count: folded.pseudo_turn_count,
    }))
}

/// 已折算过的父会话若又落进新的伪轮次，子会话序号必须从存量最大值续号，
/// 否则会重算出 `legacy-1` 并覆盖上一次抽离子会话。
async fn next_subagent_ordinal(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
    parent_external_id: &str,
) -> StoreResult<i64> {
    let prefix = format!("{parent_external_id}:subagent:legacy-");
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT external_id FROM conversation_sessions \
         WHERE tenant_id = ?1 AND source_id = ?2 AND external_id LIKE ?3 || '%'",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&prefix)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let max_ordinal = rows
        .iter()
        .filter_map(|external_id| external_id.strip_prefix(&prefix))
        .filter_map(|ordinal| ordinal.parse::<i64>().ok())
        .max()
        .unwrap_or(0);
    Ok(max_ordinal + 1)
}

async fn load_session(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<Option<ConversationSession>> {
    let row = sqlx::query(
        "SELECT id, source_id, adapter_id, external_id, title, project_path, started_at, \
                updated_at, source_locator, source_fingerprint, missing, created_at, imported_at, \
                execution_origin, execution_purpose, user_visible, last_cwd \
         FROM conversation_sessions WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?;
    row.as_ref().map(map_sqlx_conversation_session).transpose()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TurnClass {
    Human,
    Subagent,
    Continuation,
}

impl TurnClass {
    fn is_pseudo(self) -> bool {
        !matches!(self, Self::Human)
    }
}

fn classify_turn(turn_kind: Option<&str>, prompt: &str) -> TurnClass {
    match turn_kind {
        Some("subagent") | Some("agent") => TurnClass::Subagent,
        Some("continuation") | Some("main_continuation") => TurnClass::Continuation,
        _ if prompt.trim() == CONTINUATION_PROMPT => TurnClass::Continuation,
        _ => TurnClass::Human,
    }
}

#[derive(Debug, FromRow)]
struct LegacyTurnRow {
    id: String,
    prompt: String,
    process: Option<String>,
    response: Option<String>,
    started_at: String,
    ended_at: Option<String>,
    model: Option<String>,
    imported_at: String,
}

#[derive(Debug)]
struct LoadedTurn {
    class: TurnClass,
    row: LegacyTurnRow,
    process: TurnProcess,
    parts: Vec<TriadPart>,
}

async fn load_turns(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<Vec<LoadedTurn>> {
    let rows = sqlx::query_as::<_, LegacyTurnRow>(
        "SELECT id, turn_index, prompt, process, response, started_at, ended_at, model, imported_at \
         FROM conversation_turns WHERE tenant_id = ?1 AND session_id = ?2 \
         ORDER BY turn_index ASC, id ASC",
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    let part_rows: Vec<(
        String,
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT p.turn_id, p.part_index, p.kind, p.content, p.attrs_json, p.translated_text \
             FROM conversation_turns t \
             CROSS JOIN conversation_parts p ON p.tenant_id = t.tenant_id AND p.turn_id = t.id \
             WHERE t.tenant_id = ?1 AND t.session_id = ?2 \
             ORDER BY p.turn_id ASC, p.part_index ASC",
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    let mut parts_by_turn = BTreeMap::<String, Vec<TriadPart>>::new();
    for (turn_id, part_index, kind, content, attrs_json, translated_text) in part_rows {
        parts_by_turn.entry(turn_id).or_default().push(TriadPart {
            part_index,
            kind,
            content,
            attrs_json,
            translated_text,
        });
    }

    Ok(rows
        .into_iter()
        .map(|row| {
            let process = row
                .process
                .as_deref()
                .and_then(|raw| serde_json::from_str::<TurnProcess>(raw).ok())
                .unwrap_or_default();
            let class = classify_turn(process.turn_kind.as_deref(), &row.prompt);
            let parts = parts_by_turn.remove(&row.id).unwrap_or_default();
            LoadedTurn {
                class,
                row,
                process,
                parts,
            }
        })
        .collect())
}

#[cfg(test)]
#[path = "conversation_subagent_fold_tests.rs"]
mod tests;
