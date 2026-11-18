use crate::backend::{
    application::prelude::{AppError, AppResult},
    domain::{ConversationAdapter, ConversationAdapterRuntimeGateStatus},
    store,
};
use sqlx::SqlitePool;

/// Project a prepared built-in adapter environment into one tenant.
///
/// Application owns the orchestration; infrastructure normalizes file paths
/// and materializes adapter metadata while Store persists the prepared rows.
pub async fn seed_prepared_builtin_adapters(
    pool: &SqlitePool,
    tenant_id: &str,
    adapters: &[ConversationAdapter],
) -> AppResult<()> {
    let mut prepared_adapters = adapters.to_vec();
    for adapter in &mut prepared_adapters {
        crate::backend::infrastructure::path_utils::normalize_conversation_adapter_paths(adapter)?;
    }

    store::seed_prepared_builtin_conversation_adapters_sqlx(pool, tenant_id, prepared_adapters)
        .await
        .map_err(AppError::external)?;
    normalize_conversation_paths(pool, tenant_id).await?;
    reconcile_app_conversation_adapters(pool, tenant_id).await?;
    Ok(())
}

/// Collapse legacy host-absolute paths back into portable anchors for one
/// tenant: conversation adapters, adapter packages, package versions and
/// conversation sources. Idempotent; only changed rows are rewritten.
pub async fn normalize_conversation_paths(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    for mut adapter in store::list_conversation_adapters_sqlx(pool, tenant_id).await? {
        crate::backend::infrastructure::path_utils::normalize_conversation_adapter_paths(
            &mut adapter,
        )?;
        store::upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter).await?;
    }

    crate::backend::application::conversations::conversation_sources::normalize_persisted_sources(
        pool, tenant_id,
    )
    .await?;

    for mut package in store::list_conversation_adapter_packages_sqlx(pool).await? {
        crate::backend::infrastructure::path_utils::normalize_conversation_adapter_package_paths(
            &mut package,
        )?;
        store::upsert_conversation_adapter_package_sqlx(pool, &package).await?;

        for mut version in
            store::list_conversation_adapter_package_versions_sqlx(pool, &package.package_id)
                .await?
        {
            crate::backend::infrastructure::path_utils::normalize_conversation_adapter_version_paths(
                &mut version,
            )?;
            store::update_conversation_adapter_package_version_install_dir_sqlx(
                pool,
                &version.package_id,
                &version.version,
                &version.install_dir,
            )
            .await?;
        }
    }
    Ok(())
}

pub async fn reconcile_app_conversation_adapters(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    let settings =
        crate::backend::infrastructure::app_settings::load_or_import_app_settings_sqlx(pool)
            .await?;
    // #199: legacy `dev_override` rows pointed straight at a source tree and skipped the
    // hash gate. Migrate them into immutable managed snapshots before projecting runtimes,
    // so the trust boundary applies to every remaining package.
    crate::backend::application::conversations::conversation_script_catalog::migrate_legacy_dev_override_packages(pool)
        .await?;
    // Self-healing: prune obsolete standalone workbuddy adapter records and normalize to codebuddy
    let _ = sqlx::query("DELETE FROM conversation_adapters WHERE id = 'workbuddy'")
        .execute(pool)
        .await;
    let _ = sqlx::query(
        "DELETE FROM conversation_sources WHERE id = 'workbuddy-live' OR adapter_id = 'workbuddy'",
    )
    .execute(pool)
    .await;
    let _ = sqlx::query(
        "UPDATE conversation_sessions SET adapter_id = 'codebuddy' WHERE adapter_id = 'workbuddy'",
    )
    .execute(pool)
    .await;
    let packages = store::list_conversation_adapter_packages_sqlx(pool).await?;
    for package in packages {
        let mut adapter = if package.runtime_ready
            && package.runtime_gate_status == ConversationAdapterRuntimeGateStatus::Ready
        {
            async {
                let manifest_path = crate::backend::infrastructure::path_utils::expand_path(
                    &package.adapter_manifest_path,
                )?;
                let preview =
                    crate::backend::infrastructure::conversations::register_external_adapter_with_settings(
                        crate::backend::infrastructure::conversations::ExternalAdapterRegisterParams {
                            manifest_path: manifest_path.to_string_lossy().to_string(),
                            dry_run: true,
                            yes: true,
                        },
                        &settings,
                    )
                    .await?;
                crate::backend::infrastructure::conversations::adapter_from_registration_preview(preview)
            }
            .await
            .map_err(|error| {
                tracing::warn!(
                    target: "assetiweave.operation",
                    operation = "app.environment.conversation_adapter_projection",
                    tenant_id = %tenant_id,
                    package_id = %package.package_id,
                    error = %error,
                    "conversation adapter package projection was disabled"
                );
                error
            })
            .ok()
        } else {
            None
        };
        if let Some(adapter) = adapter.as_mut() {
            crate::backend::infrastructure::path_utils::normalize_conversation_adapter_paths(
                adapter,
            )?;
        }
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
