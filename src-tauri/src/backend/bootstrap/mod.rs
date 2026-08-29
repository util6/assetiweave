//! Infrastructure bootstrap for prepared built-in assets.
//!
//! This module is intentionally below `runtime` and `application`: it prepares
//! filesystem-backed built-ins and persists the prepared values without
//! making Runtime depend on an Application workflow module.

use crate::backend::{
    models::{ConversationAdapter, ConversationAdapterRuntimeGateStatus},
    runtime::{AppError, AppResult},
    store,
};
use sqlx::SqlitePool;

pub(crate) async fn materialize_and_seed_builtin_adapters(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<ConversationAdapter>> {
    let adapters = tokio::task::spawn_blocking(
        crate::backend::conversations::ensure_official_conversation_adapters,
    )
    .await
    .map_err(AppError::external)?
    .map_err(AppError::external)?;
    seed_prepared_builtin_adapters(pool, tenant_id, &adapters).await?;
    Ok(adapters)
}

/// Project an application-owned built-in adapter environment into one tenant.
///
/// The prepared adapter list belongs to `AppRuntime`; tenant creation only
/// writes the tenant-owned enablement/source projections and never mutates the
/// shared adapter files.
pub(crate) async fn seed_prepared_builtin_adapters(
    pool: &SqlitePool,
    tenant_id: &str,
    adapters: &[ConversationAdapter],
) -> AppResult<()> {
    store::seed_prepared_builtin_conversation_adapters_sqlx(pool, tenant_id, adapters.to_vec())
        .await
        .map_err(AppError::external)?;
    store::migrate_legacy_conversation_adapter_hashes_sqlx(pool, tenant_id)
        .await
        .map_err(AppError::external)?;
    store::normalize_conversation_paths_sqlx(pool, tenant_id)
        .await
        .map_err(AppError::external)?;
    reconcile_app_conversation_adapters(pool, tenant_id).await?;
    Ok(())
}

pub(crate) async fn reconcile_app_conversation_adapters(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    let settings = crate::backend::app_settings::load_or_import_app_settings_sqlx(pool).await?;
    let packages = store::list_conversation_adapter_packages_sqlx(pool).await?;
    for package in packages {
        let adapter = if package.runtime_ready
            && package.runtime_gate_status == ConversationAdapterRuntimeGateStatus::Ready
        {
            let adapter_manifest_path = package.adapter_manifest_path.clone();
            let settings = settings.clone();
            tokio::task::spawn_blocking(move || {
                crate::backend::path_utils::expand_path(&adapter_manifest_path)
                    .and_then(|manifest_path| {
                        crate::backend::conversations::register_external_adapter_with_settings(
                            crate::backend::conversations::ExternalAdapterRegisterParams {
                                manifest_path: manifest_path.to_string_lossy().to_string(),
                                dry_run: true,
                                yes: true,
                            },
                            &settings,
                        )
                        .map_err(AppError::external)
                    })
                    .and_then(|preview| {
                        crate::backend::conversations::adapter_from_registration_preview(preview)
                    })
            })
            .await
            .map_err(AppError::external)
            .and_then(|result| result)
            .map_err(|error| {
                crate::backend::operation_log::log_warn(
                    "app.environment.conversation_adapter_projection",
                    "conversation adapter package projection was disabled",
                    &[
                        ("tenant_id", tenant_id.to_string()),
                        ("package_id", package.package_id.clone()),
                        ("error", error.to_string()),
                    ],
                );
                error
            })
            .ok()
        } else {
            None
        };
        store::set_app_conversation_adapter_projection_sqlx(
            pool,
            tenant_id,
            &package.adapter_id,
            adapter.as_ref(),
        )
        .await?;
    }
    Ok(())
}
