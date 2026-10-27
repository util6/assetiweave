use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::store;
use sha2::{Digest, Sha256};

impl AppService {
    pub async fn resolve_memory_context(
        &self,
        params: MemoryContextResolveParams,
    ) -> AppResult<MemoryContextResult> {
        let token_budget = params.token_budget.unwrap_or(2_000).clamp(64, 32_000);
        let project_path = self
            .resolve_context_project_path(params.project_path.as_deref())
            .await?
            .or(params.project_path);
        let tenant_id = self.tenant_id().to_string();
        let storage =
            crate::backend::infrastructure::memory::storage::MemoryStorage::new(&tenant_id);

        let mut sections = Vec::new();
        let mut references = Vec::new();

        // 1. Global Memory (MEMORY.md)
        if let Ok(Some(global_content)) = storage.read_markdown("MEMORY.md") {
            if !global_content.trim().is_empty() {
                sections.push(format!(
                    "## 全局长期记忆 (Global Memory)\n{}",
                    global_content.trim()
                ));
                references.push(MemoryContextReference {
                    kind: "global_memory".to_string(),
                    id: "MEMORY.md".to_string(),
                    source_revision: None,
                    session_id: None,
                });
            }
        }

        // 2. Project Memory (projects/<slug>.md)
        let proj_slug = project_path
            .as_deref()
            .map(crate::backend::application::memory::project_path_to_slug);
        if let (Some(proj), Some(slug)) = (project_path.as_deref(), proj_slug.as_ref()) {
            let rel_path = format!("projects/{}.md", slug);
            if let Ok(Some(proj_content)) = storage.read_markdown(&rel_path) {
                if !proj_content.trim().is_empty() {
                    sections.push(format!(
                        "## 项目记忆: {} (Project Memory)\n{}",
                        proj,
                        proj_content.trim()
                    ));
                    references.push(MemoryContextReference {
                        kind: "project_memory".to_string(),
                        id: rel_path,
                        source_revision: None,
                        session_id: None,
                    });
                }
            }
        }

        // 3. Recent Work (recent/latest.md)
        if let Ok(Some(recent_content)) = storage.read_markdown("recent/latest.md") {
            if !recent_content.trim().is_empty() {
                sections.push(format!(
                    "## 近期研发进展 (Recent Work)\n{}",
                    recent_content.trim()
                ));
                references.push(MemoryContextReference {
                    kind: "recent_memory".to_string(),
                    id: "recent/latest.md".to_string(),
                    source_revision: None,
                    session_id: None,
                });
            }
        }

        let full_text = sections.join("\n\n");
        let char_budget = token_budget * 4;
        let text = if full_text.len() > char_budget {
            format!(
                "{}\n\n...[truncated due to token budget]",
                &full_text[..char_budget]
            )
        } else {
            full_text
        };
        let estimated_tokens = (text.len() + 3) / 4;
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        let revision = format!("{:x}", hasher.finalize());

        let compiled = MemoryContextResult {
            text,
            revision,
            generated_at: Some(chrono::Utc::now().to_rfc3339()),
            estimated_tokens,
            token_budget,
            references,
            global_version: None,
            project_version: None,
            project_sources: Vec::new(),
        };

        if self.backend_settings()?.is_memory_usage_enabled() {
            let used_at = Utc::now().to_rfc3339();
            for reference in &compiled.references {
                store::record_memory_usage_event_sqlx(
                    self.db.pool(),
                    &tenant_id,
                    &reference.kind,
                    &reference.id,
                    "context",
                    &compiled.revision,
                    &used_at,
                )
                .await?;
            }
        }
        Ok(compiled)
    }

    pub async fn resolve_context_project_path(
        &self,
        raw_path: Option<&str>,
    ) -> AppResult<Option<String>> {
        let Some(raw_path) = raw_path.filter(|path| !path.trim().is_empty()) else {
            return Ok(None);
        };
        let roots = crate::backend::store::load_sources_sqlx(self.db.pool(), self.tenant_id())
            .await?
            .into_iter()
            .filter_map(|source| source.repo_root)
            .collect::<Vec<_>>();
        crate::backend::application::memory::recent::recent::resolve_project_directory(
            raw_path, &roots,
        )
        .map(Some)
        .ok_or_else(|| AppError::Validation("project_path cannot be normalized".to_string()))
    }
}

pub fn fit_context_section(content: &str, budget: usize, used: usize) -> Option<String> {
    let remaining = budget.saturating_sub(used);
    if remaining == 0 {
        return None;
    }
    if estimate_context_tokens(content) <= remaining {
        return Some(content.to_string());
    }
    let max_chars = remaining.saturating_mul(4);
    let prefix = content.lines().next().unwrap_or(content);
    if max_chars <= prefix.len() + 1 {
        return Some(prefix.chars().take(max_chars).collect());
    }
    let body_limit = max_chars - prefix.len() - 1;
    let body = content
        .strip_prefix(prefix)
        .unwrap_or_default()
        .chars()
        .take(body_limit)
        .collect::<String>();
    Some(format!("{prefix}\n{}", body.trim_end()))
}

pub fn bullet_lines(values: &[String]) -> String {
    if values.is_empty() {
        return "- none".to_string();
    }
    values
        .iter()
        .map(|value| format!("- {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn session_short_id(session_id: &str) -> String {
    let trimmed = session_id.trim();
    let stripped = ["conversation-session-", "web-record-session-", "session-"]
        .iter()
        .find_map(|prefix| trimmed.strip_prefix(prefix))
        .unwrap_or(trimmed);

    if stripped.len() >= 8 {
        stripped[..8].to_string()
    } else {
        stripped.to_string()
    }
}

pub fn estimate_context_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

pub fn context_revision(
    tenant_id: &str,
    project_path: Option<&str>,
    query: &str,
    token_budget: usize,
    references: &[MemoryContextReference],
) -> String {
    let mut hasher = Sha256::new();
    for value in [
        tenant_id,
        project_path.unwrap_or_default(),
        query,
        &token_budget.to_string(),
    ] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    for reference in references {
        hasher.update(reference.kind.as_bytes());
        hasher.update([0]);
        hasher.update(reference.id.as_bytes());
        hasher.update([0]);
        if let Some(source_revision) = reference.source_revision {
            hasher.update(source_revision.to_string().as_bytes());
        }
        hasher.update([0]);
    }
    format!("context-{:x}", hasher.finalize())
}
