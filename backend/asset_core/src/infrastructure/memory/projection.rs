use super::storage::MemoryStorage;
use crate::backend::domain::memory::{
    MemoryTreeNode, NodeStatus, ProjectMemoryState, RecentMemoryState,
};
use std::path::PathBuf;

fn format_turn_timestamp(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return dt.format("%Y-%m-%d %H:%M").to_string();
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
        return dt.format("%Y-%m-%d %H:%M").to_string();
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M") {
        return dt.format("%Y-%m-%d %H:%M").to_string();
    }
    if trimmed.len() >= 16 && trimmed.chars().nth(10) == Some('T') {
        let date_part = &trimmed[..10];
        let time_part = &trimmed[11..16];
        return format!("{} {}", date_part, time_part);
    }
    trimmed.to_string()
}

fn format_node_line(node: &MemoryTreeNode) -> String {
    let ts_prefix = match &node.turn_started_at {
        Some(raw) if !raw.trim().is_empty() => format!("[{}] ", format_turn_timestamp(raw)),
        _ => String::new(),
    };
    let parent_suffix = match &node.parent_id {
        Some(pid) if !pid.trim().is_empty() => format!(" *(演进自 [{}])*", pid.trim()),
        _ => String::new(),
    };
    format!(
        "- [{}](aiwc://session/{}/turn/{}): {}{}{}\n",
        node.id,
        node.short_id(),
        node.turn_index,
        ts_prefix,
        node.summary,
        parent_suffix
    )
}

/// Pure function that projects a `ProjectMemoryState` into markdown format.
pub fn project_state_to_markdown(state: &ProjectMemoryState) -> String {
    let mut md = String::new();
    md.push_str("---\n");
    md.push_str(&format!("project_path: {}\n", state.project_slug));
    md.push_str(&format!("updated_at: {}\n", state.updated_at));
    md.push_str("---\n\n");
    md.push_str(&format!("# 项目记忆: {}\n\n", state.project_slug));

    // Sort topic keys deterministically
    let mut topic_keys: Vec<&String> = state.topic_trees.keys().collect();
    topic_keys.sort();

    md.push_str("## 🍂 待探索的悬空叶子节点 (Unexplored Branches)\n");
    let mut has_unexplored = false;
    for key in &topic_keys {
        if let Some(tree) = state.topic_trees.get(*key) {
            let mut unex_nodes = Vec::new();
            let mut node_keys: Vec<&String> = tree.nodes.keys().collect();
            node_keys.sort();
            for n_key in node_keys {
                if let Some(node) = tree.nodes.get(n_key) {
                    if node.status == NodeStatus::Unexplored {
                        unex_nodes.push(node);
                    }
                }
            }
            if !unex_nodes.is_empty() {
                has_unexplored = true;
                let topic_display = if tree.topic_name.is_empty() {
                    &tree.topic_id
                } else {
                    &tree.topic_name
                };
                md.push_str(&format!("\n### 🏷️ Topic: {}\n", topic_display));
                for node in unex_nodes {
                    md.push_str(&format_node_line(node));
                }
            }
        }
    }
    if !has_unexplored {
        md.push_str("- 暂无待探索分支\n");
    }
    md.push('\n');

    md.push_str("## ✅ 已完成的闭环任务 (Completed Tasks)\n");
    let mut has_completed = false;
    for key in &topic_keys {
        if let Some(tree) = state.topic_trees.get(*key) {
            let mut comp_nodes = Vec::new();
            let mut node_keys: Vec<&String> = tree.nodes.keys().collect();
            node_keys.sort();
            for n_key in node_keys {
                if let Some(node) = tree.nodes.get(n_key) {
                    if node.status == NodeStatus::Completed {
                        comp_nodes.push(node);
                    }
                }
            }
            if !comp_nodes.is_empty() {
                has_completed = true;
                let topic_display = if tree.topic_name.is_empty() {
                    &tree.topic_id
                } else {
                    &tree.topic_name
                };
                md.push_str(&format!("\n### 🏷️ Topic: {}\n", topic_display));
                for node in comp_nodes {
                    md.push_str(&format_node_line(node));
                }
            }
        }
    }
    if !has_completed {
        md.push_str("- 暂无已完成任务\n");
    }

    md
}

