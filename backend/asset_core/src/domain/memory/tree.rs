use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Status of a node within the multi-branch memory topic tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Completed,
    Continued,
    Unexplored,
    Dismissed,
}

impl NodeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Continued => "continued",
            Self::Unexplored => "unexplored",
            Self::Dismissed => "dismissed",
        }
    }
}

impl std::fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for NodeStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "completed" => Ok(Self::Completed),
            "continued" => Ok(Self::Continued),
            "unexplored" => Ok(Self::Unexplored),
            "dismissed" => Ok(Self::Dismissed),
            other => Err(format!("unknown node status: {}", other)),
        }
    }
}

/// A single node in a technical topic tree, representing an explored or unexplored turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryTreeNode {
    /// Normalized node identifier `<short_id>#T<turn>` or `<short_id>#T<turn>.<sub_index>`.
    pub id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub topic_id: String,
    pub summary: String,
    pub status: NodeStatus,
    pub source_session_id: String,
    pub turn_index: u32,
    #[serde(default)]
    pub turn_started_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl MemoryTreeNode {
    /// Format a normalized node ID from session short_id, turn number and optional sub-index.
    pub fn format_id(short_id: &str, turn: u32, sub_index: Option<u32>) -> String {
        match sub_index {
            Some(sub) => format!("{short_id}#T{turn}.{sub}"),
            None => format!("{short_id}#T{turn}"),
        }
    }

    /// Parse a node ID into `(short_id, turn_index, Option<sub_index>)`.
    pub fn parse_id(id: &str) -> Option<(String, u32, Option<u32>)> {
        let (short_id, rest) = id.split_once("#T")?;
        if short_id.is_empty() || rest.is_empty() {
            return None;
        }
        if let Some((turn_str, sub_str)) = rest.split_once('.') {
            let turn: u32 = turn_str.parse().ok()?;
            let sub: u32 = sub_str.parse().ok()?;
            Some((short_id.to_string(), turn, Some(sub)))
        } else {
            let turn: u32 = rest.parse().ok()?;
            Some((short_id.to_string(), turn, None))
        }
    }

    /// Extract the short session identifier.
    pub fn short_id(&self) -> String {
        if let Some((short, _, _)) = Self::parse_id(&self.id) {
            short
        } else {
            self.source_session_id.chars().take(8).collect::<String>()
        }
    }
}

/// A named technical topic tree organizing related memory nodes into an inheritance hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TopicTree {
    pub topic_id: String,
    pub topic_name: String,
    #[serde(default)]
    pub root_node_ids: Vec<String>,
    #[serde(default)]
    pub nodes: HashMap<String, MemoryTreeNode>,
}

impl TopicTree {
    pub fn new(topic_id: impl Into<String>, topic_name: impl Into<String>) -> Self {
        Self {
            topic_id: topic_id.into(),
            topic_name: topic_name.into(),
            root_node_ids: Vec::new(),
            nodes: HashMap::new(),
        }
    }

    pub fn insert_node(&mut self, node: MemoryTreeNode, is_root: bool) {
        let id = node.id.clone();
        if is_root && !self.root_node_ids.contains(&id) {
            self.root_node_ids.push(id.clone());
        }
        self.nodes.insert(id, node);
    }
}

/// The complete state of a project's memory, containing multi-branch topic trees and session watermarks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectMemoryState {
    pub project_slug: String,
    #[serde(default)]
    pub topic_trees: HashMap<String, TopicTree>,
    #[serde(default)]
    pub session_watermarks: HashMap<String, u32>,
    pub updated_at: String,
}

impl ProjectMemoryState {
    pub fn new(project_slug: impl Into<String>) -> Self {
        Self {
            project_slug: project_slug.into(),
            topic_trees: HashMap::new(),
            session_watermarks: HashMap::new(),
            updated_at: Utc::now().to_rfc3339(),
        }
    }

