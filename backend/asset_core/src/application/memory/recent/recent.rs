use crate::backend::application::prelude::*;
use crate::backend::domain::{ConversationSession, RecentMemoryEvent};
use chrono::{DateTime, Utc};
use std::cmp::Ordering;
use std::path::{Component, Path, PathBuf};

const RECENT_WINDOW_HOURS: i64 = 72;

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecentConversationView {
    #[default]
    Project,
    Time,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct RecentConversationSessionListParams {
    #[serde(default)]
    pub view: RecentConversationView,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RecentConversationSession {
    pub session: ConversationSession,
    pub project_path: Option<String>,
    pub last_activity_at: String,
    pub source_agent: String,
    pub question_count: usize,
    pub turn_count: usize,
    pub recent_events: Vec<RecentMemoryEvent>,
}

impl AppService {
    pub async fn get_recent_memory_event_target(
        &self,
        event_id: String,
    ) -> AppResult<Option<crate::backend::domain::memory::RecentMemoryEventTarget>> {
        if event_id.trim().is_empty() {
            return Err(AppError::Validation(
                "Recent Memory event id is required".to_string(),
            ));
        }
        Ok(None)
    }

    pub async fn list_recent_conversation_sessions(
        &self,
        params: RecentConversationSessionListParams,
    ) -> AppResult<Vec<RecentConversationSession>> {
        self.list_recent_conversation_sessions_at(params, Utc::now())
            .await
    }

    pub async fn list_recent_conversation_sessions_at(
        &self,
        params: RecentConversationSessionListParams,
        now: DateTime<Utc>,
    ) -> AppResult<Vec<RecentConversationSession>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let cutoff = (now - chrono::Duration::hours(RECENT_WINDOW_HOURS)).to_rfc3339();
        let now_text = now.to_rfc3339();
        let internal_agent_workspace =
            crate::backend::infrastructure::agent_execution::agent_execution_workspace_root(
                &self.db_path,
            );
        let mut records = crate::backend::store::list_recent_conversation_sessions_sqlx(
            &pool,
            &tenant_id,
            &cutoff,
            &now_text,
            &internal_agent_workspace,
        )
        .await?;
        for record in &mut records {
            record.recent_events = Vec::new();
        }
        // Source.repo_root is the current app-owned registry of project roots.
        let registered_roots = crate::backend::store::load_sources_sqlx(&pool, &tenant_id)
            .await?
            .into_iter()
            .filter_map(|source| source.repo_root)
            .collect::<Vec<_>>();

        let cutoff = now - chrono::Duration::hours(RECENT_WINDOW_HOURS);
        let mut sessions = records
            .into_iter()
            .filter_map(|record| {
                let last_activity_at =
                    crate::backend::domain::parse_conversation_timestamp(&record.last_activity_at)?;
                if last_activity_at < cutoff || last_activity_at > now {
                    return None;
                }
                let raw_project_path = record.cwd.as_deref().or(record
                    .session
                    .session
                    .project_path
                    .as_deref());
                let project_path = raw_project_path
                    .and_then(|path| resolve_project_directory(path, &registered_roots));
                Some(RecentConversationSession {
                    session: record.session.session,
                    project_path,
                    last_activity_at: last_activity_at.to_rfc3339(),
                    source_agent: record.source_agent,
                    question_count: record.session.question_count,
                    turn_count: record.session.turn_count,
                    recent_events: record.recent_events,
                })
            })
            .collect::<Vec<_>>();

        sessions.sort_by(|left, right| match params.view {
            RecentConversationView::Project => compare_project_view(left, right),
            RecentConversationView::Time => compare_time_view(left, right),
        });

        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(50).clamp(1, 500);
        Ok(sessions.into_iter().skip(offset).take(limit).collect())
    }
}

fn compare_project_view(
    left: &RecentConversationSession,
    right: &RecentConversationSession,
) -> Ordering {
    left.project_path
        .is_none()
        .cmp(&right.project_path.is_none())
        .then_with(|| left.project_path.cmp(&right.project_path))
        .then_with(|| compare_activity_desc(left, right))
        .then_with(|| left.session.id.cmp(&right.session.id))
}

fn compare_time_view(
    left: &RecentConversationSession,
    right: &RecentConversationSession,
) -> Ordering {
    compare_activity_desc(left, right).then_with(|| left.session.id.cmp(&right.session.id))
}

fn compare_activity_desc(
    left: &RecentConversationSession,
    right: &RecentConversationSession,
) -> Ordering {
    right
        .last_activity_at
        .cmp(&left.last_activity_at)
        .then_with(|| left.session.id.cmp(&right.session.id))
}
pub use crate::backend::infrastructure::project_path::resolve_project_directory;
