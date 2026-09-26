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
pub(crate) async fn seed_prepared_builtin_adapters(
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

pub(crate) async fn normalize_conversation_paths(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    for mut adapter in store::list_conversation_adapters_sqlx(pool, tenant_id).await? {
        crate::backend::infrastructure::path_utils::normalize_conversation_adapter_paths(
            &mut adapter,
        )?;
        store::upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter).await?;
    }

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

pub(crate) async fn reconcile_app_conversation_adapters(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    let settings =
        crate::backend::infrastructure::app_settings::load_or_import_app_settings_sqlx(pool)
            .await?;
    let packages = store::list_conversation_adapter_packages_sqlx(pool).await?;
    for mut package in packages {
        if let Ok(install_dir) =
            crate::backend::infrastructure::path_utils::expand_path(&package.install_dir)
        {
            let is_official = crate::backend::infrastructure::conversations::is_official_adapter_id(
                &package.adapter_id,
            );
            if is_official {
                let _ = crate::backend::infrastructure::conversations::sync_official_adapter_to_package_dir(
                    &package.adapter_id,
                    &install_dir,
                );
                if let Ok(validation) =
                    crate::backend::infrastructure::conversations::validate_conversation_adapter_package_dir(
                        &install_dir,
                    )
                {
                    if validation.manifest.package_id == package.package_id {
                        let mut changed = false;
                        if package.version != validation.manifest.version {
                            package.version = validation.manifest.version;
                            changed = true;
                        }
                        if package.trusted_package_hash.as_deref() != Some(&validation.content_hash)
                        {
                            package.trusted_package_hash = Some(validation.content_hash.clone());
                            changed = true;
                        }
                        if package.installed_content_hash.as_deref()
                            != Some(&validation.content_hash)
                        {
                            package.installed_content_hash = Some(validation.content_hash.clone());
                            changed = true;
                        }
                        if !package.runtime_ready
                            || package.runtime_gate_status
                                != ConversationAdapterRuntimeGateStatus::Ready
                        {
                            package.runtime_ready = true;
                            package.runtime_gate_status =
                                ConversationAdapterRuntimeGateStatus::Ready;
                            package.error_message = None;
                            changed = true;
                        }
                        if changed {
                            package.updated_at = chrono::Utc::now().to_rfc3339();
                            let _ = store::upsert_conversation_adapter_package_sqlx(pool, &package)
                                .await;
                        }
                    }
                }
            }
        }
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
