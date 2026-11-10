//! 会话快照整包替换（ADR-0023）。
//!
//! 对话适配器每次返回的都是单个会话的完整快照，宿主又普遍支持回滚与分支切换，
//! 因此落库粒度统一收口在会话级：版本未变直接短路，版本变化就在单事务里
//! 把该会话的内容事实与交互轮次整体清空后重写。
//!
//! 本层只写业务事实（`conversation_sessions` / `turns` / `parts`）。搜索索引是只读派生产物，
//! 由事实提交后的事件消费者异步推进；旋钮对账（missing / resurrect）属于冷路径，
//! 父快照替换也不承担子会话的物理清理——本地资产遵循保险箱语义。
//!
//! 轮次身份只表达"某会话的第几轮"（`sha256(session_id, turn_index)`），
//! 适配器提供的 `external_id` 降级为导入期的内存临时源键，仅用于解析子代理父子关系。

use std::collections::HashMap;

use sqlx::{Sqlite, Transaction};

use crate::backend::domain::conversations::resolve_turn_started_at;
use crate::backend::domain::conversations::triad::{
    merge_turn_triad, split_normalized_turn, TriadPart, TurnProcess,
};
use crate::backend::domain::{
    derive_part_id, derive_part_id_key, derive_turn_id, ConversationPart, ConversationRecordKind,
    ConversationSession, ConversationTurn, NormalizedConversationTurn,
};
use crate::backend::store::{StoreError, StoreResult};

use super::subagent::{annotate_subagent_anchors, SUBAGENT_EXECUTION_ORIGIN};

/// 两族记录在物理表名、零件列结构与主键前缀上的唯一映射。
///
/// 快照写入与启动期修复钩子共用同一份定义，避免两处各写一遍再慢慢漂移。
#[derive(Debug, Clone, Copy)]
pub(crate) struct RecordTables {
    pub(crate) turns: &'static str,
    pub(crate) parts: &'static str,
    pub(crate) parts_have_attrs: bool,
    pub(crate) turn_id_prefix: &'static str,
    pub(crate) part_id_prefix: &'static str,
}

impl ConversationRecordKind {
    pub(crate) fn record_tables(self) -> RecordTables {
        match self {
            ConversationRecordKind::Session => RecordTables {
                turns: "conversation_turns",
                parts: "conversation_parts",
                parts_have_attrs: true,
                turn_id_prefix: "conversation-turn",
                part_id_prefix: "conversation-part",
            },
            ConversationRecordKind::Web => RecordTables {
                turns: "web_record_turns",
                parts: "web_record_parts",
                parts_have_attrs: false,
                turn_id_prefix: "web-record-turn",
                part_id_prefix: "web-record-part",
            },
        }
    }
}

/// 一个已定位身份的轮次及其拆解出的内容事实。
#[derive(Debug, Clone)]
pub(crate) struct PreparedTurn {
    pub turn: ConversationTurn,
    pub parts: Vec<TriadPart>,
}

