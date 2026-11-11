//! 存量伪轮次的纯内存折算：累加器、锚点载荷与位置身份重排。
//!
//! 本模块不碰数据库，只把"一轮轮读进来的 Turn"折成可直接落库的
//! `PreparedTurn` 序列；IO、事务与一次性标记由 `super` 负责。

use crate::backend::domain::conversations::triad::{TriadPart, TurnProcess, TurnProcessItem};
use crate::backend::domain::{derive_turn_id, ConversationSession, ConversationTurn};
use crate::backend::store::conversations::snapshot::PreparedTurn;
use crate::backend::store::conversations::subagent::{
    derive_child_session_id, SUBAGENT_ANCHOR_KIND, SUBAGENT_EXECUTION_ORIGIN,
};

use super::{LoadedTurn, TurnClass, CONTINUATION_PROMPT};

const CHILD_TITLE_LIMIT: usize = 80;
const REPORT_SUMMARY_LIMIT: usize = 240;

/// 父轮次里的有序内容节点：过程节点与留存零件共用同一套序号空间。
enum Entry {
    Item(TurnProcessItem),
    Part(TriadPart),
}

#[derive(Debug, Clone, Default)]
struct ResponsePayload {
    text: Option<String>,
    translated: Option<String>,
    card: Option<bool>,
    card_kind: Option<String>,
    signal: Option<String>,
}

impl ResponsePayload {
    /// 三段式的回答元数据散落在 turn.response 与 process 的四个稀疏列上，
    /// 宿主轮次与续接轮共用同一份收拢逻辑，避免两处慢慢漂移。
    fn from_turn(turn: &LoadedTurn) -> Self {
        Self {
            text: non_empty(turn.row.response.clone()),
            translated: turn.process.response_translated_text.clone(),
            card: turn.process.response_card,
            card_kind: turn.process.response_card_kind.clone(),
            signal: turn.process.response_signal.clone(),
        }
    }
}

struct Accumulator {
    /// 折算后的父轮次序号，先定下来供锚点写 `spawn_anchor`。
    index: i64,
    prompt: String,
    started_at: String,
    ended_at: Option<String>,
    model: Option<String>,
    imported_at: String,
    process: TurnProcess,
    entries: Vec<Entry>,
    response: ResponsePayload,
}

impl Accumulator {
    fn from_turn(turn: LoadedTurn, index: i64) -> Self {
        let mut entries = Vec::new();
        collect_entries(&turn.process, &turn.parts, &mut entries);
        let response = ResponsePayload::from_turn(&turn);
        let process = turn.process;
        Self {
            index,
            prompt: turn.row.prompt,
            started_at: turn.row.started_at,
            ended_at: turn.row.ended_at,
            model: turn.row.model,
            imported_at: turn.row.imported_at,
            process,
            entries,
            response,
        }
    }

    /// 伪轮次出现在首轮时构造的宿主轮次：保留时间线，Prompt 留空。
    fn orphan_host(turn: &LoadedTurn, index: i64) -> Self {
        Self {
            index,
            prompt: String::new(),
            started_at: turn.row.started_at.clone(),
            ended_at: turn.row.ended_at.clone(),
            model: turn.row.model.clone(),
            imported_at: turn.row.imported_at.clone(),
            process: TurnProcess::default(),
            entries: Vec::new(),
            response: ResponsePayload::default(),
        }
    }

    /// 锚点以「正文 Part + 过程链 Item」成对写入，二者各占一个序号。
    ///
    /// `flush` 用同一个自增下标给两类条目编号，所以折算产物里
    /// `part_index` 与 `process.seq` **天然差 1**（part@21 / item@22）。
    /// 消费方只能按 `child_session_id` 对齐，不得假设两者相等。
    fn append_anchor(&mut self, part: TriadPart, item: TurnProcessItem) {
        self.entries.push(Entry::Part(part));
        self.entries.push(Entry::Item(item));
    }

