//! 子代理会话与父轮次委派锚点的入库对账。
//!
//! 契约为 ADR-0022：
//! 1. 父会话交互轮次里派发子代理的位置只留一个 `kind = "subagent"` 的轻量锚点 Part，
//!    锚点通过 `metadata_json.child_external_id` 指向子会话的 `external_id`；
//! 2. 会话主键由 `stable_id("conversation-session", [source_id, external_id])` 纯函数派生，
//!    因此父锚点在入库时即可算出子会话主键，与文件扫描先后顺序无关；
//! 3. 父会话整包快照替换时，按"本次快照引用集合"对账清理不再被引用的孤儿子会话
//!    （父会话在宿主侧 `/rewind` 回滚后，被抹掉的派发不会留下幽灵子会话）。

use std::collections::BTreeSet;

use sqlx::{Sqlite, Transaction};

use crate::backend::domain::conversations::triad::{SplitTurnTriad, TurnProcessItem};
use crate::backend::domain::{stable_id, ConversationPartKind, NormalizedConversationPart};
use crate::backend::store::{StoreError, StoreResult};

/// 子代理会话在 `conversation_sessions.execution_origin` 上的取值。
pub(crate) const SUBAGENT_EXECUTION_ORIGIN: &str = "subagent";

/// 委派锚点 Part 在 `TurnProcess.items` 与零件属性上的语义类型。
pub(crate) const SUBAGENT_ANCHOR_KIND: &str = "subagent";

/// 内联子代理会话的虚拟定位器前缀。
///
/// 宿主把子代理流内联在主会话文件里时没有独立磁盘文件，`source_locator`
/// 用 `<父文件>#subagent:<id>` 表达；这类子会话一旦失去父引用就没有任何
/// 源数据可以重新读取，必须随父快照整包替换一起清理。
const INLINE_SUBAGENT_LOCATOR_MARKER: &str = "#subagent:";

/// 委派锚点携带的子代理元数据。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SubagentAnchorMeta {
    pub child_external_id: String,
    pub agent_role: Option<String>,
    pub title: Option<String>,
    pub status: Option<String>,
    pub model: Option<String>,
    pub step_count: Option<i64>,
    pub report_summary: Option<String>,
    pub spawn_anchor: Option<String>,
}

