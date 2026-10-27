use crate::backend::application::{AppResult, AppService};
use crate::backend::infrastructure::memory::storage::MemoryStorage;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMarkdownFileView {
    pub file_path: String,
    pub content: String,
    pub exists: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl AppService {
    pub async fn get_recent_memory_markdown_view(
        &self,
        date: Option<&str>,
    ) -> AppResult<MemoryMarkdownFileView> {
        let storage = MemoryStorage::new(self.tenant_id());
        let rel_path = match date {
            Some(d) if !d.trim().is_empty() => format!("recent/{}.md", d.trim()),
            _ => "recent/latest.md".to_string(),
        };
        let full_path = storage.root_dir().join(&rel_path);
        let exists = full_path.exists();
        let content = if exists {
            storage.read_markdown(&rel_path)?.unwrap_or_default()
        } else {
            String::new()
        };
        let updated_at = if exists {
            full_path
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
        } else {
            None
        };
        Ok(MemoryMarkdownFileView {
            file_path: rel_path,
            content,
            exists,
            updated_at,
        })
    }

    pub async fn get_project_memory_markdown_view(
        &self,
        project_path: &str,
    ) -> AppResult<MemoryMarkdownFileView> {
        let storage = MemoryStorage::new(self.tenant_id());
        let slug = crate::backend::application::memory::project_path_to_slug(project_path);
        let rel_path = format!("projects/{}.md", slug);
        let full_path = storage.root_dir().join(&rel_path);
        let exists = full_path.exists();
        let content = if exists {
            storage.read_markdown(&rel_path)?.unwrap_or_default()
        } else {
            String::new()
        };
        let updated_at = if exists {
            full_path
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
        } else {
            None
        };
        Ok(MemoryMarkdownFileView {
            file_path: rel_path,
            content,
            exists,
            updated_at,
        })
    }
}

#[cfg(test)]
#[path = "memory_markdown_views_tests.rs"]
mod tests;
