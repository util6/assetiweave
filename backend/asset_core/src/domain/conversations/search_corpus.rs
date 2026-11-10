//! 会话检索语料的策略化生成（ADR-0023）。
//!
//! 搜索索引是只读派生产物，语料口径必须只有一处定义。本模块只消费已落库的 Turn 与结构化
//! Part 事实（提问正文、最终回答、变更文件路径、命令语义标签），不消费任意展示卡片正文，
//! 因此终端输出、diff 正文、推理日志与委派明细天然无法进入检索语料。
//!
//! 策略一旦发布即不可就地改语义：新增或调整口径只能提升 [`SEARCH_CORPUS_POLICY_VERSION`]，
//! 由派生索引按"记录的策略身份不一致"触发受控重建，避免同一代索引被悄悄改写。

use std::collections::BTreeSet;

use super::{ConversationPart, ConversationPartKind};

/// 策略族名。同一族内的版本必须保持"高信号白名单"这一共同意图。
pub const SEARCH_CORPUS_POLICY_FAMILY: &str = "high-signal";

/// 当前生效的语料策略版本。
pub const SEARCH_CORPUS_POLICY_VERSION: u32 = 1;

/// 明确表达"目标文件被改写"的命令语义标签。
///
/// 适配器不会统一产出 `file_change` 零件，文件变更通常表现为携带 `file_path` 的工具零件
/// 加一个语义标签（如 `Edit` / `Write` / `apply_patch`）。只认标签，不解析 diff 正文。
const MUTATING_FILE_LABELS: [&str; 11] = [
    "edit",
    "write",
    "patch",
    "apply_patch",
    "applypatch",
    "create",
    "delete",
    "move",
    "rename",
    "file_change",
    "file-change",
];

/// 检索语料的四类高信号来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SearchCorpusSignal {
    /// 用户提问正文。
    Prompt,
    /// AI 最终回答正文。
    Answer,
    /// 变更目标文件路径。
    ChangedFile,
    /// 命令语义标签。
    CommandLabel,
}

impl SearchCorpusSignal {
    pub const ALL: [Self; 4] = [
        Self::Prompt,
        Self::Answer,
        Self::ChangedFile,
        Self::CommandLabel,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::Answer => "answer",
            Self::ChangedFile => "changed_file",
            Self::CommandLabel => "command_label",
        }
    }
}

/// 后端中立的检索文档：FTS5 与独立搜索索引消费同一份条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchCorpusDocument {
    pub signal: SearchCorpusSignal,
    pub turn_id: String,
    pub text: String,
}

impl SearchCorpusDocument {
    /// 稳定行键。同一个 Turn 的同一种信号恒为同一行，替换即幂等。
    pub fn anchor_id(&self) -> String {
        format!("{}:{}", self.turn_id, self.signal.as_str())
    }
}

/// 单个 Turn 的已提交事实视图。
#[derive(Debug, Clone, Copy)]
pub struct CorpusTurnFacts<'a> {
    pub turn_id: &'a str,
    pub prompt: &'a str,
    pub response: Option<&'a str>,
    pub parts: &'a [ConversationPart],
}

/// 可替换、可版本化的语料策略。
pub trait SearchCorpusPolicy: Send + Sync {
    /// 当前策略版本。
    fn version(&self) -> u32;

    /// 策略身份（`family@version`），写入索引代际并参与兼容性判定。
    fn identity(&self) -> String {
        format!("{SEARCH_CORPUS_POLICY_FAMILY}@{}", self.version())
    }

    /// 从一个 Turn 抽取高信号文档。
    fn extract(&self, turn: &CorpusTurnFacts<'_>) -> Vec<SearchCorpusDocument>;
}

/// 当前生效策略的版本号。
pub fn search_corpus_policy_version() -> u32 {
    SEARCH_CORPUS_POLICY_VERSION
}

/// 当前生效策略的身份标识。
pub fn search_corpus_policy_identity() -> String {
    format!("{SEARCH_CORPUS_POLICY_FAMILY}@{SEARCH_CORPUS_POLICY_VERSION}")
}

/// 当前生效的高信号策略。
pub fn current_search_corpus_policy() -> &'static dyn SearchCorpusPolicy {
    &HighSignalCorpusPolicy
}

/// 初始高信号策略：只放行提问、最终回答、变更文件路径与命令标签。
#[derive(Debug, Default, Clone, Copy)]
pub struct HighSignalCorpusPolicy;

impl SearchCorpusPolicy for HighSignalCorpusPolicy {
    fn version(&self) -> u32 {
        SEARCH_CORPUS_POLICY_VERSION
    }

    fn extract(&self, turn: &CorpusTurnFacts<'_>) -> Vec<SearchCorpusDocument> {
        let mut documents = Vec::new();
        push_document(
            &mut documents,
            SearchCorpusSignal::Prompt,
            turn.turn_id,
            turn.prompt,
        );
        push_document(
            &mut documents,
            SearchCorpusSignal::Answer,
            turn.turn_id,
            turn.response.unwrap_or_default(),
        );

        let changed_files = collect_changed_file_paths(turn.parts);
        push_document(
            &mut documents,
            SearchCorpusSignal::ChangedFile,
            turn.turn_id,
            &changed_files.join("\n"),
        );

        let labels = collect_command_labels(turn.parts);
        push_document(
            &mut documents,
            SearchCorpusSignal::CommandLabel,
            turn.turn_id,
            &labels.join("\n"),
        );

        documents
    }
}

/// 只把非空正文沉淀为文档；空白信号直接缺席，而不是写一行空索引。
fn push_document(
    documents: &mut Vec<SearchCorpusDocument>,
    signal: SearchCorpusSignal,
    turn_id: &str,
    text: &str,
) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    documents.push(SearchCorpusDocument {
        signal,
        turn_id: turn_id.to_string(),
        text: trimmed.to_string(),
    });
}

fn collect_changed_file_paths(parts: &[ConversationPart]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();
    for part in parts {
        let Some(path) = part.file_path.as_deref().map(str::trim) else {
            continue;
        };
        if path.is_empty() || !is_file_mutation_part(part) {
            continue;
        }
        if seen.insert(path.to_string()) {
            paths.push(path.to_string());
        }
    }
    paths
}

fn is_file_mutation_part(part: &ConversationPart) -> bool {
    if part.kind == ConversationPartKind::FileChange {
        return true;
    }
    let label = part
        .command_label
        .as_deref()
        .or_else(|| part.content_card.as_ref().map(|card| card.kind.as_str()))
        .unwrap_or_default();
    let normalized = label.trim().to_ascii_lowercase();
    let tail = normalized
        .rsplit_once('.')
        .map(|(_, tail)| tail)
        .unwrap_or(normalized.as_str());
    MUTATING_FILE_LABELS.contains(&tail)
}

fn collect_command_labels(parts: &[ConversationPart]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut labels = Vec::new();
    for part in parts {
        let Some(label) = part.command_label.as_deref().map(str::trim) else {
            continue;
        };
        if label.is_empty() {
            continue;
        }
        // 语义标签去重按大小写不敏感，避免同一意图因大小写差异放大噪音。
        if seen.insert(label.to_ascii_lowercase()) {
            labels.push(label.to_string());
        }
    }
    labels
}

#[cfg(test)]
#[path = "search_corpus_tests.rs"]
mod tests;
