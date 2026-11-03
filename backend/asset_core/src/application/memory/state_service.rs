use crate::backend::application::prelude::*;
use crate::backend::domain::memory::{
    MemoryCommitResult, ProjectMemoryState, RecentMemoryState, TopicEvolutionCommitPayload,
};
use crate::backend::infrastructure::memory::projection::{
    sync_project_markdown, sync_recent_markdown,
};
use crate::backend::infrastructure::memory::storage::MemoryStorage;

impl AppService {
    /// Commit an evolution turn to a project's memory topic tree and atomically project to Markdown in specified storage.
    pub async fn commit_memory_topic_evolution_to(
        &self,
        payload: TopicEvolutionCommitPayload,
        storage: &MemoryStorage,
    ) -> AppResult<MemoryCommitResult> {
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

        // 5. Update Recent memory state and its Markdown projection
        let mut recent_state = storage
            .read_recent_state()
            .map_err(AppError::from)?
            .unwrap_or_default();
        recent_state.upsert_project(project_state);

        storage
            .write_recent_state_atomic(&recent_state)
            .map_err(AppError::from)?;
        sync_recent_markdown(storage, &recent_state).map_err(AppError::from)?;

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

    /// Get strong-typed recent memory state.
    pub async fn get_recent_memory_state(&self) -> AppResult<Option<RecentMemoryState>> {
        let storage = MemoryStorage::new(self.tenant_id());
        storage.read_recent_state().map_err(AppError::from)
    }
}

#[cfg(test)]
#[path = "state_service_tests.rs"]
mod tests;
