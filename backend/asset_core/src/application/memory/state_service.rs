use crate::backend::application::prelude::*;
use crate::backend::domain::memory::{
    MemoryCommitResult, MemoryTreeNode, NodeStatus, ProjectMemoryState, RecentMemoryState,
    TopicEvolutionCommitPayload, TopicTree,
};
use crate::backend::infrastructure::memory::projection::{
    sync_project_markdown, sync_recent_markdown,
};
use crate::backend::infrastructure::memory::storage::MemoryStorage;
use std::collections::HashMap;

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

fn load_all_project_states(storage: &MemoryStorage) -> AppResult<Vec<ProjectMemoryState>> {
    let dir = storage.state_projects_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .map_err(AppError::from)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    entries.sort_by_key(|e| e.path());

    let mut states = Vec::with_capacity(entries.len());
    for entry in entries {
        if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
            if let Ok(Some(state)) = storage.read_project_state(stem) {
                states.push(state);
            }
        }
    }
    Ok(states)
}

fn find_and_update_node_in_project(
    state: &mut ProjectMemoryState,
    node_id: &str,
    status: NodeStatus,
) -> Option<MemoryTreeNode> {
    for tree in state.topic_trees.values_mut() {
        if let Some(node) = tree.nodes.get_mut(node_id) {
            node.status = status;
            node.updated_at = chrono::Utc::now().to_rfc3339();
            state.updated_at = chrono::Utc::now().to_rfc3339();
            return Some(node.clone());
        }
    }
    None
}