    /// 会话已处理到的轮次水位。
    ///
    /// 水位可能以完整会话 ID 或 8 位短 ID 写入（历史上两种都出现过），
    /// 因此先按原样精确命中，再按统一的短 ID 片段兜底匹配，多个命中取最大值。
    pub fn get_watermark(&self, session_id: &str) -> Option<u32> {
        if let Some(&watermark) = self.session_watermarks.get(session_id) {
            return Some(watermark);
        }
        let target = crate::backend::domain::conversation_id_fragment(session_id);
        if target.is_empty() {
            return None;
        }
        self.session_watermarks
            .iter()
            .filter(|(stored_id, _)| {
                crate::backend::domain::conversation_id_fragment(stored_id) == target
            })
            .map(|(_, watermark)| *watermark)
            .max()
    }

    pub fn set_watermark(&mut self, session_id: impl Into<String>, turn: u32) {
        self.session_watermarks
            .entry(session_id.into())
            .and_modify(|w| *w = (*w).max(turn))
            .or_insert(turn);
        self.updated_at = Utc::now().to_rfc3339();
    }

    /// 把同一项目的历史累计状态无损合并进当前状态。
    ///
    /// 便携路径锚点折叠引入前，同一项目可能落成多个 slug 状态文件；合并时遵循
    /// 累计真源不可毁灭原则：
    /// 1. 主题按 `topic_id`、分支按 `node.id` 增量补齐，缺的一律补齐；
    /// 2. 同一节点两侧都存在时取「状态更靠后」或「`updated_at` 更新」的一侧，
    ///    但绝不把 `completed` / `dismissed` / `continued` 退回 `unexplored`，
    ///    也绝不因为一侧缺失而丢掉另一侧的悬空分支；
    /// 3. 会话水位取两者最大值，`updated_at` 取较新者。
    pub fn merge_legacy(&mut self, legacy: ProjectMemoryState) {
        for (topic_id, legacy_tree) in legacy.topic_trees {
            let tree = self
                .topic_trees
                .entry(topic_id)
                .or_insert_with(|| TopicTree {
                    topic_id: legacy_tree.topic_id.clone(),
                    topic_name: legacy_tree.topic_name.clone(),
                    root_node_ids: Vec::new(),
                    nodes: HashMap::new(),
                });
            if tree.topic_name.trim().is_empty() && !legacy_tree.topic_name.trim().is_empty() {
                tree.topic_name = legacy_tree.topic_name.clone();
            }
            for root_id in legacy_tree.root_node_ids {
                if !tree.root_node_ids.contains(&root_id) {
                    tree.root_node_ids.push(root_id);
                }
            }
            for (node_id, legacy_node) in legacy_tree.nodes {
                match tree.nodes.entry(node_id) {
                    std::collections::hash_map::Entry::Vacant(slot) => {
                        slot.insert(legacy_node);
                    }
                    std::collections::hash_map::Entry::Occupied(mut slot) => {
                        if is_newer_node(slot.get(), &legacy_node) {
                            slot.insert(legacy_node);
                        }
                    }
                }
            }
        }

        for (session_id, watermark) in legacy.session_watermarks {
            self.session_watermarks
                .entry(session_id)
                .and_modify(|current| *current = (*current).max(watermark))
                .or_insert(watermark);
        }

        if legacy.updated_at > self.updated_at {
            self.updated_at = legacy.updated_at;
        }
    }

