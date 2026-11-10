//! 近期工作（Recent Work）切片刷新：把全部项目状态按当前时间窗口重新切片。

use crate::backend::application::prelude::*;
use crate::backend::domain::memory::{
    MemoryTreeNode, NodeStatus, ProjectMemoryState, RecentMemoryState, TopicTree,
};
use crate::backend::infrastructure::memory::projection::sync_recent_markdown;
use crate::backend::infrastructure::memory::storage::MemoryStorage;
use std::collections::HashMap;

use super::super::state_loader::load_all_project_states;

fn parse_window_duration(window: Option<&str>) -> chrono::Duration {
    let raw = window.unwrap_or("48h").trim();
    if raw.is_empty() {
        return chrono::Duration::hours(48);
    }
    if let Some(h_str) = raw.strip_suffix(['h', 'H']) {
        if let Ok(h) = h_str.trim().parse::<i64>() {
            return chrono::Duration::hours(h);
        }
    }
    if let Some(d_str) = raw.strip_suffix(['d', 'D']) {
        if let Ok(d) = d_str.trim().parse::<i64>() {
            return chrono::Duration::days(d);
        }
    }
    if let Ok(h) = raw.parse::<i64>() {
        return chrono::Duration::hours(h);
    }
    chrono::Duration::hours(48)
}

fn parse_node_timestamp(node: &MemoryTreeNode) -> Option<chrono::DateTime<chrono::Utc>> {
    let ts_str = node
        .turn_started_at
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&node.updated_at);

    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts_str) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%d %H:%M:%S") {
        return Some(chrono::DateTime::from_naive_utc_and_offset(
            naive,
            chrono::Utc,
        ));
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%d %H:%M") {
        return Some(chrono::DateTime::from_naive_utc_and_offset(
            naive,
            chrono::Utc,
        ));
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&node.created_at) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    None
}

impl AppService {
    /// Refresh recent memory state across projects with windowing slice in specified storage.
    pub async fn refresh_recent_memory_state_in(
        &self,
        window: Option<&str>,
        storage: &MemoryStorage,
    ) -> AppResult<RecentMemoryState> {
        let duration = parse_window_duration(window);
        let cutoff = chrono::Utc::now() - duration;
        let mut recent_state = RecentMemoryState::new();

        for proj in load_all_project_states(storage)? {
            let mut filtered_trees = HashMap::new();
            let mut unex_count = 0usize;
            let mut comp_count = 0usize;

            for (t_id, tree) in proj.topic_trees {
                let mut filtered_nodes = HashMap::new();
                for (n_id, node) in tree.nodes {
                    match node.status {
                        NodeStatus::Unexplored => {
                            // 🍂 Unexplored: RETAIN ALL across time windows regardless of cutoff
                            unex_count += 1;
                            filtered_nodes.insert(n_id, node);
                        }
                        NodeStatus::Completed => {
                            // ✅ Completed: STRICTLY filtered by timestamp: turn_started_at (or updated_at/created_at) >= cutoff
                            if let Some(ts) = parse_node_timestamp(&node) {
                                if ts >= cutoff {
                                    comp_count += 1;
                                    filtered_nodes.insert(n_id, node);
                                }
                            }
                        }
                        NodeStatus::Dismissed | NodeStatus::Continued => {
                            // 🗑️ Dismissed: EXCLUDE completely
                            // Continued: intermediate non-leaf, exclude from recent slice
                        }
                    }
                }
                if !filtered_nodes.is_empty() {
                    filtered_trees.insert(
                        t_id.clone(),
                        TopicTree {
                            topic_id: t_id,
                            topic_name: tree.topic_name,
                            root_node_ids: tree.root_node_ids,
                            nodes: filtered_nodes,
                        },
                    );
                }
            }

            // Project-level pruning: If a project has zero Unexplored and zero Completed nodes in this window, omit from RecentMemoryState
            if unex_count > 0 || comp_count > 0 {
                let filtered_proj = ProjectMemoryState {
                    project_slug: proj.project_slug.clone(),
                    topic_trees: filtered_trees,
                    session_watermarks: proj.session_watermarks,
                    updated_at: proj.updated_at,
                };
                recent_state
                    .projects
                    .insert(proj.project_slug, filtered_proj);
            }
        }

        storage
            .write_recent_state_atomic(&recent_state)
            .map_err(AppError::from)?;
        sync_recent_markdown(storage, &recent_state).map_err(AppError::from)?;

        Ok(recent_state)
    }

    /// Refresh recent memory state across projects with windowing slice.
    pub async fn refresh_recent_memory_state(
        &self,
        window: Option<&str>,
    ) -> AppResult<RecentMemoryState> {
        let storage = self.memory_storage();
        self.refresh_recent_memory_state_in(window, &storage).await
    }
}
