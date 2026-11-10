//! 子代理会话与父轮次委派锚点的入库对账。
//!
//! 契约为 ADR-0022：
//! 1. 父会话交互轮次里派发子代理的位置只留一个 `kind = "subagent"` 的轻量锚点 Part，
//!    锚点通过 `metadata_json.child_external_id` 指向子会话的 `external_id`；
//! 2. 会话主键由 `stable_id("conversation-session", [source_id, external_id])` 纯函数派生，
//!    因此父锚点在入库时即可算出子会话主键，与文件扫描先后顺序无关；
//! 3. 父会话整包快照替换只覆盖自己的事实，不对子会话做物理清理（ADR-0023 数据保险箱语义）。

use crate::backend::domain::conversations::triad::{SplitTurnTriad, TurnProcessItem};
use crate::backend::domain::{stable_id, ConversationPartKind, NormalizedConversationPart};

/// 子代理会话在 `conversation_sessions.execution_origin` 上的取值。
pub(crate) const SUBAGENT_EXECUTION_ORIGIN: &str = "subagent";

/// 委派锚点 Part 在 `TurnProcess.items` 与零件属性上的语义类型。
pub(crate) const SUBAGENT_ANCHOR_KIND: &str = "subagent";

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
///
/// 本路径下 `part_index` 与 `process.seq` 同源相等；但存量折算产物里两者
/// **各占一个序号**（见 `conversation_subagent_fold_build::append_anchor`），
/// 所以任何消费方都不得按序号对齐，只能按 `child_session_id` 对齐。
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

#[cfg(test)]
#[path = "subagent_tests.rs"]
mod tests;
