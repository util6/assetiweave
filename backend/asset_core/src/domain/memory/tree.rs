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
}

impl NodeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Continued => "continued",
            Self::Unexplored => "unexplored",
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

    pub fn get_watermark(&self, session_id: &str) -> Option<u32> {
        self.session_watermarks.get(session_id).copied()
    }

    pub fn set_watermark(&mut self, session_id: impl Into<String>, turn: u32) {
        self.session_watermarks
            .entry(session_id.into())
            .and_modify(|w| *w = (*w).max(turn))
            .or_insert(turn);
        self.updated_at = Utc::now().to_rfc3339();
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

        // 3. Mount unexplored siblings, protecting nodes already continued or completed
        let mut unexplored_node_ids = Vec::new();
        for mut sibling in payload.unexplored_siblings {
            if let Some(existing) = tree.nodes.get(&sibling.id) {
                if existing.status == NodeStatus::Continued
                    || existing.status == NodeStatus::Completed
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
