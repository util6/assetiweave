use std::collections::HashMap;

use sqlx::SqlitePool;

use crate::backend::application::prelude::*;
use crate::backend::domain::memory::{
    RecentMemoryErrorView, RecentMemoryItemView, RecentMemorySnapshotView, RecentMemoryStateView,
    RecentMemoryStatus, RecentProjectView, RecentSessionReferenceView, SourceAvailability,
};
use crate::backend::store::memory::{
    load_recent_memory_state_fact_sqlx, load_recent_snapshot_fact_by_id_sqlx,
    RecentMemoryStateFact, RecentSnapshotFact,
};

pub fn assemble_recent_snapshot_view(fact: RecentSnapshotFact) -> RecentMemorySnapshotView {
    let mut refs_by_revision: HashMap<String, Vec<RecentSessionReferenceView>> = HashMap::new();
    for r in fact.references {
        refs_by_revision
            .entry(r.item_revision_id)
            .or_default()
            .push(RecentSessionReferenceView {
                source_id: r.source_id,
                session_id: r.session_id,
                session_title: r.session_title,
                source_agent: r.source_agent,
                last_activity_at: r.last_activity_at,
                available: r.available,
                unavailable_reason: r.unavailable_reason,
            });
    }

    let mut items_by_project: HashMap<String, Vec<RecentMemoryItemView>> = HashMap::new();
    for item in fact.items {
        let refs = refs_by_revision
            .remove(&item.item_revision_id)
            .unwrap_or_default();
        let source_availability = if refs.is_empty() {
            SourceAvailability::Available
        } else {
            let available_count = refs.iter().filter(|r| r.available).count();
            if available_count == refs.len() {
                SourceAvailability::Available
            } else if available_count == 0 {
                SourceAvailability::Unavailable
            } else {
                SourceAvailability::PartiallyUnavailable
            }
        };

        items_by_project
            .entry(item.project_key)
            .or_default()
            .push(RecentMemoryItemView {
                item_id: item.item_id,
                revision_id: item.item_revision_id,
                category: item.category,
                status: item.status,
                title: item.title,
                summary: item.summary,
                rationale: item.rationale.unwrap_or_default(),
                occurred_at: item.occurred_at.unwrap_or_default(),
                recommendation_rank: item.recommendation_rank,
                source_availability,
                session_references: refs,
            });
    }

    let mut projects = Vec::new();
    for p in fact.projects {
        let items = items_by_project.remove(&p.project_key).unwrap_or_default();
        projects.push(RecentProjectView {
            project_key: p.project_key,
            project_title: p.project_title,
            project_path: p.project_path,
            summary: p.summary.unwrap_or_default(),
            no_material_change: p.no_material_change,
            latest_activity_at: p.latest_activity_at.unwrap_or_default(),
            source_session_count: p.source_session_count,
            items,
        });
    }

    RecentMemorySnapshotView {
        snapshot_id: fact.id,
        sequence: fact.sequence,
        target_watermark: fact.target_watermark_utc,
        window_start: fact.window_start_utc,
        window_end: fact.window_end_utc,
        window_hours: fact.window_hours,
        publication_kind: fact.publication_kind,
        reused_from_snapshot_id: fact.reused_from_snapshot_id,
        content_generated_at: fact.content_generated_at,
        published_at: fact.published_at,
        projects,
    }
}

pub(crate) fn infer_recent_memory_error_retryable(code: &str) -> bool {
    // 只有明确已知的系统瞬态或可恢复业务错误才推断为可重试。
    // 严禁使用“未列出即 true”的黑名单推断未知错误，未知错误一律视为不可重试。
    matches!(
        code,
        "MEMORY_COVERAGE_INCOMPLETE"
            | "TASK_ALREADY_EXISTS"
            | "timeout"
            | "storage_error"
            | "process_error"
            | "external_error"
            | "conflict"
            | "agent_unavailable"
            | "spawn_failed"
            | "process_output_failed"
            | "protocol_failed"
            | "agent_exited"
            | "workspace_failed"
            | "cleanup_failed"
    )
}

pub fn assemble_recent_memory_state_view(
    state_fact: RecentMemoryStateFact,
    snapshot: Option<RecentMemorySnapshotView>,
) -> RecentMemoryStateView {
    let status = if state_fact.has_active_job {
        RecentMemoryStatus::Generating
    } else if state_fact.latest_attempt_error_code.is_some() {
        RecentMemoryStatus::UpdateFailed
    } else if snapshot.is_some() {
        RecentMemoryStatus::Ready
    } else {
        RecentMemoryStatus::Empty
    };

    let latest_attempt_error = match (
        state_fact.latest_attempt_error_code,
        state_fact.latest_attempt_error_message,
    ) {
        (Some(code), Some(message)) => {
            let retryable = state_fact
                .latest_attempt_error_retryable
                .unwrap_or_else(|| infer_recent_memory_error_retryable(&code));
            Some(RecentMemoryErrorView {
                code,
                message,
                retryable,
            })
        }
        _ => None,
    };

    RecentMemoryStateView {
        status,
        snapshot,
        latest_attempt_task_id: state_fact.latest_attempt_task_id,
        latest_attempt_error,
    }
}

pub(crate) async fn load_recent_snapshot_view_by_id(
    pool: &SqlitePool,
    tenant_id: &str,
    snapshot_id: &str,
) -> AppResult<Option<RecentMemorySnapshotView>> {
    let fact_opt = load_recent_snapshot_fact_by_id_sqlx(pool, tenant_id, snapshot_id).await?;
    Ok(fact_opt.map(assemble_recent_snapshot_view))
}

pub(crate) async fn load_recent_memory_state_view(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<RecentMemoryStateView> {
    let state_fact = load_recent_memory_state_fact_sqlx(pool, tenant_id).await?;
    let snapshot_view = if let Some(ref snapshot_id) = state_fact.last_successful_snapshot_id {
        load_recent_snapshot_view_by_id(pool, tenant_id, snapshot_id).await?
    } else {
        None
    };
    Ok(assemble_recent_memory_state_view(state_fact, snapshot_view))
}