impl AppService {
    /// Query SQLite for started_at of a turn, falling back to session started_at or now.
    async fn resolve_turn_started_at(&self, session_id: &str, turn_index: u32) -> String {
        let tenant_id = self.tenant_id();
        let prefix = format!("{}%", session_id);

        // 1. Try conversation_turns
        let turn_res: Result<Option<String>, _> = sqlx::query_scalar(
            "SELECT started_at FROM conversation_turns \
             WHERE tenant_id = ?1 AND (session_id = ?2 OR session_id LIKE ?3) AND turn_index = ?4 \
             AND started_at IS NOT NULL AND started_at != '' \
             ORDER BY CASE WHEN session_id = ?2 THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(tenant_id)
        .bind(session_id)
        .bind(&prefix)
        .bind(turn_index as i64)
        .fetch_optional(self.db.pool())
        .await;

        if let Ok(Some(ts)) = turn_res {
            if !ts.trim().is_empty() {
                return ts;
            }
        }

        // 2. Try conversation_sessions
        let sess_res: Result<Option<String>, _> = sqlx::query_scalar(
            "SELECT started_at FROM conversation_sessions \
             WHERE tenant_id = ?1 AND (id = ?2 OR id LIKE ?3) \
             AND started_at IS NOT NULL AND started_at != '' \
             ORDER BY CASE WHEN id = ?2 THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(tenant_id)
        .bind(session_id)
        .bind(&prefix)
        .fetch_optional(self.db.pool())
        .await;

        if let Ok(Some(ts)) = sess_res {
            if !ts.trim().is_empty() {
                return ts;
            }
        }

        // 3. Fallback to Utc::now()
        chrono::Utc::now().to_rfc3339()
    }

    /// Commit an evolution turn to a project's memory topic tree and atomically project to Markdown in specified storage.
    pub async fn commit_memory_topic_evolution_to(
        &self,
        payload: TopicEvolutionCommitPayload,
        storage: &MemoryStorage,
    ) -> AppResult<MemoryCommitResult> {
        let mut payload = payload;

        // Auto-fill turn_started_at if None
        if payload.new_node.turn_started_at.is_none() {
            let ts = self
                .resolve_turn_started_at(
                    &payload.new_node.source_session_id,
                    payload.new_node.turn_index,
                )
                .await;
            payload.new_node.turn_started_at = Some(ts);
        }

        for sib in &mut payload.unexplored_siblings {
            if sib.turn_started_at.is_none() {
                let ts = self
                    .resolve_turn_started_at(&sib.source_session_id, sib.turn_index)
                    .await;
                sib.turn_started_at = Some(ts);
            }
        }

        let slug = payload.project_slug.clone();

        // 1. Read existing project state, or initialize new
        let mut project_state = storage
            .read_project_state(&slug)
            .map_err(AppError::from)?
            .unwrap_or_else(|| ProjectMemoryState::new(&slug));

        // 2. Apply evolution to domain model
        let result = project_state
            .apply_evolution(payload)
            .map_err(AppError::Validation)?;

        // 3. Atomically write project state JSON
        storage
            .write_project_state_atomic(&slug, &project_state)
            .map_err(AppError::from)?;

        // 4. One-way Markdown projection for project
        sync_project_markdown(storage, &project_state).map_err(AppError::from)?;

        // 5. Update Recent memory state with windowing and its Markdown projection
        self.refresh_recent_memory_state_in(None, storage).await?;

        Ok(result)
    }

    /// Commit an evolution turn to a project's memory topic tree and atomically project to Markdown.
    pub async fn commit_memory_topic_evolution(
        &self,
        payload: TopicEvolutionCommitPayload,
    ) -> AppResult<MemoryCommitResult> {
        let storage = MemoryStorage::new(self.tenant_id());
        self.commit_memory_topic_evolution_to(payload, &storage)
            .await
    }

    /// Update status of a memory node and trigger projections in specified storage.
    pub async fn update_memory_node_status_in(
        &self,
        node_id: &str,
        status: NodeStatus,
        project_slug: Option<&str>,
        window: Option<&str>,
        storage: &MemoryStorage,
    ) -> AppResult<MemoryTreeNode> {
        let (mut project_state, slug) = if let Some(raw_slug) = project_slug {
            let slug = if raw_slug.contains('/') || raw_slug.contains('\\') {
                crate::backend::application::memory::project_path_to_slug(raw_slug)
            } else {
                raw_slug.to_string()
            };
            let state = storage
                .read_project_state(&slug)
                .map_err(AppError::from)?
                .ok_or_else(|| AppError::NotFound(format!("Project not found: {}", slug)))?;
            (state, slug)
        } else {
            let projects = load_all_project_states(storage)?;
            let mut found_match = None;
            for state in projects {
                if state
                    .topic_trees
                    .values()
                    .any(|t| t.nodes.contains_key(node_id))
                {
                    found_match = Some((state.clone(), state.project_slug.clone()));
                    break;
                }
            }
            found_match.ok_or_else(|| {
                AppError::NotFound(format!(
                    "Memory node not found across projects: {}",
                    node_id
                ))
            })?
        };

        let updated_node = find_and_update_node_in_project(&mut project_state, node_id, status)
            .ok_or_else(|| {
                AppError::NotFound(format!("Node {} not found in project {}", node_id, slug))
            })?;

        storage
            .write_project_state_atomic(&slug, &project_state)
            .map_err(AppError::from)?;
        sync_project_markdown(storage, &project_state).map_err(AppError::from)?;
        self.refresh_recent_memory_state_in(window, storage).await?;

        Ok(updated_node)
    }

    /// Update status of a memory node and trigger projections.
    pub async fn update_memory_node_status(
        &self,
        node_id: &str,
        status: NodeStatus,
        project_slug: Option<&str>,
        window: Option<&str>,
    ) -> AppResult<MemoryTreeNode> {
        let storage = MemoryStorage::new(self.tenant_id());
        self.update_memory_node_status_in(node_id, status, project_slug, window, &storage)
            .await
    }

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
        let storage = MemoryStorage::new(self.tenant_id());
        self.refresh_recent_memory_state_in(window, &storage).await
    }

    /// Advance a session's watermark without modifying the topic tree in specified storage.
    pub async fn advance_memory_session_watermark_in(
        &self,
        project_slug: &str,
        session_id: &str,
        turn: u32,
        storage: &MemoryStorage,
    ) -> AppResult<()> {
        let mut project_state = storage
            .read_project_state(project_slug)
            .map_err(AppError::from)?
            .unwrap_or_else(|| ProjectMemoryState::new(project_slug));

        project_state.set_watermark(session_id, turn);

        storage
            .write_project_state_atomic(project_slug, &project_state)
            .map_err(AppError::from)?;

        Ok(())
    }

    /// Advance a session's watermark without modifying the topic tree.
    pub async fn advance_memory_session_watermark(
        &self,
        project_slug: &str,
        session_id: &str,
        turn: u32,
    ) -> AppResult<()> {
        let storage = MemoryStorage::new(self.tenant_id());
        self.advance_memory_session_watermark_in(project_slug, session_id, turn, &storage)
            .await
    }

    /// Check if a session has an existing watermark recorded for a project in specified storage.
    pub async fn check_memory_session_watermark_in(
        &self,
        project_slug: &str,
        session_id: &str,
        storage: &MemoryStorage,
    ) -> AppResult<Option<u32>> {
        let project_state = storage
            .read_project_state(project_slug)
            .map_err(AppError::from)?;

        Ok(project_state.and_then(|s| s.get_watermark(session_id)))
    }

    /// Check if a session has an existing watermark recorded for a project.
    pub async fn check_memory_session_watermark(
        &self,
        project_slug: &str,
        session_id: &str,
    ) -> AppResult<Option<u32>> {
        let storage = MemoryStorage::new(self.tenant_id());
        self.check_memory_session_watermark_in(project_slug, session_id, &storage)
            .await
    }

    /// Get strong-typed project memory state from specified storage.
    pub async fn get_project_memory_state_in(
        &self,
        project_slug: &str,
        storage: &MemoryStorage,
    ) -> AppResult<Option<ProjectMemoryState>> {
        storage
            .read_project_state(project_slug)
            .map_err(AppError::from)
    }

    /// Get strong-typed project memory state.
    pub async fn get_project_memory_state(
        &self,
        project_slug: &str,
    ) -> AppResult<Option<ProjectMemoryState>> {
        let storage = MemoryStorage::new(self.tenant_id());
        self.get_project_memory_state_in(project_slug, &storage)
            .await
    }

    /// Get strong-typed recent memory state from specified storage.
    pub async fn get_recent_memory_state_in(
        &self,
        storage: &MemoryStorage,
    ) -> AppResult<Option<RecentMemoryState>> {
        storage.read_recent_state().map_err(AppError::from)
    }

    /// Get strong-typed recent memory state.
    pub async fn get_recent_memory_state(&self) -> AppResult<Option<RecentMemoryState>> {
        let storage = MemoryStorage::new(self.tenant_id());
        self.get_recent_memory_state_in(&storage).await
    }
}

#[cfg(test)]
#[path = "state_service_tests.rs"]
mod tests;
