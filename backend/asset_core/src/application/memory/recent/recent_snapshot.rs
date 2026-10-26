use crate::backend::application::prelude::*;
use crate::backend::domain::memory::{
    RecentMemoryItemView, RecentMemorySnapshotView, RecentMemoryStateView, RecentMemoryStatus,
    RecentProjectView, RecentSessionReferenceView, RecentSnapshotPublicationKind,
    SourceAvailability,
};

fn extract_short_id_and_turn(text: &str) -> (Option<String>, Option<i64>) {
    if let Some(idx) = text.find("aiwc://session/") {
        let rest = &text[idx + "aiwc://session/".len()..];
        let end_idx = rest.find(')').unwrap_or(rest.len());
        let uri = &rest[..end_idx];
        let parts: Vec<&str> = uri.split('/').collect();
        let short_id = parts.first().map(|s| s.to_string());
        let turn_idx = if parts.len() >= 3 && parts[1] == "turn" {
            parts[2].parse::<i64>().ok()
        } else {
            None
        };
        (short_id, turn_idx)
    } else {
        (None, None)
    }
}

pub fn parse_recent_markdown_to_state_view(content: &str) -> RecentMemoryStateView {
    let now = chrono::Utc::now().to_rfc3339();
    let mut projects = Vec::new();

    let parts: Vec<&str> = content.split("## 项目: ").collect();
    for (idx, part) in parts.iter().enumerate() {
        if idx == 0 {
            continue;
        }
        let mut lines = part.lines();
        let proj_path = lines.next().unwrap_or("").trim().to_string();
        let proj_body = lines.collect::<Vec<_>>().join("\n");
        let slug = crate::backend::application::memory::project_path_to_slug(&proj_path);

        let mut items = Vec::new();
        let mut session_ids = std::collections::HashSet::new();

        for line in proj_body.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("- [") {
                let (short_id, _turn_idx) = extract_short_id_and_turn(trimmed);
                if let Some(ref s) = short_id {
                    session_ids.insert(s.clone());
                }

                let item_id = format!("{}-{}", slug, items.len() + 1);
                items.push(RecentMemoryItemView {
                    item_id,
                    revision_id: "rev-1".to_string(),
                    category: "recent_task".to_string(),
                    status: "active".to_string(),
                    title: trimmed.trim_start_matches("- ").to_string(),
                    summary: trimmed.to_string(),
                    rationale: String::new(),
                    occurred_at: now.clone(),
                    recommendation_rank: Some(items.len() as i64 + 1),
                    source_availability: SourceAvailability::Available,
                    session_references: short_id
                        .map(|s| {
                            vec![RecentSessionReferenceView {
                                source_id: "source-1".to_string(),
                                session_id: s.clone(),
                                session_title: format!("会话 {}", s),
                                source_agent: "agent".to_string(),
                                last_activity_at: now.clone(),
                                available: true,
                                unavailable_reason: None,
                            }]
                        })
                        .unwrap_or_default(),
                });
            }
        }

        projects.push(RecentProjectView {
            project_key: slug,
            project_title: proj_path.clone(),
            project_path: Some(proj_path),
            summary: proj_body,
            no_material_change: false,
            latest_activity_at: now.clone(),
            source_session_count: session_ids.len() as i64,
            items,
        });
    }

    RecentMemoryStateView {
        status: RecentMemoryStatus::Ready,
        snapshot: Some(RecentMemorySnapshotView {
            snapshot_id: "markdown-latest".to_string(),
            sequence: 1,
            target_watermark: "markdown".to_string(),
            window_start: now.clone(),
            window_end: now.clone(),
            window_hours: 24,
            publication_kind: RecentSnapshotPublicationKind::Generated,
            reused_from_snapshot_id: None,
            content_generated_at: now.clone(),
            published_at: now,
            projects,
        }),
        latest_attempt_task_id: None,
        latest_attempt_error: None,
    }
}

impl AppService {
    pub async fn get_recent_memory_snapshot(&self) -> AppResult<RecentMemoryStateView> {
        let tenant_id = self.tenant_id().to_string();

        let storage = crate::backend::domain::memory::storage::MemoryStorage::new(&tenant_id);
        if let Ok(Some(content)) = storage.read_markdown("recent/latest.md") {
            if !content.trim().is_empty() {
                return Ok(parse_recent_markdown_to_state_view(&content));
            }
        }

        Ok(RecentMemoryStateView {
            status: RecentMemoryStatus::Empty,
            snapshot: None,
            latest_attempt_task_id: None,
            latest_attempt_error: None,
        })
    }
}
