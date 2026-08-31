use super::prelude::*;
use crate::backend::models::ConversationSession;
use chrono::{DateTime, Utc};
use std::cmp::Ordering;
use std::path::{Component, Path, PathBuf};

const RECENT_WINDOW_HOURS: i64 = 72;

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecentConversationView {
    #[default]
    Project,
    Time,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub(crate) struct RecentConversationSessionListParams {
    #[serde(default)]
    pub(crate) view: RecentConversationView,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct RecentConversationSession {
    pub(crate) session: ConversationSession,
    pub(crate) project_path: Option<String>,
    pub(crate) last_activity_at: String,
    pub(crate) source_agent: String,
    pub(crate) question_count: usize,
    pub(crate) turn_count: usize,
}

impl AppService {
    pub(crate) fn list_recent_conversation_sessions(
        &self,
        params: RecentConversationSessionListParams,
    ) -> AppResult<Vec<RecentConversationSession>> {
        self.list_recent_conversation_sessions_at(params, Utc::now())
    }

    pub(crate) fn list_recent_conversation_sessions_at(
        &self,
        params: RecentConversationSessionListParams,
        now: DateTime<Utc>,
    ) -> AppResult<Vec<RecentConversationSession>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let cutoff = (now - chrono::Duration::hours(RECENT_WINDOW_HOURS)).to_rfc3339();
        let now_text = now.to_rfc3339();
        let (records, registered_roots) = self.runtime.run_sync(async move {
            let records = crate::backend::store::list_recent_conversation_sessions_sqlx(
                &pool, &tenant_id, &cutoff, &now_text,
            )
            .await?;
            // Source.repo_root is the current app-owned registry of project roots.
            let registered_roots = crate::backend::store::load_sources_sqlx(&pool, &tenant_id)
                .await?
                .into_iter()
                .filter_map(|source| source.repo_root)
                .collect::<Vec<_>>();
            Ok::<_, AppError>((records, registered_roots))
        })?;

        let cutoff = now - chrono::Duration::hours(RECENT_WINDOW_HOURS);
        let mut sessions = records
            .into_iter()
            .filter_map(|record| {
                let last_activity_at = DateTime::parse_from_rfc3339(&record.last_activity_at)
                    .ok()?
                    .with_timezone(&Utc);
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

fn resolve_project_directory(raw_path: &str, registered_roots: &[String]) -> Option<String> {
    let cwd = crate::backend::path_utils::expand_path(raw_path).ok()?;
    let cwd = canonicalize_or_normalize(&cwd);
    let filesystem = crate::backend::host_filesystem::HostFilesystem::current();

    let registered_root = registered_roots
        .iter()
        .filter_map(|root| {
            let root = crate::backend::path_utils::expand_path(root).ok()?;
            let root = canonicalize_or_normalize(&root);
            filesystem
                .is_within(&cwd, &root)
                .then_some((root.components().count(), root))
        })
        .max_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.to_string_lossy().cmp(&right.1.to_string_lossy()))
        })
        .map(|(_, root)| root);

    let project_root = registered_root
        .or_else(|| {
            crate::backend::path_utils::find_git_root(&cwd)
                .map(|root| canonicalize_or_normalize(&root))
        })
        .unwrap_or(cwd);
    crate::backend::path_utils::normalize_path_for_storage(&project_root.to_string_lossy()).ok()
}

fn canonicalize_or_normalize(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|_| normalize_path_lexically(path))
}

fn normalize_path_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}