    fn append_continuation(&mut self, turn: LoadedTurn, adapter_id: &str) {
        let mut continuation_entries = Vec::new();
        collect_entries(&turn.process, &turn.parts, &mut continuation_entries);
        for entry in &mut continuation_entries {
            // 续接轮自己的公共 cwd 会随轮次一起消失，先烘进各节点再并表。
            if let Entry::Item(item) = entry {
                if item.cwd.is_none() {
                    item.cwd = turn.process.cwd.clone();
                }
            }
        }
        self.entries.extend(continuation_entries);
        if let Some(ended_at) = turn.row.ended_at.clone() {
            self.ended_at = Some(ended_at);
        }
        let next = ResponsePayload::from_turn(&turn);
        if next.text.is_some() {
            self.demote_response(adapter_id);
            self.response = next;
        }
    }

    /// 过渡回答降级成普通助手零件，避免被后续续接回答覆盖时丢内容。
    fn demote_response(&mut self, adapter_id: &str) {
        let Some(text) = self.response.text.take() else {
            return;
        };
        if text.trim().is_empty() {
            return;
        }
        let attrs = serde_json::json!({ "renderer": "markdown" }).to_string();
        // 卡片类型必须以合法首字符开头，适配器标识缺失时退到通用命名空间。
        let namespace = if adapter_id.trim().is_empty() {
            "conversation"
        } else {
            adapter_id
        };
        self.entries.push(Entry::Part(TriadPart {
            part_index: -1,
            kind: format!("{namespace}.reasoning"),
            content: Some(text),
            attrs_json: Some(attrs),
            translated_text: self.response.translated.take(),
        }));
        self.response.card = None;
        self.response.card_kind = None;
        self.response.signal = None;
    }

    fn flush(self, ctx: &FoldContext<'_>) -> PreparedTurn {
        let mut items = Vec::new();
        let mut parts = Vec::new();
        let mut next_index = 0i64;
        for entry in self.entries {
            match entry {
                Entry::Item(mut item) => {
                    item.seq = next_index;
                    items.push(item);
                }
                Entry::Part(mut part) => {
                    part.part_index = next_index;
                    parts.push(part);
                }
            }
            next_index += 1;
        }

        let mut process = self.process;
        process.turn_kind = None;
        process.agent_role = None;
        process.parent_turn_id = None;
        process.items = items;
        let has_response = self.response.text.is_some();
        process.response_seq = has_response.then_some(next_index);
        process.response_translated_text = self.response.translated;
        process.response_card = self.response.card;
        process.response_card_kind = self.response.card_kind;
        process.response_signal = self.response.signal;

        let title = process.title.clone();
        let process_json = if process.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&process).unwrap_or_default())
        };

        PreparedTurn {
            turn: ConversationTurn {
                id: derive_turn_id("conversation-turn", &ctx.parent.id, self.index),
                session_id: ctx.parent.id.clone(),
                turn_index: self.index,
                prompt: self.prompt,
                title,
                started_at: self.started_at,
                ended_at: self.ended_at,
                imported_at: self.imported_at,
                model: self.model,
                process: process_json,
                response: self.response.text,
                turn_kind: None,
                agent_role: None,
                parent_turn_id: None,
            },
            parts,
        }
    }
}

fn collect_entries(process: &TurnProcess, parts: &[TriadPart], out: &mut Vec<Entry>) {
    let mut ordered: Vec<(i64, bool, Entry)> = Vec::new();
    for item in &process.items {
        ordered.push((item.seq, true, Entry::Item(item.clone())));
    }
    for part in parts {
        if part
            .content
            .as_deref()
            .is_some_and(|body| body.trim() == CONTINUATION_PROMPT)
        {
            continue;
        }
        ordered.push((part.part_index, false, Entry::Part(part.clone())));
    }
    // 同一序号上过程节点优先，保持"命令先于其结果"的原始阅读顺序。
    ordered.sort_by_key(|(index, is_item, _)| (*index, !*is_item));
    out.extend(ordered.into_iter().map(|(_, _, entry)| entry));
}

pub(super) struct FoldedChild {
    pub(super) session: ConversationSession,
    pub(super) turn: PreparedTurn,
}

pub(super) struct FoldedSession {
    pub(super) parent_turns: Vec<PreparedTurn>,
    pub(super) children: Vec<FoldedChild>,
    pub(super) pseudo_turn_count: usize,
}