/// 把一次会话快照准备成可直接落库的轮次序列。
///
/// 纯内存同步计算：过滤空白轮次、按数组顺序重排 `turn_index = 0..N`、算出位置主键，
/// 再第二遍拆解三元组与内容事实；父轮次里的子代理锚点在此补算确定性 `child_session_id`。
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_session_snapshot(
    kind: ConversationRecordKind,
    source_id: &str,
    session_id: &str,
    adapter_id: &str,
    execution_origin: Option<&str>,
    session_started_at: Option<&str>,
    session_updated_at: Option<&str>,
    turns: &[NormalizedConversationTurn],
    now: &str,
) -> Vec<PreparedTurn> {
    let tables = kind.record_tables();
    let valid_turns = turns
        .iter()
        .filter(|turn| !turn.user_text.trim().is_empty())
        .collect::<Vec<_>>();

    let mut source_key_to_turn_id = HashMap::<String, String>::new();
    let mut turn_ids = Vec::with_capacity(valid_turns.len());
    for (index, turn) in valid_turns.iter().enumerate() {
        let turn_id = derive_turn_id(tables.turn_id_prefix, session_id, index as i64);
        if let Some(key) = turn.source_key() {
            source_key_to_turn_id
                .entry(key.to_string())
                .or_insert_with(|| turn_id.clone());
        }
        turn_ids.push(turn_id);
    }

    let split_adapter_id = match kind {
        ConversationRecordKind::Session => adapter_id,
        ConversationRecordKind::Web => "web",
    };
    // 子代理会话自身也是"1 问 = 1 轮"，但它的首轮把逐轮工具链与最终汇报压进了同一轮，
    // 汇报之后可能还跟着一次工具执行，和三段式"取最后一条助手输出"的规则不一致。
    let is_subagent_session =
        matches!(execution_origin, Some(origin) if origin == SUBAGENT_EXECUTION_ORIGIN);
    let detect_anchors = matches!(kind, ConversationRecordKind::Session);

    let mut prepared = Vec::with_capacity(valid_turns.len());
    for (index, turn) in valid_turns.into_iter().enumerate() {
        let turn_id = turn_ids[index].clone();
        let turn_index = index as i64;
        let mut triad = split_normalized_turn(split_adapter_id, &turn.user_text, &turn.parts);
        if detect_anchors {
            annotate_subagent_anchors(source_id, &turn.parts, &mut triad);
        }
        if (is_subagent_session || turn.is_subagent())
            && matches!(kind, ConversationRecordKind::Session)
        {
            triad.promote_trailing_assistant_output(&turn.parts);
        }

        let parent_turn_id = turn
            .parent_external_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .and_then(|key| source_key_to_turn_id.get(key).cloned());
        let title = turn.title.clone();
        let turn_kind = turn.turn_kind.clone();
        let agent_role = turn.agent_role.clone();

        if title.is_some()
            || turn_kind.is_some()
            || agent_role.is_some()
            || parent_turn_id.is_some()
        {
            let process = triad.process.get_or_insert_with(TurnProcess::default);
            if title.is_some() {
                process.title = title.clone();
            }
            if turn_kind.is_some() {
                process.turn_kind = turn_kind.clone();
            }
            if agent_role.is_some() {
                process.agent_role = agent_role.clone();
            }
            if parent_turn_id.is_some() {
                process.parent_turn_id = parent_turn_id.clone();
            }
        }

        // 落库前把 cwd 折成便携锚点。放在这里而不是 `split` 内部，是因为 `split` 里有
        // 「多个命令的 cwd 相同才提升到顶层」的字符串比较，折叠若提前介入会改变该判断。
        if let Some(process) = triad.process.as_mut() {
            fold_turn_process_paths(process);
        }
        let process_json = triad
            .process
            .as_ref()
            .filter(|process| !process.is_empty())
            .map(|process| serde_json::to_string(process).unwrap_or_default());
        let started_at = resolve_turn_started_at(
            turn.started_at.as_deref(),
            session_updated_at,
            session_started_at,
            now,
        );

        prepared.push(PreparedTurn {
            turn: ConversationTurn {
                id: turn_id,
                session_id: session_id.to_string(),
                turn_index,
                prompt: triad.prompt,
                title,
                started_at,
                ended_at: turn.ended_at.clone(),
                imported_at: now.to_string(),
                model: turn.model.clone(),
                process: process_json,
                response: triad.response,
                turn_kind,
                agent_role,
                parent_turn_id,
            },
            parts: fold_part_attrs_paths(triad.parts),
        });
    }
    prepared
}

