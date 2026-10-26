use super::types::MemoryProjectView;
use crate::backend::application::prelude::*;
use sha2::Digest;

impl AppService {
    pub async fn get_memory_project(
        &self,
        params: MemoryProjectGetParams,
    ) -> AppResult<Option<MemoryProjectView>> {
        let project_path = self
            .resolve_context_project_path(Some(&params.project_path))
            .await?
            .unwrap_or(params.project_path);
        self.get_project_memory_l2(&project_path).await
    }

    /// 获取当前项目的长期记忆视图（从 projects/<slug>.md 读取）
    pub async fn get_project_memory_l2(
        &self,
        project_path: &str,
    ) -> AppResult<Option<crate::backend::domain::L2ProjectMemoryView>> {
        let storage = crate::backend::domain::memory::storage::MemoryStorage::new(self.tenant_id());
        let slug = crate::backend::application::memory::project_path_to_slug(project_path);
        let rel_path = format!("projects/{}.md", slug);
        let full_path = storage.root_dir().join(&rel_path);
        if !full_path.exists() {
            return Ok(None);
        }
        let content = storage.read_markdown(&rel_path)?.unwrap_or_default();
        let updated_at = full_path
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());

        let mut hasher = sha2::Sha256::new();
        sha2::Digest::update(&mut hasher, content.as_bytes());
        let revision_hash = format!("{:x}", sha2::Digest::finalize(hasher));

        Ok(Some(crate::backend::domain::L2ProjectMemoryView {
            project_key: slug,
            project_path: Some(project_path.to_string()),
            content: Some(content),
            items: Vec::new(),
            last_successful_consolidation_at: updated_at,
            revision_hash,
        }))
    }
}
