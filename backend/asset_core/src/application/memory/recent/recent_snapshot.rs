use crate::backend::application::prelude::*;
use crate::backend::domain::memory::{
    RecentMemoryItemView, RecentMemorySnapshotView, RecentMemoryStateView, RecentMemoryStatus,
    RecentNodeStatus, RecentProjectView, RecentSessionReferenceView, RecentSnapshotPublicationKind,
    RecentStateFile, RecentStateProject, RecentTopicView, RecentTreeNodeView, SourceAvailability,
};

/// Convert a `RecentStateFile` into the presentation `RecentMemoryStateView`.
pub fn convert_recent_state_file_to_view(state_file: RecentStateFile) -> RecentMemoryStateView {
    if state_file.projects.is_empty() {
        return RecentMemoryStateView {
            status: RecentMemoryStatus::Empty,
            snapshot: None,
            latest_attempt_task_id: None,
            latest_attempt_error: None,
        };
    }

    let now = if !state_file.updated_at.is_empty() {
        state_file.updated_at.clone()
    } else {
        chrono::Utc::now().to_rfc3339()
    };

    let projects: Vec<RecentProjectView> = state_file
        .projects
        .into_iter()
        .map(|proj| {
            let mut items = Vec::new();
            let mut session_ids = std::collections::HashSet::new();

            for topic in &proj.topics {
                for node in &topic.nodes {
                    let short_id = node.id.split('#').next().unwrap_or(&node.id).to_string();
                    if !short_id.is_empty() {
                        session_ids.insert(short_id.clone());
                    }

                    let item_id = format!("{}-{}", proj.project_key, items.len() + 1);
                    items.push(RecentMemoryItemView {
                        item_id,
                        revision_id: "rev-1".to_string(),
                        category: "recent_task".to_string(),
                        status: match node.status {
                            RecentNodeStatus::Completed => "completed".to_string(),
                            RecentNodeStatus::Continued => "continued".to_string(),
                            RecentNodeStatus::Unexplored => "unexplored".to_string(),
                        },
                        title: if node.label.is_empty() {
                            node.summary.clone()
                        } else {
                            node.label.clone()
                        },
                        summary: node.summary.clone(),
                        rationale: String::new(),
                        occurred_at: node.recorded_at.clone().unwrap_or_else(|| now.clone()),
                        recommendation_rank: Some(items.len() as i64 + 1),
                        source_availability: SourceAvailability::Available,
                        session_references: vec![RecentSessionReferenceView {
                            source_id: short_id.clone(),
                            session_id: short_id.clone(),
                            session_title: format!("会话 {}", short_id),
                            source_agent: "agent".to_string(),
                            last_activity_at: node
                                .recorded_at
                                .clone()
                                .unwrap_or_else(|| now.clone()),
                            available: true,
                            unavailable_reason: None,
                        }],
                    });
                }
            }

            let project_title = proj
                .project_path
                .clone()
                .unwrap_or_else(|| proj.project_key.clone());
            let latest_activity_at = if !proj.updated_at.is_empty() {
                proj.updated_at
            } else {
                now.clone()
            };

            RecentProjectView {
                project_key: proj.project_key,
                project_title,
                project_path: proj.project_path,
                summary: format!("{} topics", proj.topics.len()),
                no_material_change: false,
                latest_activity_at,
                source_session_count: session_ids.len() as i64,
                items,
                topics: proj.topics,
            }
        })
        .collect();

    RecentMemoryStateView {
        status: RecentMemoryStatus::Ready,
        snapshot: Some(RecentMemorySnapshotView {
            snapshot_id: "recent-state-snapshot".to_string(),
            sequence: 1,
            target_watermark: "state".to_string(),
            window_start: now.clone(),
            window_end: now.clone(),
            window_hours: 24,
            publication_kind: RecentSnapshotPublicationKind::Generated,
            reused_from_snapshot_id: None,
            content_generated_at: now.clone(),
            published_at: now,
            projects,
        }),
        latest_attempt_task_id: None,
        latest_attempt_error: None,
    }
}