/// 在单事务内完成一个会话的整包替换。
///
/// 顺序固定为：清内容事实 → 清交互轮次 → 批量写新快照。派生索引与子会话清理不在此边界内：
/// 前者由事实提交后的消费者异步推进，后者属于冷路径的显式对账。
/// 调用方负责先 `UPSERT` 会话主表行与成功版本凭据。
pub(crate) async fn replace_session_snapshot_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    kind: ConversationRecordKind,
    session: &ConversationSession,
    prepared: &[PreparedTurn],
) -> StoreResult<()> {
    let tables = kind.record_tables();

    let delete_parts = format!(
        "DELETE FROM {parts} WHERE tenant_id = ?1 AND turn_id IN \
         (SELECT id FROM {turns} WHERE tenant_id = ?1 AND session_id = ?2)",
        parts = tables.parts,
        turns = tables.turns,
    );
    sqlx::query(sqlx::AssertSqlSafe(delete_parts))
        .bind(tenant_id)
        .bind(&session.id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;

    let delete_turns = format!(
        "DELETE FROM {turns} WHERE tenant_id = ?1 AND session_id = ?2",
        turns = tables.turns,
    );
    sqlx::query(sqlx::AssertSqlSafe(delete_turns))
        .bind(tenant_id)
        .bind(&session.id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;

    for entry in prepared {
        insert_turn_sqlx_tx(tx, tenant_id, tables, &entry.turn).await?;
        insert_parts_sqlx_tx(tx, tenant_id, tables, &entry.turn.id, &entry.parts).await?;
    }

    Ok(())
}

/// 把某一轮次当前的提交事实还原成完整 Part 序列（供检索语料抽取使用）。
///
/// 事实表只存三段式与中间零件，命令语义标签与文件路径散落在 `process` / `attrs_json` 里，
/// 因此语料抽取必须复用这套还原逻辑，而不是另写一份读法。
#[allow(clippy::too_many_arguments)]
pub(crate) fn restore_turn_parts(
    adapter_id: &str,
    part_id_prefix: &str,
    turn_id: &str,
    prompt: &str,
    process_json: Option<&str>,
    response: Option<&str>,
    parts: &[TriadPart],
) -> Vec<ConversationPart> {
    let process: Option<TurnProcess> =
        process_json.and_then(|json| serde_json::from_str(json).ok());
    merge_turn_triad(
        adapter_id,
        part_id_prefix,
        turn_id,
        prompt,
        process.as_ref(),
        response,
        parts,
    )
}

async fn insert_turn_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    tables: RecordTables,
    turn: &ConversationTurn,
) -> StoreResult<()> {
    let insert = format!(
        "INSERT INTO {turns} (\
            tenant_id, id, session_id, turn_index, prompt, process, response, \
            started_at, ended_at, model, imported_at\
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        turns = tables.turns,
    );
    sqlx::query(sqlx::AssertSqlSafe(insert))
        .bind(tenant_id)
        .bind(&turn.id)
        .bind(&turn.session_id)
        .bind(turn.turn_index)
        .bind(&turn.prompt)
        .bind(&turn.process)
        .bind(&turn.response)
        .bind(&turn.started_at)
        .bind(&turn.ended_at)
        .bind(&turn.model)
        .bind(&turn.imported_at)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    Ok(())
}

async fn insert_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    tables: RecordTables,
    turn_id: &str,
    parts: &[TriadPart],
) -> StoreResult<()> {
    for part in parts {
        let part_id = derive_part_id(tables.part_id_prefix, turn_id, part.part_index);
        let id_key = derive_part_id_key(&part_id).unwrap_or([0u8; 8]);
        if tables.parts_have_attrs {
            let insert = format!(
                "INSERT INTO {parts} (\
                    tenant_id, turn_id, part_index, id_key, kind, content, attrs_json, translated_text\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                parts = tables.parts,
            );
            sqlx::query(sqlx::AssertSqlSafe(insert))
                .bind(tenant_id)
                .bind(turn_id)
                .bind(part.part_index)
                .bind(id_key.as_slice())
                .bind(&part.kind)
                .bind(&part.content)
                .bind(&part.attrs_json)
                .bind(&part.translated_text)
                .execute(&mut **tx)
                .await
                .map_err(StoreError::external)?;
        } else {
            let insert = format!(
                "INSERT INTO {parts} (\
                    tenant_id, turn_id, part_index, id_key, kind, content, translated_text\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                parts = tables.parts,
            );
            sqlx::query(sqlx::AssertSqlSafe(insert))
                .bind(tenant_id)
                .bind(turn_id)
                .bind(part.part_index)
                .bind(id_key.as_slice())
                .bind(&part.kind)
                .bind(&part.content)
                .bind(part.translated_text.clone())
                .execute(&mut **tx)
                .await
                .map_err(StoreError::external)?;
        }
    }
    Ok(())
}

/// 把 `TurnProcess` 及其 `items` 上的 `cwd` 折叠为便携锚点。
///
/// 契约见 ADR-0020。`process` 是 `conversation_turns` / `web_record_turns` 里唯一
/// 承载结构化路径的 JSON 列，折叠只在写库前这一步发生，读取侧按原样投影给前端，
/// 由展示层还原 `~`。命令输出（`items[].content`）里的路径属于记录原貌，不在此处理。
fn fold_turn_process_paths(process: &mut TurnProcess) {
    process.cwd = fold_cwd(process.cwd.as_deref());
    for item in &mut process.items {
        item.cwd = fold_cwd(item.cwd.as_deref());
    }
}

fn fold_cwd(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(
        crate::backend::store::path_codec::normalize_path_for_storage(trimmed)
            .unwrap_or_else(|_| trimmed.to_string()),
    )
}

/// 把中间零件属性里的 `file_path` 折叠为便携锚点。
///
/// `attrs_json` 由 adapter 原样透传，其中 `file_path` 表示工具调用涉及的文件。
/// 前端只用它做展示与复制、不做文件 I/O，因此存储侧折叠即可，无需读取侧展开。
fn fold_part_attrs_paths(parts: Vec<TriadPart>) -> Vec<TriadPart> {
    parts
        .into_iter()
        .map(|mut part| {
            part.attrs_json = fold_part_attrs(part.attrs_json);
            part
        })
        .collect()
}

fn fold_part_attrs(raw: Option<String>) -> Option<String> {
    let source = raw?;
    // 快速短路：只有带 `file_path` 的零件才需要解析。不能按 `/Users/` 之类的主机前缀判断，
    // 否则 Linux 的 `/home/...` 会被漏掉。
    if !source.contains("\"file_path\"") {
        return Some(source);
    }
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&source) else {
        return Some(source);
    };
    let Some(map) = value.as_object_mut() else {
        return Some(source);
    };
    let Some(path) = map
        .get("file_path")
        .and_then(|value| value.as_str())
        .map(str::to_string)
    else {
        return Some(source);
    };
    let folded = crate::backend::store::path_codec::normalize_path_for_storage(&path)
        .unwrap_or_else(|_| path.clone());
    if folded == path {
        return Some(source);
    }
    map.insert("file_path".to_string(), serde_json::Value::String(folded));
    Some(serde_json::to_string(&value).unwrap_or(source))
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
