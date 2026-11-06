//! 会话快照整包替换。
//!
//! 对话适配器每次返回的都是单个会话的完整快照，宿主又普遍支持回滚与分支切换，
//! 因此落库粒度统一收口在会话级：版本未变直接短路，版本变化就在单事务里
//! 把该会话的全文索引、内容事实与交互轮次整体清空后重写。
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
    derive_part_id, derive_part_id_key, derive_turn_id, ConversationCardKindDefinition,
    ConversationPart, ConversationRecordKind, ConversationSession, ConversationTurn,
    NormalizedConversationTurn,
};
use crate::backend::store::{StoreError, StoreResult};

use super::conversation_repo::part_index::part_index_text;

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
/// 再第二遍拆解三元组与内容事实并把子代理的临时父键解析成父轮次主键。
pub(crate) fn prepare_session_snapshot(
    kind: ConversationRecordKind,
    session_id: &str,
    adapter_id: &str,
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

    let mut prepared = Vec::with_capacity(valid_turns.len());
    for (index, turn) in valid_turns.into_iter().enumerate() {
        let turn_id = turn_ids[index].clone();
        let turn_index = index as i64;
        let mut triad = split_normalized_turn(split_adapter_id, &turn.user_text, &turn.parts);
        if turn.is_subagent() && matches!(kind, ConversationRecordKind::Session) {
            // 子代理轮次合并了逐轮内容，最终汇报可能落在尾随的工具执行之前；
            // 与主轮次同源，仍按 Triad 约定把最后一段助手输出写进 response。
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
            parts: triad.parts,
        });
    }
    prepared
}

/// 在单事务内完成一个会话的整包替换。
///
/// 顺序固定为：清该会话全文索引 → 清内容事实 → 清交互轮次 → 批量写新快照 →
/// 直接复用内存中的轮次与内容事实投影索引行。调用方负责先 `UPSERT` 会话主表行。
pub(crate) async fn replace_session_snapshot_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    kind: ConversationRecordKind,
    session: &ConversationSession,
    prepared: &[PreparedTurn],
) -> StoreResult<()> {
    let tables = kind.record_tables();

    sqlx::query("DELETE FROM conversation_part_fts WHERE tenant_id = ?1 AND session_id = ?2")
        .bind(tenant_id)
        .bind(&session.id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;

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

    let card_kinds = load_adapter_card_kinds_sqlx_tx(tx, tenant_id, &session.adapter_id).await?;
    for entry in prepared {
        project_turn_part_index_sqlx_tx(
            tx,
            tenant_id,
            tables,
            &session.id,
            &session.adapter_id,
            &entry.turn,
            &entry.parts,
            &card_kinds,
        )
        .await?;
    }
    Ok(())
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

async fn load_adapter_card_kinds_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    adapter_id: &str,
) -> StoreResult<Vec<ConversationCardKindDefinition>> {
    let card_kinds_json: Option<String> = sqlx::query_scalar(
        "SELECT card_kinds_json FROM conversation_adapters WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(adapter_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    match card_kinds_json {
        Some(json) => Ok(serde_json::from_str(&json).map_err(StoreError::external)?),
        None => Ok(Vec::new()),
    }
}

/// 复用内存中的轮次与内容事实直接投影索引正文，不再按轮次回读数据库。
#[allow(clippy::too_many_arguments)]
async fn project_turn_part_index_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    tables: RecordTables,
    session_id: &str,
    adapter_id: &str,
    turn: &ConversationTurn,
    parts: &[TriadPart],
    card_kinds: &[ConversationCardKindDefinition],
) -> StoreResult<()> {
    let process: Option<TurnProcess> = turn
        .process
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok());
    let restored = merge_turn_triad(
        adapter_id,
        tables.part_id_prefix,
        &turn.id,
        &turn.prompt,
        process.as_ref(),
        turn.response.as_deref(),
        parts,
    );
    for part in restored {
        let Some(text) = part_index_text(&part, adapter_id, card_kinds)? else {
            continue;
        };
        insert_part_index_row_sqlx_tx(tx, tenant_id, &part, session_id, &text).await?;
    }
    Ok(())
}

async fn insert_part_index_row_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    part: &ConversationPart,
    session_id: &str,
    text: &str,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_part_fts (tenant_id, part_id, session_id, turn_id, text)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(tenant_id)
    .bind(&part.id)
    .bind(session_id)
    .bind(&part.turn_id)
    .bind(text)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
