use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::store;
use sha2::{Digest, Sha256};

impl AppService {
    pub(crate) async fn resolve_memory_context(
        &self,
        params: MemoryContextResolveParams,
    ) -> AppResult<MemoryContextResult> {
        let token_budget = params.token_budget.unwrap_or(2_000).clamp(64, 32_000);
        let project_path = self
            .resolve_context_project_path(params.project_path.as_deref())
            .await?;
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let query = params.query.unwrap_or_default();
        let global_version =
            store::load_global_memory_latest_version_sqlx(&pool, &tenant_id).await?;
        let project = if let Some(path) = project_path.as_deref() {
            store::load_project_memory_sqlx(&pool, &tenant_id, path).await?
        } else {
            None
        };
        let project_version = match project.as_ref() {
            Some(project) => {
                store::load_project_memory_latest_version_sqlx(&pool, &tenant_id, &project.id)
                    .await?
            }
            None => None,
        };
        let project_sources = match project_version.as_ref() {
            Some(version) => {
                store::load_project_memory_sources_sqlx(&pool, &tenant_id, &version.id).await?
            }
            None => Vec::new(),
        };
        let sessions = match project_path.as_deref() {
            Some(path) => {
                store::list_session_memories_for_project_sqlx(&pool, &tenant_id, path).await?
            }
            None => Vec::new(),
        };
        let l3_items =
            crate::backend::application::memory::global_consolidation_pipeline::get_global_memory_l3_view(
                &pool, &tenant_id,
            )
            .await?
            .map(|v| v.items)
            .unwrap_or_default();
        let l2_items = if let Some(path) = project_path.as_deref() {
            crate::backend::application::memory::project_consolidation_pipeline::load_l2_items(
                &pool, &tenant_id, path,
            )
            .await
            .unwrap_or_default()
        } else {
            Vec::new()
        };
        let compiled = compile_memory_context(
            &tenant_id,
            project_path.as_deref(),
            &query,
            token_budget,
            global_version.as_ref(),
            project_version.as_ref(),
            &project_sources,
            &sessions,
            &l3_items,
            &l2_items,
        );
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

    pub(crate) async fn resolve_context_project_path(
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

pub(crate) fn compile_memory_context(
    tenant_id: &str,
    project_path: Option<&str>,
    query: &str,
    token_budget: usize,
    global_version: Option<&crate::backend::domain::GlobalMemoryVersion>,
    project_version: Option<&crate::backend::domain::ProjectMemoryVersion>,
    project_sources: &[crate::backend::domain::ProjectMemorySource],
    sessions: &[crate::backend::domain::SessionMemory],
    l3_items: &[crate::backend::domain::L3MemoryItemView],
    l2_items: &[crate::backend::domain::L2MemoryItemView],
) -> MemoryContextResult {
    let mut sections = Vec::new();
    if !l3_items.is_empty() {
        for item in l3_items {
            sections.push(ContextSection::new(
                "global_memory_l3",
                item.revision_id.clone(),
                Some(item.revision_number),
                format!(
                    "## [Global] {}\n{}\n{}",
                    item.title, item.summary, item.rationale
                ),
            ));
        }
    } else if let Some(version) = global_version {
        let summary = version.summary_markdown.as_deref().unwrap_or_default();
        let memory = version.memory_markdown.as_deref().unwrap_or_default();
        if !summary.is_empty() || !memory.is_empty() {
            sections.push(ContextSection::new(
                "global_memory",
                version.id.clone(),
                Some(version.source_watermark),
                format!(
                    "## Global Memory\n{}\n\n## Project Index\n{}",
                    summary, memory
                ),
            ));
        }
    }
    if !l2_items.is_empty() {
        for item in l2_items {
            sections.push(ContextSection::new(
                "project_memory_l2",
                item.revision_id.clone(),
                Some(item.revision_number),
                format!(
                    "### [{:?}] {}\n{}\n{}",
                    item.category, item.title, item.summary, item.rationale
                ),
            ));
        }
    } else if let Some(version) = project_version {
        if let Some(content) = version
            .content_markdown
            .as_deref()
            .filter(|text| !text.is_empty())
        {
            sections.push(ContextSection::new(
                "project_memory",
                version.id.clone(),
                Some(version.source_watermark),
                format!("## Project Memory\n{content}"),
            ));
        }
    }
    let mut selected_sessions = sessions
        .iter()
        .filter(|session| session.status == crate::backend::domain::SessionMemoryStatus::Active)
        .collect::<Vec<_>>();
    selected_sessions.sort_by(|left, right| {
        right
            .source_revision
            .cmp(&left.source_revision)
            .then_with(|| left.id.cmp(&right.id))
    });
    let terms = query
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|term| term.len() > 1)
        .collect::<Vec<_>>();
    if !terms.is_empty() {
        selected_sessions.sort_by_key(|session| {
            let haystack = format!(
                "{} {} {} {}",
                session.summary,
                session.goal,
                session.result,
                session.topics.join(" ")
            )
            .to_lowercase();
            std::cmp::Reverse(terms.iter().filter(|term| haystack.contains(*term)).count())
        });
    }
    for session in selected_sessions.into_iter().take(3) {
        let short_id = session_short_id(&session.session_id);
        sections.push(
            ContextSection::new(
                "session_memory",
                session.id.clone(),
                Some(session.source_revision),
                format!(
                    "## Session Memory [Session: {}]\n### Summary\n{}\n\n### Goal\n{}\n\n### Result\n{}\n\n### Decisions\n{}\n\n### Verification\n{}\n\n### Follow-up\n{}",
                    short_id,
                    session.summary,
                    session.goal,
                    session.result,
                    bullet_lines(&session.decisions),
                    bullet_lines(&session.verification),
                    bullet_lines(&session.follow_up),
                ),
            )
            .with_session_id(session.session_id.clone()),
        );
    }

    let mut used_tokens = 0usize;
    let mut text = String::new();
    let mut references = Vec::new();
    for section in sections {
        let Some(content) = fit_context_section(&section.content, token_budget, used_tokens) else {
            continue;
        };
        let section_tokens = estimate_context_tokens(&content);
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(&content);
        used_tokens = used_tokens.saturating_add(section_tokens);
        references.push(MemoryContextReference {
            kind: section.kind,
            id: section.id,
            source_revision: section.source_revision,
            session_id: section.session_id,
        });
        if used_tokens >= token_budget {
            break;
        }
    }
    let revision = context_revision(tenant_id, project_path, query, token_budget, &references);
    let estimated_tokens = estimate_context_tokens(&text);
    MemoryContextResult {
        text,
        revision,
        generated_at: l3_items
            .iter()
            .map(|i| i.updated_at.clone())
            .max()
            .or_else(|| l2_items.iter().map(|i| i.updated_at.clone()).max())
            .or_else(|| global_version.map(|version| version.updated_at.clone()))
            .or_else(|| project_version.map(|version| version.updated_at.clone())),
        estimated_tokens,
        token_budget,
        references,
        global_version: global_version.cloned(),
        project_version: project_version.cloned(),
        project_sources: project_sources.to_vec(),
    }
}

pub(crate) struct ContextSection {
    kind: String,
    id: String,
    source_revision: Option<i64>,
    session_id: Option<String>,
    content: String,
}

impl ContextSection {
    fn new(
        kind: impl Into<String>,
        id: impl Into<String>,
        source_revision: Option<i64>,
        content: String,
    ) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
            source_revision,
            session_id: None,
            content,
        }
    }

    fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}

pub(crate) fn fit_context_section(content: &str, budget: usize, used: usize) -> Option<String> {
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

pub(crate) fn bullet_lines(values: &[String]) -> String {
    if values.is_empty() {
        return "- none".to_string();
    }
    values
        .iter()
        .map(|value| format!("- {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn session_short_id(session_id: &str) -> String {
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

pub(crate) fn estimate_context_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

pub(crate) fn context_revision(
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
