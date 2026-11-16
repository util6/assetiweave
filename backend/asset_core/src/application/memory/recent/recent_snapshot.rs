use crate::backend::application::memory::state_loader::load_project_state;
use crate::backend::application::prelude::*;
use crate::backend::domain::memory::{
    MemoryProjectStateView, MemoryTreeNode, NodeStatus, ProjectMemoryState, RecentMemoryItemView,
    RecentMemorySnapshotView, RecentMemoryStateView, RecentMemoryStatus, RecentNodeStatus,
    RecentProjectView, RecentSessionReferenceView, RecentSnapshotPublicationKind, RecentStateFile,
    RecentStateProject, RecentTopicView, RecentTreeNodeView, SourceAvailability,
};
use crate::backend::infrastructure::memory::storage::MemoryStorage;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
struct SessionMeta {
    id: String,
    title: String,
    adapter_id: String,
    project_path: Option<String>,
    available: bool,
}

fn friendly_project_title(project_path: Option<&str>, fallback_key: &str) -> String {
    if let Some(path) = project_path {
        let trimmed = path.trim().trim_end_matches(['/', '\\']);
        if !trimmed.is_empty() {
            if let Some(name) = std::path::Path::new(trimmed)
                .file_name()
                .and_then(|s| s.to_str())
                .filter(|s| !s.is_empty())
            {
                return name.to_string();
            }
        }
    }
    fallback_key.to_string()
}

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
            let mut session_ids = HashSet::new();

            for topic in &proj.topics {
                for node in &topic.nodes {
                    let short_id = node.id.split('#').next().unwrap_or(&node.id).to_string();
                    if !short_id.is_empty() {
                        session_ids.insert(short_id.clone());
                    }

                    let item_id = format!("{}-{}", proj.project_key, items.len() + 1);
                    let occurred_at = node.recorded_at.clone().unwrap_or_else(|| now.clone());
                    let session_id = node
                        .source_session_id
                        .clone()
                        .unwrap_or_else(|| short_id.clone());
                    let session_title = node
                        .session_title
                        .clone()
                        .unwrap_or_else(|| format!("会话 {}", short_id));
                    let source_agent = node
                        .source_agent
                        .clone()
                        .unwrap_or_else(|| "agent".to_string());
                    let available = node.session_available.unwrap_or(true);

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
                        occurred_at: occurred_at.clone(),
                        recommendation_rank: None,
                        source_availability: if available {
                            SourceAvailability::Available
                        } else {
                            SourceAvailability::Unavailable
                        },
                        session_references: vec![RecentSessionReferenceView {
                            source_id: short_id.clone(),
                            session_id,
                            session_title,
                            source_agent,
                            last_activity_at: occurred_at,
                            available,
                            unavailable_reason: None,
                        }],
                    });
                }
            }

            let project_title = proj
                .project_path
                .clone()
                .filter(|p| !p.trim().is_empty())
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
            target_watermark: now.clone(),
            window_start: now.clone(),
            window_end: now.clone(),
            window_hours: 48,
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

