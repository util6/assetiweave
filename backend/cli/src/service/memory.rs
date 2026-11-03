use anyhow::{Context, Result};
use asset_core::application::AppService;
use asset_core::application::{
    MemoryContextResolveParams, MemoryRecallSearchParams, MemoryRecallSessionCreateParams,
    MemoryRecallSessionGetParams, MemoryRecallTurnCancelParams, MemoryRecallTurnSendParams,
    MemoryScopeRebuildParams, MemoryTaskGetParams, MemoryTaskListParams, MemoryTaskRetryParams,
};
use asset_core::domain::MemoryScope;
use serde_json::Value;

pub struct MemoryOperations<'a> {
    app_service: &'a AppService,
}

impl<'a> MemoryOperations<'a> {
    pub fn new(app_service: &'a AppService) -> Self {
        Self { app_service }
    }

    pub async fn get_recent_snapshot(&self, date: Option<String>) -> Result<Value> {
        let view = self
            .app_service
            .get_recent_memory_markdown_view(date.as_deref())
            .await
            .context("failed to get recent memory markdown")?;
        Ok(serde_json::to_value(view)?)
    }

    pub async fn resolve_context(
        &self,
        project_path: Option<String>,
        query: Option<String>,
        token_budget: usize,
    ) -> Result<Value> {
        let params = MemoryContextResolveParams {
            project_path,
            query,
            token_budget: Some(token_budget),
        };
        let result = self
            .app_service
            .resolve_memory_context(params)
            .await
            .context("failed to resolve memory context")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn get_project(&self, project_path: String) -> Result<Value> {
        let view = self
            .app_service
            .get_project_memory_markdown_view(&project_path)
            .await
            .context("failed to get project memory markdown")?;
        Ok(serde_json::to_value(view)?)
    }

    pub async fn rebuild(&self, params: MemoryScopeRebuildParams) -> Result<Value> {
        let result = self
            .app_service
            .rebuild_memory_scope(params)
            .await
            .context("failed to rebuild memory")?;

        for task_id in &result.scheduled_task_ids {
            let _ = self.app_service.wait_for_memory_task(task_id).await;
        }

        Ok(serde_json::to_value(result)?)
    }

    pub fn list_tasks(&self, active_only: bool) -> Result<Value> {
        let params = MemoryTaskListParams { active_only };
        let result = self
            .app_service
            .list_memory_task_views(params)
            .context("failed to list memory tasks")?;
        Ok(serde_json::to_value(result)?)
    }

    pub fn get_task(&self, task_id: String) -> Result<Value> {
        let params = MemoryTaskGetParams { task_id };
        let result = self
            .app_service
            .get_memory_task_view(params)
            .context("failed to get memory task")?;
        Ok(serde_json::to_value(result)?)
    }

    pub fn cancel_task(&self, task_id: String) -> Result<Value> {
        let params = MemoryTaskGetParams { task_id };
        let result = self
            .app_service
            .cancel_memory_task_view(params)
            .context("failed to cancel memory task")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn retry_task(&self, task_id: String) -> Result<Value> {
        let params = MemoryTaskRetryParams { task_id };
        let result = self
            .app_service
            .retry_memory_task(params)
            .await
            .context("failed to retry memory task")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn search_recall(&self, params: MemoryRecallSearchParams) -> Result<Value> {
        let result = self
            .app_service
            .search_memory_recall(params)
            .await
            .context("failed to search memory recall")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn create_recall_session(&self, scope: MemoryScope) -> Result<Value> {
        let params = MemoryRecallSessionCreateParams { scope };
        let result = self
            .app_service
            .create_memory_recall_session(params)
            .await
            .context("failed to create memory recall session")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn get_recall_session(&self, session_id: String) -> Result<Value> {
        let params = MemoryRecallSessionGetParams { session_id };
        let result = self
            .app_service
            .get_memory_recall_session(params)
            .await
            .context("failed to get memory recall session")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn send_recall_turn(&self, session_id: String, query: String) -> Result<Value> {
        let params = MemoryRecallTurnSendParams { session_id, query };
        let result = self
            .app_service
            .send_memory_recall_turn(params)
            .await
            .context("failed to send memory recall turn")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn cancel_recall_turn(&self, turn_id: String) -> Result<Value> {
        let params = MemoryRecallTurnCancelParams { turn_id };
        let result = self
            .app_service
            .cancel_memory_recall_turn(params)
            .await
            .context("failed to cancel memory recall turn")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn get_state(&self, project: Option<String>) -> Result<Value> {
        if let Some(slug) = project {
            let state = self
                .app_service
                .get_project_memory_state(&slug)
                .await
                .context("failed to get project memory state")?;
            Ok(serde_json::to_value(state)?)
        } else {
            let state = self
                .app_service
                .get_recent_memory_state()
                .await
                .context("failed to get recent memory state")?;
            Ok(serde_json::to_value(state)?)
        }
    }

    pub async fn commit_evolution(
        &self,
        payload: asset_core::domain::TopicEvolutionCommitPayload,
    ) -> Result<Value> {
        let result = self
            .app_service
            .commit_memory_topic_evolution(payload)
            .await
            .context("failed to commit memory topic evolution")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn check_watermark(&self, project: &str, session_id: &str) -> Result<Value> {
        let turn = self
            .app_service
            .check_memory_session_watermark(project, session_id)
            .await
            .context("failed to check memory session watermark")?;
        Ok(serde_json::json!({
            "project_slug": project,
            "session_id": session_id,
            "watermark": turn,
            "has_watermark": turn.is_some(),
        }))
    }

    pub async fn advance_watermark(
        &self,
        project: &str,
        session_id: &str,
        turn: u32,
    ) -> Result<Value> {
        self.app_service
            .advance_memory_session_watermark(project, session_id, turn)
            .await
            .context("failed to advance memory session watermark")?;
        Ok(serde_json::json!({
            "project_slug": project,
            "session_id": session_id,
            "advanced_to_turn": turn,
            "success": true,
        }))
    }
}
