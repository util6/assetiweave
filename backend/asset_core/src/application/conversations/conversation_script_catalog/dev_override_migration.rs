//! #199：把遗留 `DevOverride` 包记录迁移为受管目录下的不可变版本快照。
//!
//! 旧版 `register_conversation_adapter_dev` 会把 `install_dir` 直接指向源码树并跳过
//! hash 校验。废除该特权分支后，这些行既不应该继续免检，也不应该被静默删除，因此
//! 环境对账时按工作区晋升的语义就地迁移：
//!
//! - 源目录有效 → 复制为 `packages/<package_id>/versions/<version>-<hash12>` 不可变快照，
//!   `origin` 改写为 `LocalDirectory`，`trusted_package_hash` 写入快照内容 hash，
//!   `install_dir` 由路径归一化折叠为便携锚点；
//! - 源目录缺失或清单非法 → 保留行与用户数据，只标记为不可运行，等待用户修复后重试。
//!
//! 迁移幂等：快照目录按内容 hash 命名，已存在且 hash 相同的快照直接复用；迁移成功后
//! `origin` 不再是 `DevOverride`，重复对账不会再触碰该行。

use super::*;
use crate::backend::application::prelude::*;
use crate::backend::domain::{
    ConversationAdapterPackageOrigin, ConversationAdapterRuntimeGateStatus,
    ConversationPackageUpdatePolicy,
};
use crate::backend::infrastructure::conversations::ConversationAdapterPackageValidationResult;
use sqlx::SqlitePool;

/// 迁移所有遗留 `DevOverride` 包记录，返回本次成功迁移的行数。
///
/// 单行失败不会中断对账：失败行被标记为不可运行并记录原因。
pub async fn migrate_legacy_dev_override_packages(pool: &SqlitePool) -> AppResult<usize> {
    let packages = crate::backend::store::list_conversation_adapter_packages_sqlx(pool).await?;
    if !packages
        .iter()
        .any(|package| package.origin == ConversationAdapterPackageOrigin::DevOverride)
    {
        return Ok(0);
    }
    let managed_root = crate::backend::infrastructure::app_settings::conversation_adapter_dir()
        .map_err(AppError::external)?;
    let mut migrated = 0;
    for mut package in packages {
        if package.origin != ConversationAdapterPackageOrigin::DevOverride {
            continue;
        }
        match migrate_dev_override_package(pool, &mut package, &managed_root).await {
            Ok(()) => migrated += 1,
            Err(error) => {
                let message = error.to_string();
                tracing::warn!(
                    target: "assetiweave.operation",
                    operation = "app.environment.conversation_adapter_dev_override_migration",
                    package_id = %package.package_id,
                    error = %message,
                    "legacy dev_override package could not be migrated and was marked unready"
                );
                mark_unready(pool, &mut package, &message).await?;
            }
        }
    }
    Ok(migrated)
}

async fn migrate_dev_override_package(
    pool: &SqlitePool,
    package: &mut ConversationAdapterPackage,
    managed_root: &Path,
) -> AppResult<()> {
    let source_dir = crate::backend::infrastructure::path_utils::expand_path(&package.install_dir)?;
    let source_validation = validate_package_dir(&source_dir)?;
    if source_validation.manifest.package_id != package.package_id {
        return Err(AppError::Validation(format!(
            "conversation adapter package manifest id {} does not match registered package {}",
            source_validation.manifest.package_id, package.package_id
        )));
    }
    let version = validated_package_version(&source_validation.manifest.version)?;
    let revision = format!(
        "{}-{}",
        version,
        &source_validation.content_hash[..12.min(source_validation.content_hash.len())]
    );
    let package_root = managed_root.join("packages").join(&package.package_id);
    let version_dir = package_root.join("versions").join(&revision);
    let expected_hash = source_validation.content_hash.clone();
    let snapshot_source = source_dir.clone();
    let snapshot_dir = version_dir.clone();
    tokio::task::spawn_blocking(move || {
        ensure_immutable_snapshot(&snapshot_source, &snapshot_dir, &expected_hash)
    })
    .await
    .map_err(AppError::external)??;

    let final_validation = validate_package_dir(&version_dir)?;
    let settings =
        crate::backend::infrastructure::app_settings::load_or_import_app_settings_sqlx(pool)
            .await
            .map_err(AppError::external)?;
    crate::backend::infrastructure::conversations::register_external_adapter_with_settings(
        crate::backend::infrastructure::conversations::ExternalAdapterRegisterParams {
            manifest_path: final_validation.adapter_manifest_path.clone(),
            dry_run: false,
            yes: true,
        },
        &settings,
    )
    .await
    .map_err(AppError::external)?;
    let preview =
        crate::backend::infrastructure::conversations::register_external_adapter_with_settings(
            crate::backend::infrastructure::conversations::ExternalAdapterRegisterParams {
                manifest_path: final_validation.adapter_manifest_path.clone(),
                dry_run: true,
                yes: true,
            },
            &settings,
        )
        .await
        .map_err(AppError::external)?;
    let adapter =
        crate::backend::infrastructure::conversations::adapter_from_registration_preview(preview)
            .map_err(AppError::external)?;

    let now = Utc::now().to_rfc3339();
    let migrated = ConversationAdapterPackage {
        package_id: final_validation.manifest.package_id.clone(),
        adapter_id: adapter.id.clone(),
        name: final_validation.manifest.name.clone(),
        version: final_validation.manifest.version.clone(),
        record_kind: final_validation.manifest.record_kind,
        install_dir: version_dir.to_string_lossy().to_string(),
        manifest_path: final_validation.manifest_path.clone(),
        adapter_manifest_path: final_validation.adapter_manifest_path.clone(),
        runtime_protocol: final_validation
            .manifest
            .runtime
            .protocol
            .as_str()
            .to_string(),
        runtime_ready: true,
        origin: ConversationAdapterPackageOrigin::LocalDirectory,
        source_url: Some(source_dir.to_string_lossy().to_string()),
        git_ref: None,
        git_commit: None,
        catalog_url: None,
        update_policy: ConversationPackageUpdatePolicy::PinExact,
        latest_version: Some(final_validation.manifest.version.clone()),
        last_checked_at: Some(now.clone()),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        runtime_validated_at: Some(now.clone()),
        installed_content_hash: Some(final_validation.content_hash.clone()),
        trusted_package_hash: Some(final_validation.content_hash.clone()),
        error_message: None,
        created_at: package.created_at.clone(),
        updated_at: now,
    };
    super::super::conversation_storage::activate_workspace(pool, &adapter, &migrated).await?;
    if let Err(error) = retain_only_active_workspace_runtime(&package_root, &version_dir) {
        tracing::warn!(
            target: "assetiweave.operation",
            operation = "app.environment.conversation_adapter_dev_override_migration",
            package_id = %migrated.package_id,
            error = %error,
            "migrated conversation adapter snapshot cleanup was skipped"
        );
    }
    *package = migrated;
    Ok(())
}

