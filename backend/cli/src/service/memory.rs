use anyhow::{Context, Result};
use asset_core::application::AppService;
use asset_core::application::{
    MemoryContextResolveParams, MemoryProjectGetParams, MemoryRecallSearchParams,
    MemoryRecallSessionCreateParams, MemoryRecallSessionGetParams, MemoryRecallTurnCancelParams,
    MemoryRecallTurnSendParams, MemoryScopeRebuildParams, MemoryTaskGetParams,
    MemoryTaskListParams, MemoryTaskRetryParams,
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

    pub async fn get_recent_snapshot(&self) -> Result<Value> {
        let snapshot = self
            .app_service
            .get_recent_memory_snapshot()
            .await
            .context("failed to get recent memory snapshot")?;
        Ok(serde_json::to_value(snapshot)?)
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
        let params = MemoryProjectGetParams { project_path };
        let result = self
            .app_service
            .get_memory_project(params)
            .await
            .context("failed to get memory project")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn rebuild(&self, params: MemoryScopeRebuildParams) -> Result<Value> {
        let result = self
            .app_service
            .rebuild_memory_scope(params)
            .await
            .context("failed to rebuild memory")?;
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
}