    /// Apply an evolution commit payload to this project memory state.
    pub fn apply_evolution(
        &mut self,
        payload: TopicEvolutionCommitPayload,
    ) -> Result<MemoryCommitResult, String> {
        let now = Utc::now().to_rfc3339();

        let topic_name = payload
            .topic_name
            .clone()
            .unwrap_or_else(|| payload.topic_id.clone());
        let tree = self
            .topic_trees
            .entry(payload.topic_id.clone())
            .or_insert_with(|| TopicTree::new(&payload.topic_id, topic_name));

        if let Some(ref name) = payload.topic_name {
            if !name.is_empty() {
                tree.topic_name = name.clone();
            }
        }

        // 1. If parent node ID is specified, verify existence and update parent status and timestamp
        if let Some(ref parent_id) = payload.parent_node_id {
            let parent = tree
                .nodes
                .get_mut(parent_id)
                .ok_or_else(|| format!("parent node not found in topic tree: {parent_id}"))?;
            let new_status = payload.parent_new_status.unwrap_or(NodeStatus::Continued);
            parent.status = new_status;
            parent.updated_at = now.clone();
        }

        // 2. Mount new node
        let mut new_node = payload.new_node;
        if new_node.parent_id.is_none() && payload.parent_node_id.is_some() {
            new_node.parent_id = payload.parent_node_id.clone();
        }
        let is_root = new_node.parent_id.is_none();
        let committed_node_id = new_node.id.clone();
        tree.insert_node(new_node.clone(), is_root);

        // 3. Mount unexplored siblings, protecting nodes already continued, completed, or dismissed
        let mut unexplored_node_ids = Vec::new();
        for mut sibling in payload.unexplored_siblings {
            if let Some(existing) = tree.nodes.get(&sibling.id) {
                if existing.status == NodeStatus::Continued
                    || existing.status == NodeStatus::Completed
                    || existing.status == NodeStatus::Dismissed
                {
                    continue;
                }
            }
            sibling.status = NodeStatus::Unexplored;
            if sibling.parent_id.is_none() {
                sibling.parent_id = payload.parent_node_id.clone();
            }
            let sib_is_root = sibling.parent_id.is_none();
            unexplored_node_ids.push(sibling.id.clone());
            tree.insert_node(sibling, sib_is_root);
        }

        // 4. Update session watermark monotonically
        self.session_watermarks
            .entry(new_node.source_session_id.clone())
            .and_modify(|w| *w = (*w).max(new_node.turn_index))
            .or_insert(new_node.turn_index);
        self.updated_at = now.clone();

        Ok(MemoryCommitResult {
            project_slug: payload.project_slug,
            topic_id: payload.topic_id,
            committed_node_id,
            parent_node_id: payload.parent_node_id,
            unexplored_node_ids,
            updated_at: self.updated_at.clone(),
        })
    }
}

/// 合并历史状态时判断「哪一侧节点更可信」。
///
/// 优先保留已经推进过的状态（非 `unexplored`），其次比较 `updated_at`；
/// 这样可以避免历史文件把已闭环/已废弃的节点重新变回悬空分支。
fn is_newer_node(existing: &MemoryTreeNode, legacy: &MemoryTreeNode) -> bool {
    let existing_progressed = existing.status != NodeStatus::Unexplored;
    let legacy_progressed = legacy.status != NodeStatus::Unexplored;
    match (existing_progressed, legacy_progressed) {
        (false, true) => true,
        (true, false) => false,
        _ => legacy.updated_at > existing.updated_at,
    }
}

/// The aggregated state of recent memory across active projects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecentMemoryState {
    #[serde(default)]
    pub projects: HashMap<String, ProjectMemoryState>,
    pub updated_at: String,
}

impl Default for RecentMemoryState {
    fn default() -> Self {
        Self::new()
    }
}

impl RecentMemoryState {
    pub fn new() -> Self {
        Self {
            projects: HashMap::new(),
            updated_at: Utc::now().to_rfc3339(),
        }
    }

    pub fn upsert_project(&mut self, project: ProjectMemoryState) {
        self.projects.insert(project.project_slug.clone(), project);
        self.updated_at = Utc::now().to_rfc3339();
    }
}

/// Input payload to commit an evolution turn onto a project's memory topic tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TopicEvolutionCommitPayload {
    pub project_slug: String,
    pub topic_id: String,
    #[serde(default)]
    pub topic_name: Option<String>,
    #[serde(default)]
    pub parent_node_id: Option<String>,
    #[serde(default)]
    pub parent_new_status: Option<NodeStatus>,
    pub new_node: MemoryTreeNode,
    #[serde(default)]
    pub unexplored_siblings: Vec<MemoryTreeNode>,
}

/// Result returned after successfully committing an evolution turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryCommitResult {
    pub project_slug: String,
    pub topic_id: String,
    pub committed_node_id: String,
    #[serde(default)]
    pub parent_node_id: Option<String>,
    #[serde(default)]
    pub unexplored_node_ids: Vec<String>,
    pub updated_at: String,
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
