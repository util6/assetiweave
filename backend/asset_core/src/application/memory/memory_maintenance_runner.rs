use crate::backend::application::{AppError, AppResult, AppService};
use crate::backend::domain::{
    compute_global_consolidation_fingerprint, MemoryJobPurpose, MemoryMaintenanceWorkOrderPayload,
};
use crate::backend::infrastructure::runtime::AppRuntime;
use crate::backend::infrastructure::tasks::TaskContext;
use crate::backend::store;
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;

pub async fn run_memory_maintenance_task(
    runtime: &Arc<AppRuntime>,
    tenant_id: &str,
    job_id: &str,
    owner: &str,
    _purpose: &str,
    context: &TaskContext,
) -> AppResult<serde_json::Value> {
    let service = AppService::from_runtime(runtime)
        .for_tenant(tenant_id)
        .await?;
    let job = store::load_memory_maintenance_job_sqlx(service.db.pool(), tenant_id, job_id)
        .await?
        .ok_or_else(|| AppError::NotFound("memory maintenance job not found".to_string()))?;
    let payload: MemoryMaintenanceWorkOrderPayload = serde_json::from_str(&job.work_order_json)
        .map_err(|_| {
            AppError::Validation(
                "MEMORY_WORK_ORDER_INVALID: invalid maintenance work order".to_string(),
            )
        })?;
    if payload.work_order.tenant_id != tenant_id
        || payload.work_order.purpose != MemoryJobPurpose::GlobalConsolidation
    {
        return Err(AppError::Validation(
            "MEMORY_WORK_ORDER_INVALID: maintenance scope mismatch".to_string(),
        ));
    }
    if payload.work_order.input_fingerprint != job.input_fingerprint
        || payload.work_order.scope.project_key.as_deref() != job.project_key.as_deref()
        || payload.project_path.as_deref() != job.project_path.as_deref()
    {
        return Err(AppError::Validation(
            "MEMORY_WORK_ORDER_INVALID: durable row binding mismatch".to_string(),
        ));
    }
    match payload.work_order.purpose {
        MemoryJobPurpose::ProjectConsolidation => {
            return Err(AppError::Validation(
                "MEMORY_WORK_ORDER_INVALID: project consolidation is decommissioned".to_string(),
            ));
        }
        MemoryJobPurpose::GlobalConsolidation => {
            let global_input = payload.global_input.as_ref().ok_or_else(|| {
                AppError::Validation(
                    "MEMORY_WORK_ORDER_INVALID: missing frozen global input".to_string(),
                )
            })?;
            if payload.project_input.is_some()
                || global_input.tenant_id != tenant_id
                || compute_global_consolidation_fingerprint(global_input)
                    != payload.work_order.source_revision_set_hash
            {
                return Err(AppError::Validation(
                    "MEMORY_WORK_ORDER_INVALID: frozen global input binding mismatch".to_string(),
                ));
            }
        }
        MemoryJobPurpose::RecentSnapshot => {
            return Err(AppError::Validation(
                "MEMORY_WORK_ORDER_INVALID: recent snapshot in maintenance queue".to_string(),
            ));
        }
    }
    let current_source_revision_set_hash = service
        .load_memory_source_revision_set_hash(
            payload.work_order.purpose,
            payload.work_order.scope.project_key.as_deref(),
            payload.project_path.as_deref(),
        )
        .await?;
    if current_source_revision_set_hash != payload.work_order.source_revision_set_hash {
        return Err(AppError::Domain {
            code: "MEMORY_WORK_ORDER_STALE".to_string(),
            message: "memory maintenance evidence changed after enqueue".to_string(),
            retryable: false,
            details: None,
        });
    }
    let current_skill = service.get_active_generation_skill_binding().await?;
    if current_skill != payload.work_order.skill {
        return Err(AppError::Domain {
            code: "MEMORY_WORK_ORDER_STALE".to_string(),
            message: "memory generation skill changed after enqueue".to_string(),
            retryable: false,
            details: None,
        });
    }
    if context.is_cancelled() {
        return Err(AppError::Cancelled(
            "memory maintenance task cancelled".to_string(),
        ));
    }

    let result = service
        .reconcile_global_consolidation_with_agent(
            Utc::now(),
            payload.work_order.clone(),
            payload.skill_text.clone(),
            context.cancellation(),
            Some((job_id, owner)),
            payload.global_input.clone(),
        )
        .await
        .map(|view| json!({ "updated": view.is_some() }));

    match result {
        Ok(summary) => {
            let job_after =
                store::load_memory_maintenance_job_sqlx(service.db.pool(), tenant_id, job_id)
                    .await?;
            let durably_succeeded =
                job_after.as_ref().map(|job| job.status.as_str()) == Some("succeeded");
            if context.is_cancelled() {
                if durably_succeeded {
                    // Cancellation arrived after the atomic
                    // publication; the durable job result wins.
                } else {
                    let cancelled_at = Utc::now().to_rfc3339();
                    let _ = store::finish_memory_maintenance_job_sqlx(
                        service.db.pool(),
                        tenant_id,
                        job_id,
                        owner,
                        "canceled",
                        Some("CANCELED"),
                        Some("memory maintenance task cancelled"),
                        false,
                        &cancelled_at,
                    )
                    .await?;
                    return Err(AppError::Cancelled(
                        "memory maintenance task cancelled".to_string(),
                    ));
                }
            }
            if !durably_succeeded {
                return Err(AppError::Conflict(
                    "memory maintenance result was not durably committed".to_string(),
                ));
            }
            if let Err(error) = service.rebuild_markdown_projections().await {
                tracing::warn!(
                    action = "memory.projection",
                    tenant_id = %tenant_id,
                    job_id = %job_id,
                    error = %error,
                    "Memory Markdown projection was not published"
                );
            }
            Ok(json!({
                "domain": "global_memory",
                "maintenance_job_id": job_id,
                "summary": summary,
            }))
        }
        Err(error) => {
            let failed_at = Utc::now().to_rfc3339();
            let durable_status = if matches!(&error, AppError::Cancelled(_)) {
                "canceled"
            } else if error.code() == "MEMORY_WORK_ORDER_STALE" {
                "stale"
            } else {
                "failed"
            };
            let committed = store::finish_memory_maintenance_job_sqlx(
                service.db.pool(),
                tenant_id,
                job_id,
                owner,
                durable_status,
                Some(&error.code()),
                Some(&error.to_string()),
                error.retryable() && !matches!(&error, AppError::Cancelled(_)),
                &failed_at,
            )
            .await?;
            if !committed {
                return Err(AppError::Conflict(
                    "memory maintenance lease is no longer owned".to_string(),
                ));
            }
            Err(error)
        }
    }
}