pub(super) struct FoldContext<'a> {
    pub(super) parent: &'a ConversationSession,
    pub(super) now: &'a str,
}

impl FoldContext<'_> {
    pub(super) fn fold_turns(&self, turns: Vec<LoadedTurn>, ordinal_start: i64) -> FoldedSession {
        let mut parent_turns = Vec::new();
        let mut children = Vec::new();
        let mut accumulator: Option<Accumulator> = None;
        let mut pseudo_turn_count = 0usize;
        let mut ordinal = ordinal_start - 1;

        for turn in turns {
            match turn.class {
                TurnClass::Human => {
                    if let Some(done) = accumulator.take() {
                        parent_turns.push(done.flush(self));
                    }
                    accumulator = Some(Accumulator::from_turn(turn, parent_turns.len() as i64));
                }
                TurnClass::Subagent => {
                    let index = parent_turns.len() as i64;
                    let host =
                        accumulator.get_or_insert_with(|| Accumulator::orphan_host(&turn, index));
                    ordinal += 1;
                    let child = self.build_child(&turn, ordinal, host.index);
                    let anchor = build_anchor(&turn, &child.session, host.index);
                    host.append_anchor(anchor.0, anchor.1);
                    children.push(child);
                    pseudo_turn_count += 1;
                }
                TurnClass::Continuation => {
                    let index = parent_turns.len() as i64;
                    let host =
                        accumulator.get_or_insert_with(|| Accumulator::orphan_host(&turn, index));
                    host.append_continuation(turn, &self.parent.adapter_id);
                    pseudo_turn_count += 1;
                }
            }
        }
        if let Some(done) = accumulator.take() {
            parent_turns.push(done.flush(self));
        }

        FoldedSession {
            parent_turns,
            children,
            pseudo_turn_count,
        }
    }

    fn build_child(&self, turn: &LoadedTurn, ordinal: i64, parent_turn_index: i64) -> FoldedChild {
        let parent = self.parent;
        let child_external_id = format!("{}:subagent:legacy-{}", parent.external_id, ordinal);
        let child_session_id = derive_child_session_id(&parent.source_id, &child_external_id);
        let title = turn
            .process
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| first_line_preview(&turn.row.prompt, CHILD_TITLE_LIMIT));
        let updated_at = turn
            .row
            .ended_at
            .clone()
            .unwrap_or_else(|| turn.row.started_at.clone());

        let origin = LegacySubagentOriginMeta {
            parent_session_id: Some(parent.id.clone()),
            parent_external_id: Some(parent.external_id.clone()),
            parent_turn_index: Some(parent_turn_index),
            agent_role: non_empty(turn.process.agent_role.clone()),
            spawn_anchor: Some(spawn_anchor(parent_turn_index)),
        };
        let execution_purpose = serde_json::to_string(&origin).ok();

        let session = ConversationSession {
            id: child_session_id.clone(),
            source_id: parent.source_id.clone(),
            adapter_id: parent.adapter_id.clone(),
            external_id: child_external_id,
            title: if title.is_empty() {
                "Subagent".to_string()
            } else {
                title
            },
            project_path: parent.project_path.clone(),
            started_at: Some(turn.row.started_at.clone()),
            updated_at: Some(updated_at),
            // 存量内联子代理没有独立磁盘文件，不伪造一个指不到任何位置的定位器。
            source_locator: None,
            source_fingerprint: None,
            missing: parent.missing,
            created_at: self.now.to_string(),
            imported_at: self.now.to_string(),
            execution_origin: SUBAGENT_EXECUTION_ORIGIN.to_string(),
            execution_purpose,
            user_visible: false,
            last_cwd: parent.last_cwd.clone(),
        };

        let mut process = turn.process.clone();
        process.turn_kind = None;
        process.parent_turn_id = None;
        let turn_id = derive_turn_id("conversation-turn", &child_session_id, 0);
        let child_title = process.title.clone();
        let agent_role = process.agent_role.clone();
        let process_json = Some(serde_json::to_string(&process).unwrap_or_default());

        FoldedChild {
            session,
            turn: PreparedTurn {
                turn: ConversationTurn {
                    id: turn_id,
                    session_id: child_session_id,
                    turn_index: 0,
                    prompt: turn.row.prompt.clone(),
                    title: child_title,
                    started_at: turn.row.started_at.clone(),
                    ended_at: turn.row.ended_at.clone(),
                    imported_at: turn.row.imported_at.clone(),
                    model: turn.row.model.clone(),
                    process: process_json,
                    response: turn.row.response.clone(),
                    turn_kind: None,
                    agent_role,
                    parent_turn_id: None,
                },
                parts: turn.parts.clone(),
            },
        }
    }
}

