use anyhow::{Context, Result};
use asset_core::application::AppService;
use asset_core::domain::{
    MemoryCommitResult, MemoryTreeNode, NodeStatus, TopicEvolutionCommitPayload,
};
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

    pub async fn get_project(&self, project_path: String) -> Result<Value> {
        let view = self
            .app_service
            .get_project_memory_markdown_view(&project_path)
            .await
            .context("failed to get project memory markdown")?;
        Ok(serde_json::to_value(view)?)
    }

    pub async fn get_state(&self, project: Option<String>) -> Result<Value> {
        if let Some(raw_slug) = project {
            let slug = if raw_slug.contains('/') || raw_slug.contains('\\') {
                asset_core::application::memory::project_path_to_slug(&raw_slug)
            } else {
                raw_slug
            };
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
        payload: TopicEvolutionCommitPayload,
    ) -> Result<MemoryCommitResult> {
        let result = self
            .app_service
            .commit_memory_topic_evolution(payload)
            .await
            .context("failed to commit memory topic evolution")?;
        Ok(result)
    }

    pub async fn update_node_status(
        &self,
        node_id: &str,
        status: NodeStatus,
        project_slug: Option<&str>,
        window: Option<&str>,
    ) -> Result<MemoryTreeNode> {
        let node = self
            .app_service
            .update_memory_node_status(node_id, status, project_slug, window)
            .await
            .context("failed to update memory node status")?;
        Ok(node)
    }

    pub async fn refresh_recent(&self, window: Option<&str>) -> Result<Value> {
        let recent = self
            .app_service
            .refresh_recent_memory_state(window)
            .await
            .context("failed to refresh recent memory state")?;
        Ok(serde_json::to_value(recent)?)
    }

    pub async fn resolve_session_id(&self, session_id: &str) -> String {
        let trimmed = session_id.trim();
        if trimmed.len() == 36 && trimmed.contains('-') {
            return trimmed.to_string();
        }
        let params = asset_core::application::ConversationOutlineParams::new(trimmed.to_string());
        if let Ok(outline) = self.app_service.get_conversation_outline(params).await {
            match outline {
                asset_core::domain::ConversationOutline::Session(s) => return s.session_id,
                asset_core::domain::ConversationOutline::Turn(t) => return t.session_id,
                _ => {}
            }
        }
        trimmed.to_string()
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