fn validate_package_dir(dir: &Path) -> AppResult<ConversationAdapterPackageValidationResult> {
    crate::backend::infrastructure::conversations::validate_conversation_adapter_package_dir(dir)
        .map_err(AppError::external)
}

/// Create `version_dir` as an immutable copy of `source_dir`, reusing an existing
/// revision whose content hash already matches. Runs on the blocking pool because
/// directory hashing and copying can exceed the async budget.
fn ensure_immutable_snapshot(
    source_dir: &Path,
    version_dir: &Path,
    expected_hash: &str,
) -> AppResult<()> {
    if version_dir.is_dir() {
        let existing = validate_package_dir(version_dir)?;
        if existing.content_hash != expected_hash {
            return Err(AppError::Validation(format!(
                "conversation adapter runtime revision is immutable: {}",
                version_dir.display()
            )));
        }
        return Ok(());
    }
    let package_root = version_dir
        .parent()
        .and_then(|versions| versions.parent())
        .ok_or_else(|| {
            AppError::Validation(
                "conversation adapter runtime has no versions directory".to_string(),
            )
        })?;
    let prepared_dir = package_root.join("prepared").join(short_uuid());
    if let Some(parent) = prepared_dir.parent() {
        fs::create_dir_all(parent).map_err(AppError::external)?;
    }
    crate::backend::infrastructure::filesystem::copy_dir(source_dir, &prepared_dir)
        .map_err(AppError::external)?;
    let copied = validate_package_dir(&prepared_dir)
        .and_then(|validation| {
            if validation.content_hash != expected_hash {
                return Err(AppError::Validation(
                    "conversation adapter workspace changed while its runtime snapshot was being created"
                        .to_string(),
                ));
            }
            Ok(())
        })
        .and_then(|()| {
            fs::create_dir_all(version_dir.parent().unwrap_or(package_root))
                .map_err(AppError::external)?;
            fs::rename(&prepared_dir, version_dir).map_err(AppError::external)
        });
    if copied.is_err() {
        let _ = fs::remove_dir_all(&prepared_dir);
    }
    copied
}

async fn mark_unready(
    pool: &SqlitePool,
    package: &mut ConversationAdapterPackage,
    message: &str,
) -> AppResult<()> {
    let install_dir = crate::backend::infrastructure::path_utils::expand_path(&package.install_dir)
        .unwrap_or_else(|_| PathBuf::from(&package.install_dir));
    let gate_status = classify_runtime_gate_error(&install_dir, message);
    if !package.runtime_ready
        && package.runtime_gate_status == gate_status
        && package.error_message.as_deref() == Some(message)
    {
        return Ok(());
    }
    let now = Utc::now().to_rfc3339();
    package.runtime_ready = false;
    package.runtime_gate_status = gate_status;
    package.error_message = Some(message.to_string());
    package.runtime_validated_at = Some(now.clone());
    package.updated_at = now;
    crate::backend::store::upsert_conversation_adapter_package_sqlx(pool, package)
        .await
        .map_err(AppError::external)
}