/// 父轮次委派锚点 Part 的 `attrs_json` 载荷（ADR-0022 的内联类型契约）。
///
/// 读模型会把这些字段还原成 `ConversationPart.content_card`：`kind` / `semantic_role`
/// 决定卡片语义，`renderer = "subagent_tree"` 是当前卡片契约里已注册的渲染器。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
struct LegacyDispatchAnchorMeta {
    kind: String,
    semantic_role: String,
    renderer: String,
    child_session_id: String,
    child_external_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    agent_role: Option<String>,
    title: String,
    status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    step_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    report_summary: Option<String>,
    spawn_anchor: String,
}

/// 子代理会话 `execution_purpose` 里的父子溯源载荷。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
struct LegacySubagentOriginMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent_external_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent_turn_index: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    agent_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    spawn_anchor: Option<String>,
}

/// 父轮次委派锚点的 Part + 过程节点。
fn build_anchor(
    turn: &LoadedTurn,
    child: &ConversationSession,
    parent_turn_index: i64,
) -> (TriadPart, TurnProcessItem) {
    let title = if child.title.trim().is_empty() {
        first_line_preview(&turn.row.prompt, CHILD_TITLE_LIMIT)
    } else {
        child.title.clone()
    };
    let summary = summarize_response(turn.row.response.as_deref());
    let content = format!(
        "[委派任务] {title}\n[汇报结论] {}",
        summary.as_deref().unwrap_or("（无结论摘要）")
    );
    let step_count = turn.process.items.len();
    let anchor = spawn_anchor(parent_turn_index);
    let agent_role = non_empty(turn.process.agent_role.clone());
    let model = non_empty(turn.row.model.clone());

    let attrs = serde_json::to_string(&LegacyDispatchAnchorMeta {
        kind: SUBAGENT_ANCHOR_KIND.to_string(),
        semantic_role: SUBAGENT_ANCHOR_KIND.to_string(),
        renderer: "subagent_tree".to_string(),
        child_session_id: child.id.clone(),
        child_external_id: child.external_id.clone(),
        agent_role: agent_role.clone(),
        title: title.clone(),
        status: "completed".to_string(),
        model: model.clone(),
        step_count,
        report_summary: summary.clone(),
        spawn_anchor: anchor,
    })
    .unwrap_or_default();

    let label = format!(
        "Subagent · {}: {}",
        agent_role.as_deref().unwrap_or("subagent"),
        title
    );

    (
        TriadPart {
            part_index: -1,
            kind: SUBAGENT_ANCHOR_KIND.to_string(),
            content: Some(content),
            attrs_json: Some(attrs),
            translated_text: None,
        },
        TurnProcessItem {
            seq: -1,
            kind: SUBAGENT_ANCHOR_KIND.to_string(),
            label: Some(label),
            content: Some(child.id.clone()),
            signal: None,
            cwd: None,
            exit_code: None,
            summary,
            status: Some("completed".to_string()),
            translated_text: None,
        },
    )
}

/// `#T<n>` 形式的父轮次定位锚点（轮次号从 1 开始，与 CLI / UI 展示一致）。
fn spawn_anchor(parent_turn_index: i64) -> String {
    format!("#T{}", parent_turn_index + 1)
}

fn summarize_response(response: Option<&str>) -> Option<String> {
    let raw = response?.trim();
    if raw.is_empty() {
        return None;
    }
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(truncate_chars(&collapsed, REPORT_SUMMARY_LIMIT))
}

fn first_line_preview(text: &str, limit: usize) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or_default();
    truncate_chars(line, limit)
}

fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit).collect()
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.trim().is_empty())
}