impl SubagentAnchorMeta {
    /// 从锚点 Part 的 `metadata_json` 解析；缺少 `child_external_id` 视为不是锚点。
    pub(crate) fn parse(metadata_json: Option<&str>) -> Option<Self> {
        let raw = metadata_json?.trim();
        if raw.is_empty() {
            return None;
        }
        let value: serde_json::Value = serde_json::from_str(raw).ok()?;
        let map = value.as_object()?;
        let child_external_id = map
            .get("child_external_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())?
            .to_string();
        Some(Self {
            child_external_id,
            agent_role: string_field(map.get("agent_role")),
            title: string_field(map.get("title")),
            status: string_field(map.get("status")),
            model: string_field(map.get("model")),
            step_count: map.get("step_count").and_then(|value| value.as_i64()),
            report_summary: string_field(map.get("report_summary")),
            spawn_anchor: string_field(map.get("spawn_anchor")),
        })
    }

    /// 子会话主键的确定性预计算。
    fn child_session_id(&self, source_id: &str) -> String {
        derive_child_session_id(source_id, &self.child_external_id)
    }
}

fn string_field(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// 由源标识与子会话 `external_id` 确定性派生子会话主键。
pub(crate) fn derive_child_session_id(source_id: &str, child_external_id: &str) -> String {
    stable_id("conversation-session", &[source_id, child_external_id])
}

/// 为父轮次补全委派锚点：把子会话主键写入零件属性与过程链，返回本次快照引用的子会话主键。
///
/// 锚点 Part 由适配器产出 `kind = "subagent"`，`split_turn_triad` 会把它留在
/// `parts` 里（`part_index` 即原始零件下标），这里只做"补算主键 + 挂过程节点"。
pub(crate) fn annotate_subagent_anchors(
    source_id: &str,
    parts: &[NormalizedConversationPart],
    triad: &mut SplitTurnTriad,
) -> Vec<String> {
    let mut referenced = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        if part.kind != ConversationPartKind::Subagent {
            continue;
        }
        let Some(meta) = SubagentAnchorMeta::parse(part.metadata_json.as_deref()) else {
            continue;
        };
        let child_session_id = meta.child_session_id(source_id);
        let part_index = index as i64;

        if let Some(retained) = triad
            .parts
            .iter_mut()
            .find(|retained| retained.part_index == part_index)
        {
            retained.attrs_json = Some(merge_anchor_attrs(
                retained.attrs_json.take(),
                &meta,
                &child_session_id,
            ));
        }

        let process = triad.process.get_or_insert_with(Default::default);
        if !process
            .items
            .iter()
            .any(|item| item.seq == part_index && item.kind == SUBAGENT_ANCHOR_KIND)
        {
            process.items.push(TurnProcessItem {
                seq: part_index,
                kind: SUBAGENT_ANCHOR_KIND.to_string(),
                label: meta.title.clone(),
                content: Some(child_session_id.clone()),
                signal: None,
                cwd: None,
                exit_code: None,
                summary: meta.report_summary.clone(),
                status: meta.status.clone(),
                translated_text: None,
            });
        }

        if !referenced.contains(&child_session_id) {
            referenced.push(child_session_id);
        }
    }
    referenced
}

fn merge_anchor_attrs(
    existing: Option<String>,
    meta: &SubagentAnchorMeta,
    child_session_id: &str,
) -> String {
    let mut map = existing
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    map.insert(
        "kind".to_string(),
        serde_json::Value::String(SUBAGENT_ANCHOR_KIND.to_string()),
    );
    map.insert(
        "semantic_role".to_string(),
        serde_json::Value::String(SUBAGENT_ANCHOR_KIND.to_string()),
    );
    map.insert(
        "renderer".to_string(),
        serde_json::Value::String("subagent_tree".to_string()),
    );
    map.insert(
        "child_session_id".to_string(),
        serde_json::Value::String(child_session_id.to_string()),
    );
    map.insert(
        "child_external_id".to_string(),
        serde_json::Value::String(meta.child_external_id.clone()),
    );
    for (key, value) in [
        ("agent_role", meta.agent_role.as_deref()),
        ("title", meta.title.as_deref()),
        ("status", meta.status.as_deref()),
        ("model", meta.model.as_deref()),
        ("report_summary", meta.report_summary.as_deref()),
        ("spawn_anchor", meta.spawn_anchor.as_deref()),
    ] {
        if let Some(value) = value {
            map.insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
    }
    if let Some(step_count) = meta.step_count {
        map.insert(
            "step_count".to_string(),
            serde_json::Value::Number(step_count.into()),
        );
    }
    serde_json::to_string(&serde_json::Value::Object(map)).unwrap_or_default()
}

/// 从已备好的快照里收集被引用的子会话主键。
pub(crate) fn collect_referenced_child_session_ids(
    prepared: &[super::snapshot::PreparedTurn],
) -> BTreeSet<String> {
    let mut referenced = BTreeSet::new();
    for entry in prepared {
        if let Some(raw) = entry.turn.process.as_deref() {
            if let Ok(process) = serde_json::from_str::<
                crate::backend::domain::conversations::triad::TurnProcess,
            >(raw)
            {
                for item in process.items {
                    if item.kind == SUBAGENT_ANCHOR_KIND {
                        if let Some(child) = item.content.filter(|value| !value.trim().is_empty()) {
                            referenced.insert(child);
                        }
                    }
                }
            }
        }
        for part in &entry.parts {
            let Some(raw) = part.attrs_json.as_deref() else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
                continue;
            };
            if let Some(child) = value
                .get("child_session_id")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                referenced.insert(child.to_string());
            }
        }
    }
    referenced
}

/// 孤儿子会话的对账结果，供调用方按需观测。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OrphanReconcileOutcome {
    /// 被级联删除（源数据已不存在）的子会话主键。
    pub deleted: Vec<String>,
    /// 保留为隐藏会话、仅解除父轮次绑定的子会话主键。
    pub unbound: Vec<String>,
}