/// Pure function that projects a `RecentMemoryState` into markdown format.
pub fn recent_state_to_markdown(state: &RecentMemoryState) -> String {
    let mut md = String::new();
    md.push_str("# 近期研发进展与待探索分支 (Recent Work & Unexplored Branches)\n\n");

    if state.projects.is_empty() {
        md.push_str("- 暂无近期项目记录\n");
        return md;
    }

    let mut project_keys: Vec<&String> = state.projects.keys().collect();
    project_keys.sort();

    for (idx, proj_key) in project_keys.iter().enumerate() {
        if let Some(proj) = state.projects.get(*proj_key) {
            md.push_str(&format!("## 项目: {}\n\n", proj.project_slug));

            let mut topic_keys: Vec<&String> = proj.topic_trees.keys().collect();
            topic_keys.sort();

            md.push_str("### 🍂 待探索的悬空叶子节点 (Unexplored Branches)\n");
            let mut has_unexplored = false;
            for t_key in &topic_keys {
                if let Some(tree) = proj.topic_trees.get(*t_key) {
                    let mut unex_nodes = Vec::new();
                    let mut node_keys: Vec<&String> = tree.nodes.keys().collect();
                    node_keys.sort();
                    for n_key in node_keys {
                        if let Some(node) = tree.nodes.get(n_key) {
                            if node.status == NodeStatus::Unexplored {
                                unex_nodes.push(node);
                            }
                        }
                    }
                    if !unex_nodes.is_empty() {
                        has_unexplored = true;
                        let topic_display = if tree.topic_name.is_empty() {
                            &tree.topic_id
                        } else {
                            &tree.topic_name
                        };
                        md.push_str(&format!("\n### 🏷️ Topic: {}\n", topic_display));
                        for node in unex_nodes {
                            md.push_str(&format_node_line(node));
                        }
                    }
                }
            }
            if !has_unexplored {
                md.push_str("- 无待探索分支\n");
            }
            md.push('\n');

            md.push_str("### ✅ 已完成的闭环任务 (Completed Tasks)\n");
            let mut has_completed = false;
            for t_key in &topic_keys {
                if let Some(tree) = proj.topic_trees.get(*t_key) {
                    let mut comp_nodes = Vec::new();
                    let mut node_keys: Vec<&String> = tree.nodes.keys().collect();
                    node_keys.sort();
                    for n_key in node_keys {
                        if let Some(node) = tree.nodes.get(n_key) {
                            if node.status == NodeStatus::Completed {
                                comp_nodes.push(node);
                            }
                        }
                    }
                    if !comp_nodes.is_empty() {
                        has_completed = true;
                        let topic_display = if tree.topic_name.is_empty() {
                            &tree.topic_id
                        } else {
                            &tree.topic_name
                        };
                        md.push_str(&format!("\n### 🏷️ Topic: {}\n", topic_display));
                        for node in comp_nodes {
                            md.push_str(&format_node_line(node));
                        }
                    }
                }
            }
            if !has_completed {
                md.push_str("- 无已完成任务\n");
            }

            if idx + 1 < project_keys.len() {
                md.push('\n');
            }
        }
    }

    md
}

/// One-way sync helper: projects `ProjectMemoryState` to `projects/<slug>.md` atomically.
pub fn sync_project_markdown(
    storage: &MemoryStorage,
    state: &ProjectMemoryState,
) -> std::io::Result<PathBuf> {
    let md = project_state_to_markdown(state);
    let rel_path = format!("projects/{}.md", state.project_slug);
    storage.write_markdown_atomic(rel_path, &md)
}

/// One-way sync helper: projects `RecentMemoryState` to `recent/latest.md` atomically.
pub fn sync_recent_markdown(
    storage: &MemoryStorage,
    state: &RecentMemoryState,
) -> std::io::Result<PathBuf> {
    let md = recent_state_to_markdown(state);
    storage.write_markdown_atomic("recent/latest.md", &md)
}

#[cfg(test)]
#[path = "projection_tests.rs"]
mod tests;
