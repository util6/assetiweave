use super::state_loader::{load_all_project_states, load_project_state};
use crate::backend::application::prelude::*;
use crate::backend::domain::memory::{
    MemoryCommitResult, MemoryTreeNode, NodeStatus, ProjectMemoryState, RecentMemoryState,
    TopicEvolutionCommitPayload,
};
use crate::backend::infrastructure::memory::projection::sync_project_markdown;
use crate::backend::infrastructure::memory::storage::MemoryStorage;
use std::collections::HashMap;

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

        // 1. Read existing project state (legacy slugs merged), or initialize new
        let mut project_state =
            load_project_state(storage, &slug)?.unwrap_or_else(|| ProjectMemoryState::new(&slug));

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
        let storage = self.memory_storage();
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
            let state = load_project_state(storage, &slug)?
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
        let storage = self.memory_storage();
        self.update_memory_node_status_in(node_id, status, project_slug, window, &storage)
            .await
    }

    /// Advance a session's watermark without modifying the topic tree in specified storage.
    pub async fn advance_memory_session_watermark_in(
        &self,
        project_slug: &str,
        session_id: &str,
        turn: u32,
        storage: &MemoryStorage,
    ) -> AppResult<()> {
        let mut project_state = load_project_state(storage, project_slug)?
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
        let storage = self.memory_storage();
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
        let project_state = load_project_state(storage, project_slug)?;

        Ok(project_state.and_then(|s| s.get_watermark(session_id)))
    }

    /// Check if a session has an existing watermark recorded for a project.
    pub async fn check_memory_session_watermark(
        &self,
        project_slug: &str,
        session_id: &str,
    ) -> AppResult<Option<u32>> {
        let storage = self.memory_storage();
        self.check_memory_session_watermark_in(project_slug, session_id, &storage)
            .await
    }

    /// Get strong-typed project memory state from specified storage.
    pub async fn get_project_memory_state_in(
        &self,
        project_slug: &str,
        storage: &MemoryStorage,
    ) -> AppResult<Option<ProjectMemoryState>> {
        load_project_state(storage, project_slug)
    }

    /// Get strong-typed project memory state.
    pub async fn get_project_memory_state(
        &self,
        project_slug: &str,
    ) -> AppResult<Option<ProjectMemoryState>> {
        let storage = self.memory_storage();
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
        let storage = self.memory_storage();
        self.get_recent_memory_state_in(&storage).await
    }

    /// 全部项目记忆的会话水位索引（8 位会话短 ID → 已处理轮次）。
    ///
    /// 供会话列表、CLI 视图与「未处理会话发现」批量判定使用：一次遍历所有项目
    /// 状态即可回答任意多会话的 `watermark`，避免每行都去读状态文件
    /// （历史未折叠 slug 的收敛写入也会被重复触发）。同一会话多处记录时取最大值。
    pub async fn memory_session_watermark_index(&self) -> AppResult<HashMap<String, u32>> {
        let storage = self.memory_storage();
        self.memory_session_watermark_index_in(&storage).await
    }

    pub async fn memory_session_watermark_index_in(
        &self,
        storage: &MemoryStorage,
    ) -> AppResult<HashMap<String, u32>> {
        let mut index: HashMap<String, u32> = HashMap::new();
        for project in load_all_project_states(storage)? {
            for (session_id, watermark) in project.session_watermarks {
                let short_id = crate::backend::domain::conversation_id_fragment(&session_id);
                if short_id.is_empty() {
                    continue;
                }
                index
                    .entry(short_id)
                    .and_modify(|current| *current = (*current).max(watermark))
                    .or_insert(watermark);
            }
        }
        Ok(index)
    }
}

#[cfg(test)]
#[path = "state_service_tests.rs"]
mod tests;