fn convert_project_state_to_topics(p_state: &ProjectMemoryState) -> Vec<RecentTopicView> {
    let mut topics = Vec::new();
    let mut topic_keys: Vec<String> = p_state.topic_trees.keys().cloned().collect();
    topic_keys.sort();

    for t_key in topic_keys {
        if let Some(tree) = p_state.topic_trees.get(&t_key) {
            let mut node_keys: Vec<String> = tree.nodes.keys().cloned().collect();
            node_keys.sort();

            let mut nodes: Vec<RecentTreeNodeView> = node_keys
                .into_iter()
                .filter_map(|n_id| tree.nodes.get(&n_id))
                .filter_map(|node| {
                    let status = match node.status {
                        NodeStatus::Completed => RecentNodeStatus::Completed,
                        NodeStatus::Continued => RecentNodeStatus::Continued,
                        NodeStatus::Unexplored => RecentNodeStatus::Unexplored,
                        NodeStatus::Dismissed => return None,
                    };
                    let recorded_at = node
                        .turn_started_at
                        .as_deref()
                        .filter(|s| !s.trim().is_empty())
                        .map(str::to_string)
                        .or_else(|| Some(node.created_at.clone()));
                    Some(RecentTreeNodeView {
                        id: node.id.clone(),
                        parent_id: node.parent_id.clone(),
                        status,
                        label: node.summary.clone(),
                        summary: node.summary.clone(),
                        recorded_at,
                        source_session_id: Some(node.source_session_id.clone()),
                        turn_index: Some(node.turn_index),
                        session_title: None,
                        source_agent: None,
                        session_available: None,
                        parent_summary: None,
                    })
                })
                .collect();

            nodes.sort_by(|left, right| {
                let left_ts = left.recorded_at.as_deref().unwrap_or("");
                let right_ts = right.recorded_at.as_deref().unwrap_or("");
                right_ts.cmp(left_ts).then_with(|| left.id.cmp(&right.id))
            });

            if !nodes.is_empty() {
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
    }

    topics
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
            let topics = convert_project_state_to_topics(p_state);
            projects.push(RecentStateProject {
                project_key: p_state.project_slug.clone(),
                project_path: None,
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
    async fn resolve_session_meta_for_node(
        &self,
        short_id: &str,
        full_session_id: Option<&str>,
    ) -> Option<SessionMeta> {
        let lookup = full_session_id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(short_id)
            .trim();
        if lookup.is_empty() && short_id.trim().is_empty() {
            return None;
        }
        let tenant_id = self.tenant_id();
        let frag = if !short_id.trim().is_empty() {
            short_id.trim().to_ascii_lowercase()
        } else {
            crate::backend::domain::conversation_id_fragment(lookup)
        };
        let domain_prefix = format!("conversation-session-{}%", frag);
        let raw_prefix = format!("{}%", frag);

        #[derive(sqlx::FromRow)]
        struct SessionRow {
            id: String,
            title: String,
            adapter_id: String,
            project_path: Option<String>,
            last_cwd: Option<String>,
            missing: i64,
        }

        let row = sqlx::query_as::<_, SessionRow>(
            "SELECT id, title, adapter_id, project_path, last_cwd, missing \
             FROM conversation_sessions \
             WHERE tenant_id = ?1 \
               AND (id = ?2 OR id LIKE ?3 OR id LIKE ?4) \
             ORDER BY CASE WHEN id = ?2 THEN 0 ELSE 1 END, user_visible DESC, missing ASC \
             LIMIT 1",
        )
        .bind(tenant_id)
        .bind(lookup)
        .bind(&domain_prefix)
        .bind(&raw_prefix)
        .fetch_optional(self.db.pool())
        .await
        .ok()
        .flatten()?;

        let resolved_path = row
            .last_cwd
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                row.project_path
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            })
            .map(str::to_string);

        Some(SessionMeta {
            id: row.id,
            title: row.title,
            adapter_id: row.adapter_id,
            project_path: resolved_path,
            available: row.missing == 0,
        })
    }

    async fn enrich_project_topics(
        &self,
        storage: &MemoryStorage,
        project_key: &str,
        project_path: &mut Option<String>,
        project_title: &mut String,
        topics: &mut [RecentTopicView],
        include_parent_lookup: bool,
    ) {
        let mut session_cache: HashMap<String, Option<SessionMeta>> = HashMap::new();
        let full_project_state = if include_parent_lookup {
            load_project_state(storage, project_key).ok().flatten()
        } else {
            None
        };

        for topic in topics.iter_mut() {
            let visible_ids: HashSet<String> = topic.nodes.iter().map(|n| n.id.clone()).collect();

            for node in topic.nodes.iter_mut() {
                let short_id = node.id.split('#').next().unwrap_or(&node.id).trim();
                let cache_key = node
                    .source_session_id
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(short_id)
                    .to_string();

                if !cache_key.is_empty() && !session_cache.contains_key(&cache_key) {
                    let meta = self
                        .resolve_session_meta_for_node(short_id, node.source_session_id.as_deref())
                        .await;
                    session_cache.insert(cache_key.clone(), meta);
                }

                if let Some(Some(meta)) = session_cache.get(&cache_key) {
                    node.source_session_id = Some(meta.id.clone());
                    if !meta.title.trim().is_empty() {
                        node.session_title = Some(meta.title.clone());
                    }
                    if !meta.adapter_id.trim().is_empty() {
                        node.source_agent = Some(meta.adapter_id.clone());
                    }
                    node.session_available = Some(meta.available);
                    if project_path.is_none() {
                        if let Some(ref resolved_path) = meta.project_path {
                            *project_path = Some(resolved_path.clone());
                        }
                    }
                }

                if node.parent_summary.is_none() {
                    if let Some(ref parent_id) = node.parent_id {
                        if !visible_ids.contains(parent_id) {
                            if let Some(ref full_proj) = full_project_state {
                                if let Some(tree) = full_proj.topic_trees.get(&topic.topic_id) {
                                    if let Some(parent_node) = tree.nodes.get(parent_id) {
                                        node.parent_summary = Some(parent_node.summary.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if project_title == project_key {
            if let Some(ref resolved_path) = *project_path {
                *project_title = friendly_project_title(Some(resolved_path), project_key);
            }
        }
    }

    pub async fn get_recent_memory_snapshot(&self) -> AppResult<RecentMemoryStateView> {
        let storage = self.memory_storage();
        self.get_recent_memory_snapshot_with_storage(&storage).await
    }

    pub(crate) async fn get_recent_memory_snapshot_with_storage(
        &self,
        storage: &MemoryStorage,
    ) -> AppResult<RecentMemoryStateView> {
        let recent_state_path = storage.root_dir().join(".state").join("recent.json");
        let configured_window = self
            .backend_settings()
            .ok()
            .map(|s| s.memory.recent_window_hours as i64)
            .filter(|&h| h > 0)
            .unwrap_or(48);

        if recent_state_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&recent_state_path) {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    if let Ok(state_file) = serde_json::from_str::<RecentStateFile>(trimmed) {
                        if !state_file.projects.is_empty() {
                            let mut view = convert_recent_state_file_to_view(state_file);
                            if let Some(ref mut snapshot) = view.snapshot {
                                snapshot.window_hours = configured_window;
                                for proj in snapshot.projects.iter_mut() {
                                    self.enrich_project_topics(
                                        storage,
                                        &proj.project_key,
                                        &mut proj.project_path,
                                        &mut proj.project_title,
                                        &mut proj.topics,
                                        true,
                                    )
                                    .await;
                                }
                            }
                            return Ok(view);
                        }
                    } else if let Ok(mem_state) = serde_json::from_str::<
                        crate::backend::domain::memory::RecentMemoryState,
                    >(trimmed)
                    {
                        let effective_window = mem_state.window_hours.unwrap_or(configured_window);
                        let state_file = convert_recent_memory_state_to_file(mem_state);
                        if !state_file.projects.is_empty() {
                            let mut view = convert_recent_state_file_to_view(state_file);
                            if let Some(ref mut snapshot) = view.snapshot {
                                snapshot.window_hours = effective_window;
                                for proj in snapshot.projects.iter_mut() {
                                    self.enrich_project_topics(
                                        storage,
                                        &proj.project_key,
                                        &mut proj.project_path,
                                        &mut proj.project_title,
                                        &mut proj.topics,
                                        true,
                                    )
                                    .await;
                                }
                            }
                            return Ok(view);
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

    pub async fn get_memory_project_state_view(
        &self,
        project_slug: &str,
    ) -> AppResult<Option<MemoryProjectStateView>> {
        let storage = self.memory_storage();
        self.get_memory_project_state_view_with_storage(project_slug, &storage)
            .await
    }

    pub(crate) async fn get_memory_project_state_view_with_storage(
        &self,
        project_slug: &str,
        storage: &MemoryStorage,
    ) -> AppResult<Option<MemoryProjectStateView>> {
        let Some(p_state) = load_project_state(storage, project_slug)? else {
            return Ok(None);
        };

        let mut topics = convert_project_state_to_topics(&p_state);
        let mut project_path: Option<String> = None;
        let mut project_title = p_state.project_slug.clone();

        self.enrich_project_topics(
            storage,
            &p_state.project_slug,
            &mut project_path,
            &mut project_title,
            &mut topics,
            false,
        )
        .await;

        Ok(Some(MemoryProjectStateView {
            project_key: p_state.project_slug,
            project_title,
            project_path,
            updated_at: p_state.updated_at,
            watermark_count: p_state.session_watermarks.len(),
            topics,
        }))
    }

    pub async fn update_memory_node_status_for_ui(
        &self,
        params: MemoryNodeStatusUpdateParams,
    ) -> AppResult<MemoryTreeNode> {
        self.update_memory_node_status(
            &params.node_id,
            params.status,
            params.project_slug.as_deref(),
            params.window.as_deref(),
        )
        .await
    }

    pub async fn refresh_memory_recent_state_view(
        &self,
        params: MemoryRecentRefreshParams,
    ) -> AppResult<RecentMemoryStateView> {
        self.refresh_recent_memory_state(params.window.as_deref())
            .await?;
        self.get_recent_memory_snapshot().await
    }
}

#[cfg(test)]
#[path = "recent_snapshot_tests.rs"]
mod tests;
