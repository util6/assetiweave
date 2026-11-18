use crate::backend::{
    application::prelude::{AppError, AppResult},
    domain::{ConversationAdapter, ConversationAdapterRuntimeGateStatus},
    store,
};
use sqlx::SqlitePool;

/// Migrate legacy `dev_override` package rows into immutable managed snapshots.
///
/// #199/#195：这段迁移会复制目录并计算内容 hash，属于启动期的重 I/O 对账，
/// 不能挂在租户切换这类即时请求路径上（ADR-0017）。它只在启动对账时执行一次，
/// 且失败只记录告警：遗留行会被就地标记为不可运行，用户修复后下次启动重试，
/// 不能因为一次迁移失败阻断整个启动。
pub async fn migrate_legacy_dev_override_packages_on_startup(pool: &SqlitePool) {
    match crate::backend::application::conversations::conversation_script_catalog::migrate_legacy_dev_override_packages(pool).await {
        Ok(0) => {}
        Ok(migrated) => tracing::info!(
            target: "assetiweave.operation",
            operation = "app.environment.conversation_adapter_dev_override_migration",
            migrated,
            "legacy dev_override conversation adapter packages were migrated at startup"
        ),
        Err(error) => tracing::warn!(
            target: "assetiweave.operation",
            operation = "app.environment.conversation_adapter_dev_override_migration",
            error = %error,
            "legacy dev_override conversation adapter migration was skipped"
        ),
    }
}

/// Project the adapters discovered in the managed user directory into one tenant.
///
/// Application owns the orchestration; infrastructure normalizes file paths and
/// materializes adapter metadata while Store persists the observed rows. The
/// seed never grants trust: discovery records what is on disk, and only an
/// explicit promotion/install/registration writes a trusted hash.
pub async fn seed_discovered_conversation_adapters(
    pool: &SqlitePool,
    tenant_id: &str,
    adapters: &[ConversationAdapter],
) -> AppResult<()> {
    let mut discovered_adapters = adapters.to_vec();
    for adapter in &mut discovered_adapters {
        crate::backend::infrastructure::path_utils::normalize_conversation_adapter_paths(adapter)?;
    }

    store::seed_discovered_conversation_adapters_sqlx(pool, tenant_id, discovered_adapters)
        .await
        .map_err(AppError::external)?;
    normalize_conversation_paths(pool, tenant_id).await?;
    reconcile_app_conversation_adapters(pool, tenant_id).await?;
    // 对账过程可能投影出市场包适配器（例如从旧版本升级上来的库），其来源行
    // 可能从未播种过。再补一次：只插入缺失行，不触碰用户改动。补播种是尽力
    // 而为，失败不应阻断启动。
    if let Err(error) =
        crate::backend::store::seed_builtin_conversation_sources_sqlx(pool, tenant_id).await
    {
        tracing::warn!(
            target: "assetiweave.operation",
            operation = "app.conversation_adapter.builtin_source_seed",
            tenant_id = %tenant_id,
            error = %error,
            "builtin conversation source backfill was skipped"
        );
    }
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
