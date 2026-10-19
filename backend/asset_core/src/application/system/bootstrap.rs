//! Application projection of prepared, application-owned built-in adapters.

use crate::backend::{
    application::{AppError, AppResult, AppService},
    domain::{ConversationAdapter, ConversationAdapterKind},
    infrastructure::runtime::{AppRuntime, RuntimeRole},
    infrastructure::{path_utils::ensure_app_library_dirs, target_catalog::TargetCatalog},
    store,
};
use sqlx::SqlitePool;
use std::path::PathBuf;

/// Initialize the infrastructure runtime under Application-owned startup
/// sequencing. Data migration runs before a resident runtime begins accepting
/// background work, and the published adapter catalog is refreshed afterward.
impl AppService {
    pub async fn bootstrap_runtime(
        db_path: PathBuf,
        role: RuntimeRole,
    ) -> AppResult<std::sync::Arc<AppRuntime>> {
        if let Some(parent) = db_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(AppError::external)?;
        }
        crate::backend::infrastructure::runtime::storage_migration::migrate_legacy_storage_if_needed(&db_path)?;
        let pool = store::open_migrated_pool(&db_path).await?;
        let orphaned_tasks = store::reconcile_orphaned_tasks_sqlx(&pool).await?;
        if !orphaned_tasks.is_empty() {
            tracing::info!(
                orphaned_count = orphaned_tasks.len(),
                orphaned_tasks = ?orphaned_tasks,
                "Startup task reconciliation recovered orphaned background tasks"
            );
        }
        ensure_app_library_dirs()?;
        let target_catalog_dir = db_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("target-providers");
        let target_catalog = TargetCatalog::load_with_overrides(&target_catalog_dir)?;
        crate::backend::application::system::defaults::seed_defaults_with_catalog(
            &pool,
            &target_catalog,
        )
        .await?;
        let builtin_conversation_adapters = tokio::task::spawn_blocking(
            crate::backend::infrastructure::conversations::ensure_official_conversation_adapters,
        )
        .await
        .map_err(AppError::external)?
        .map_err(AppError::external)?;
        let tenant_ids = store::load_all_tenant_ids_sqlx(&pool).await?;
        for tenant_id in &tenant_ids {
            seed_prepared_builtin_adapters(&pool, tenant_id, &builtin_conversation_adapters)
                .await?;
        }

        let runtime = AppRuntime::bootstrap(
            db_path.clone(),
            pool,
            target_catalog_dir,
            target_catalog,
            builtin_conversation_adapters,
        )
        .await?;
        crate::backend::application::agents::agent_market::prepare_startup_runtime(
            &runtime,
            &db_path,
            role == RuntimeRole::ResidentHost,
        )
        .await?;
        runtime.refresh_app_settings().await;
        let tenant_id = runtime.context().tenant.id.clone();
        migrate_legacy_adapter_hashes(runtime.db().pool(), &tenant_id).await?;
        runtime.refresh_conversation_adapter_catalog().await?;
        if role == RuntimeRole::ResidentHost {
            runtime.start_resident_services().await;
        }
        Ok(runtime)
    }
}

/// Seed one tenant from the immutable built-in environment prepared by
/// `AppRuntime`. This boundary performs no filesystem writes.
pub async fn seed_prepared_builtin_adapters(
    pool: &SqlitePool,
    tenant_id: &str,
    adapters: &[ConversationAdapter],
) -> AppResult<()> {
    crate::backend::application::system::conversation_adapters::seed_prepared_builtin_adapters(
        pool, tenant_id, adapters,
    )
    .await
}

pub async fn migrate_legacy_adapter_hashes(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    for mut adapter in
        crate::backend::application::conversations::conversation_storage::list_adapters(
            pool, tenant_id,
        )
        .await?
    {
        if adapter.kind != ConversationAdapterKind::External {
            continue;
        }
        let Some(manifest_path) = adapter.manifest_path.clone() else {
            continue;
        };
        let Ok(validation) =
            crate::backend::infrastructure::conversations::validate_external_adapter(
                crate::backend::infrastructure::conversations::ExternalAdapterValidateParams {
                    manifest_path,
                },
            )
        else {
            continue;
        };
        let Some(trusted_hash) = adapter.trusted_hash.as_deref() else {
            continue;
        };
        let content_hash = validation.content_hash.as_str();
        if trusted_hash == content_hash {
            if adapter.content_hash.as_deref() != Some(content_hash) {
                adapter.content_hash = Some(validation.content_hash);
                crate::backend::application::conversations::conversation_storage::save_adapter(
                    pool, tenant_id, &adapter,
                )
                .await?;
            }
            continue;
        }
        if Some(trusted_hash) == validation.executable_hash.as_deref()
            || trusted_hash == validation.manifest_hash
        {
            adapter.content_hash = Some(validation.content_hash.clone());
            adapter.trusted_hash = Some(validation.content_hash);
            crate::backend::application::conversations::conversation_storage::save_adapter(
                pool, tenant_id, &adapter,
            )
            .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "bootstrap_tests.rs"]
mod tests;