/// 对账并清理父会话快照不再引用的孤儿子会话。
///
/// - 源数据已消失（`missing = 1`）或子会话本身是内联虚拟路径：
///   在同一事务内级联删除全文索引、内容事实、交互轮次与会话行；
/// - 独立磁盘文件仍在：只解除父轮次绑定，保留 `user_visible = 0` 的隐藏状态，
///   等它自己的文件被宿主清理时再由 missing 对账接管。
pub(crate) async fn reconcile_orphan_subagent_sessions_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    source_id: &str,
    parent_session_id: &str,
    referenced: &BTreeSet<String>,
) -> StoreResult<OrphanReconcileOutcome> {
    let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, i64)>(
        "SELECT id, external_id, execution_purpose, source_locator, missing \
         FROM conversation_sessions \
         WHERE tenant_id = ?1 AND source_id = ?2 AND execution_origin = ?3",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(SUBAGENT_EXECUTION_ORIGIN)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;

    let mut outcome = OrphanReconcileOutcome::default();
    for (child_id, external_id, purpose, locator, missing) in rows {
        if child_id == parent_session_id || referenced.contains(&child_id) {
            continue;
        }
        if parent_session_id_of(purpose.as_deref()).as_deref() != Some(parent_session_id) {
            // 归属其他父会话的子会话由它自己的父快照对账，不在本轮处理。
            continue;
        }
        if missing != 0 || is_inline_subagent_locator(locator.as_deref()) {
            delete_subagent_session_sqlx_tx(tx, tenant_id, source_id, &child_id, &external_id)
                .await?;
            outcome.deleted.push(child_id);
        } else {
            unbind_subagent_parent_turn_sqlx_tx(tx, tenant_id, &child_id, purpose.as_deref())
                .await?;
            outcome.unbound.push(child_id);
        }
    }
    Ok(outcome)
}

fn parent_session_id_of(execution_purpose: Option<&str>) -> Option<String> {
    let raw = execution_purpose?.trim();
    if raw.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    value
        .get("parent_session_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// 内联子代理会话没有独立磁盘文件，只能随父会话一起消失。
pub(crate) fn is_inline_subagent_locator(source_locator: Option<&str>) -> bool {
    match source_locator.map(str::trim) {
        None | Some("") => true,
        Some(locator) => {
            locator.contains(INLINE_SUBAGENT_LOCATOR_MARKER)
                || locator.starts_with("inline:")
                || locator.starts_with("subagent:")
        }
    }
}

async fn delete_subagent_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    source_id: &str,
    session_id: &str,
    external_id: &str,
) -> StoreResult<()> {
    for statement in [
        "DELETE FROM conversation_part_fts WHERE tenant_id = ?1 AND session_id = ?2",
        "DELETE FROM conversation_parts WHERE tenant_id = ?1 AND turn_id IN \
         (SELECT id FROM conversation_turns WHERE tenant_id = ?1 AND session_id = ?2)",
        "DELETE FROM conversation_turns WHERE tenant_id = ?1 AND session_id = ?2",
        "DELETE FROM conversation_sessions WHERE tenant_id = ?1 AND id = ?2",
    ] {
        sqlx::query(statement)
            .bind(tenant_id)
            .bind(session_id)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::external)?;
    }
    sqlx::query(
        "DELETE FROM conversation_session_observations \
         WHERE tenant_id = ?1 AND source_id = ?2 AND record_kind = 'session' AND external_id = ?3",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(external_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

async fn unbind_subagent_parent_turn_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    execution_purpose: Option<&str>,
) -> StoreResult<()> {
    let mut map = execution_purpose
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    // 只解除轮次绑定：`parent_session_id` 必须留着，否则子会话的磁盘文件随后被宿主
    // 清理时，父快照再也认不出这个孤儿，脏行会永久滞留。
    map.remove("parent_turn_index");
    let purpose = serde_json::to_string(&serde_json::Value::Object(map)).unwrap_or_default();
    sqlx::query(
        "UPDATE conversation_sessions SET execution_purpose = ?1, user_visible = 0 \
         WHERE tenant_id = ?2 AND id = ?3",
    )
    .bind(purpose)
    .bind(tenant_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

#[cfg(test)]
#[path = "subagent_tests.rs"]
mod tests;