/// Convert internal `RecentMemoryState` into `RecentStateFile` for seamless interoperability.
pub fn convert_recent_memory_state_to_file(
    state: crate::backend::domain::memory::RecentMemoryState,
) -> RecentStateFile {
    let mut projects = Vec::new();
    let mut project_keys: Vec<String> = state.projects.keys().cloned().collect();
    project_keys.sort();

    for p_key in project_keys {
        if let Some(p_state) = state.projects.get(&p_key) {
            let mut topics = Vec::new();
            let mut topic_keys: Vec<String> = p_state.topic_trees.keys().cloned().collect();
            topic_keys.sort();

            for t_key in topic_keys {
                if let Some(tree) = p_state.topic_trees.get(&t_key) {
                    let mut node_keys: Vec<String> = tree.nodes.keys().cloned().collect();
                    node_keys.sort();

                    let nodes: Vec<RecentTreeNodeView> = node_keys
                        .into_iter()
                        .filter_map(|n_id| tree.nodes.get(&n_id))
                        .filter_map(|node| {
                            let status = match node.status {
                                crate::backend::domain::memory::tree::NodeStatus::Completed => {
                                    RecentNodeStatus::Completed
                                }
                                crate::backend::domain::memory::tree::NodeStatus::Continued => {
                                    RecentNodeStatus::Continued
                                }
                                crate::backend::domain::memory::tree::NodeStatus::Unexplored => {
                                    RecentNodeStatus::Unexplored
                                }
                                crate::backend::domain::memory::tree::NodeStatus::Dismissed => {
                                    return None;
                                }
                            };
                            Some(RecentTreeNodeView {
                                id: node.id.clone(),
                                parent_id: node.parent_id.clone(),
                                status,
                                label: node.summary.clone(),
                                summary: node.summary.clone(),
                                recorded_at: Some(node.created_at.clone()),
                            })
                        })
                        .collect();

                    topics.push(RecentTopicView {
                        topic_id: tree.topic_id.clone(),
                        topic: if tree.topic_name.is_empty() {
                            tree.topic_id.clone()
                        } else {
                            tree.topic_name.clone()
                        },
                        updated_at: p_state.updated_at.clone(),
                        nodes,
                    });
                }
            }

            projects.push(RecentStateProject {
                project_key: p_state.project_slug.clone(),
                project_path: Some(p_state.project_slug.clone()),
                updated_at: p_state.updated_at.clone(),
                topics,
            });
        }
    }

    RecentStateFile {
        schema_version: 1,
        updated_at: state.updated_at,
        projects,
    }
}

impl AppService {
    pub async fn get_recent_memory_snapshot(&self) -> AppResult<RecentMemoryStateView> {
        let storage = self.memory_storage();
        self.get_recent_memory_snapshot_with_storage(&storage).await
    }

    pub(crate) async fn get_recent_memory_snapshot_with_storage(
        &self,
        storage: &crate::backend::infrastructure::memory::storage::MemoryStorage,
    ) -> AppResult<RecentMemoryStateView> {
        let recent_state_path = storage.root_dir().join(".state").join("recent.json");
        if recent_state_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&recent_state_path) {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    if let Ok(state_file) = serde_json::from_str::<RecentStateFile>(trimmed) {
                        if !state_file.projects.is_empty() {
                            return Ok(convert_recent_state_file_to_view(state_file));
                        }
                    } else if let Ok(mem_state) = serde_json::from_str::<
                        crate::backend::domain::memory::RecentMemoryState,
                    >(trimmed)
                    {
                        let state_file = convert_recent_memory_state_to_file(mem_state);
                        if !state_file.projects.is_empty() {
                            return Ok(convert_recent_state_file_to_view(state_file));
                        }
                    } else {
                        tracing::warn!(
                            path = %recent_state_path.display(),
                            "failed to parse .state/recent.json into RecentStateFile or RecentMemoryState; returning empty state"
                        );
                    }
                }
            }
        }

        Ok(RecentMemoryStateView {
            status: RecentMemoryStatus::Empty,
            snapshot: None,
            latest_attempt_task_id: None,
            latest_attempt_error: None,
        })
    }
}

#[cfg(test)]
#[path = "recent_snapshot_tests.rs"]
mod tests;
