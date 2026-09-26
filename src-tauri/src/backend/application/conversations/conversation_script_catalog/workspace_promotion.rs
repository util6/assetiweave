use super::*;
use crate::backend::application::prelude::*;
use crate::backend::domain::{
    ConversationAdapterPackageChangeAction, ConversationAdapterPackageChangeRisk,
    ConversationAdapterPackageOrigin, ConversationAdapterPackageRecordKind,
    ConversationAdapterRuntimeGateStatus, ConversationPackageUpdatePolicy,
};
use crate::backend::infrastructure::conversations::{
    ConversationAdapterPackageInstallSourceKind, ConversationAdapterPackageInstallSpec,
};

pub(crate) fn find_developer_conversation_adapter_root(start: &Path) -> AppResult<PathBuf> {
    for ancestor in start.ancestors() {
        let candidate = ancestor.join("builtin-assets").join("adapters");
        if candidate.is_dir() {
            return Ok(candidate);
        }
    }
    Err(AppError::NotFound(format!(
        "builtin-assets/adapters was not found from {} or its ancestors",
        start.display()
    )))
}

pub(crate) fn discover_conversation_adapter_workspace_dirs(root: &Path) -> AppResult<Vec<PathBuf>> {
    if !root.is_dir() {
        return Err(AppError::Validation(format!(
            "conversation adapter workspace root is not a directory: {}",
            root.display()
        )));
    }
    let mut package_dirs = fs::read_dir(root)
        .map_err(AppError::external)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if matches!(name.as_ref(), "packages" | "staging") {
                return None;
            }
            let path = entry.path();
            (entry.file_type().ok()?.is_dir()
                && path.join("conversation-adapter-package.json").is_file())
            .then_some(path)
        })
        .collect::<Vec<_>>();
    package_dirs.sort();
    Ok(package_dirs)
}

pub(crate) async fn promote_conversation_adapter_workspace_package(
    service: &AppService,
    package_dir: &Path,
    managed_root: &Path,
    dry_run: bool,
) -> AppResult<Value> {
    let source_dir = package_dir
        .canonicalize()
        .map_err(|error| format!("resolve adapter workspace failed: {error}"))
        .map_err(AppError::external)?;
    let source_validation =
        crate::backend::infrastructure::conversations::validate_conversation_adapter_package_dir(
            &source_dir,
        )
        .map_err(AppError::external)?;
    let adapter_id = source_validation.adapter_validation.manifest.id.as_str();
    let directory_name = source_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if directory_name != adapter_id {
        return Err(AppError::Validation(format!(
            "conversation adapter workspace directory must match adapter id: expected {adapter_id}, found {directory_name}"
        )));
    }
    if source_validation.manifest.version != source_validation.adapter_validation.manifest.version {
        return Err(AppError::Validation(format!(
            "conversation adapter package and adapter versions must match: {} != {}",
            source_validation.manifest.version,
            source_validation.adapter_validation.manifest.version
        )));
    }
    let version = validated_package_version(&source_validation.manifest.version)?;
    let source_version = semver::Version::parse(&version)
        .map_err(|error| format!("conversation adapter package version must be SemVer: {error}"))
        .map_err(AppError::external)?;
    if let Some(active_package) = service
        .load_conversation_adapter_package_by_adapter(adapter_id)
        .await?
    {
        if let Ok(active_version) = semver::Version::parse(active_package.version.trim()) {
            if active_version > source_version {
                return Ok(json!({
                    "dry_run": dry_run,
                    "upgraded": false,
                    "skipped": true,
                    "reason": "active_version_newer",
                    "source_dir": source_dir,
                    "package_id": source_validation.manifest.package_id,
                    "adapter_id": adapter_id,
                    "version": version,
                    "active_version": active_version.to_string(),
                }));
            }
        }
    }
    let preflight = service
        .prepare_conversation_adapter_package_change(ConversationAdapterPackageChangeParams {
            action: ConversationAdapterPackageChangeAction::Register,
            package_id: None,
            adapter_id: Some(adapter_id.to_string()),
        })
        .await?;
    reject_conversation_package_task_conflicts(&preflight)?;

    let revision = format!(
        "{}-{}",
        version,
        &source_validation.content_hash[..12.min(source_validation.content_hash.len())]
    );
    let package_root = managed_root
        .join("packages")
        .join(&source_validation.manifest.package_id);
    let version_dir = package_root.join("versions").join(&revision);
    if dry_run {
        return Ok(json!({
            "dry_run": true,
            "source_dir": source_dir,
            "install_dir": version_dir,
            "package_id": source_validation.manifest.package_id,
            "adapter_id": adapter_id,
            "version": version,
            "content_hash": source_validation.content_hash,
            "preflight": preflight,
        }));
    }

    let settings = service.app_settings_value();
    let prepared_dir = package_root.join("prepared").join(short_uuid());
    if let Some(parent) = prepared_dir.parent() {
        fs::create_dir_all(parent).map_err(AppError::external)?;
    }
    crate::backend::infrastructure::filesystem::copy_dir(&source_dir, &prepared_dir)?;
    let promotion: AppResult<_> = async {
        let prepared_validation =
            crate::backend::infrastructure::conversations::validate_conversation_adapter_package_dir(&prepared_dir)
                .map_err(AppError::external)?;
        if prepared_validation.content_hash != source_validation.content_hash {
            return Err(AppError::Validation(
                "conversation adapter workspace changed while its runtime snapshot was being created"
                    .to_string(),
            ));
        }
        crate::backend::infrastructure::conversations::register_external_adapter_with_settings(
            crate::backend::infrastructure::conversations::ExternalAdapterRegisterParams {
                manifest_path: prepared_validation.adapter_manifest_path,
                dry_run: false,
                yes: true,
            },
            &settings,
        )
        .await
        .map_err(AppError::external)?;

        let created_version_dir = if version_dir.exists() {
            let existing =
                crate::backend::infrastructure::conversations::validate_conversation_adapter_package_dir(
                    &version_dir,
                )
                .map_err(AppError::external)?;
            if existing.content_hash != source_validation.content_hash {
                return Err(AppError::Validation(format!(
                    "conversation adapter runtime revision is immutable: {}",
                    version_dir.display()
                )));
            }
            fs::remove_dir_all(&prepared_dir).map_err(AppError::external)?;
            false
        } else {
            let parent = version_dir
                .parent()
                .ok_or_else(|| "conversation adapter runtime has no versions directory".to_string())
                .map_err(AppError::external)?;
            fs::create_dir_all(parent).map_err(AppError::external)?;
            fs::rename(&prepared_dir, &version_dir).map_err(AppError::external)?;
            true
        };

        let final_validation =
            crate::backend::infrastructure::conversations::validate_conversation_adapter_package_dir(&version_dir)
                .map_err(AppError::external)?;
        let preview = crate::backend::infrastructure::conversations::register_external_adapter_with_settings(
            crate::backend::infrastructure::conversations::ExternalAdapterRegisterParams {
                manifest_path: final_validation.adapter_manifest_path.clone(),
                dry_run: true,
                yes: true,
            },
            &settings,
        )
        .await
        .map_err(AppError::external)?;
        let adapter = crate::backend::infrastructure::conversations::adapter_from_registration_preview(preview)
            .map_err(AppError::external)?;
        let previous_package = service
            .load_conversation_adapter_package(&final_validation.manifest.package_id)
            .await?;
        let now = Utc::now().to_rfc3339();
        let package = ConversationAdapterPackage {
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
            created_at: previous_package
                .as_ref()
                .map(|package| package.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        let activation = super::super::conversation_storage::activate_workspace(
            service.db.pool(),
            &adapter,
            &package,
        )
        .await;
        if let Err(error) = activation {
            if created_version_dir {
                let _ = fs::remove_dir_all(&version_dir);
            }
            return Err(error);
        }
        let cleanup_warning =
            retain_only_active_workspace_runtime(&package_root, &version_dir).err();
        Ok(json!({
            "dry_run": false,
            "upgraded": true,
            "source_dir": source_dir,
            "package": package,
            "adapter": adapter,
            "validation": final_validation,
            "preflight": preflight,
            "cleanup_warning": cleanup_warning,
        }))
    }.await;
    if promotion.is_err() {
        let _ = fs::remove_dir_all(&prepared_dir);
    }
    promotion
}

pub(crate) fn retain_only_active_workspace_runtime(
    package_root: &Path,
    active_dir: &Path,
) -> AppResult<()> {
    let versions_dir = package_root.join("versions");
    if !versions_dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(&versions_dir).map_err(AppError::external)? {
        let entry = entry.map_err(AppError::external)?;
        let path = entry.path();
        if path == active_dir {
            continue;
        }
        let file_type = entry.file_type().map_err(AppError::external)?;
        if file_type.is_dir() {
            fs::remove_dir_all(path).map_err(AppError::external)?;
        }
    }
    Ok(())
}

pub(crate) fn discover_local_conversation_adapter_packages(
    adapter_root: &Path,
) -> AppResult<Vec<ConversationScriptCatalogItem>> {
    if !adapter_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut package_dirs = fs::read_dir(adapter_root)
        .map_err(AppError::external)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if matches!(file_name.as_ref(), "packages" | "staging") {
                return None;
            }
            entry.file_type().ok()?.is_dir().then(|| entry.path())
        })
        .collect::<Vec<_>>();
    package_dirs.sort();

    Ok(package_dirs
        .into_iter()
        .filter_map(|package_dir| {
            let validation =
                crate::backend::infrastructure::conversations::validate_conversation_adapter_package_dir(
                    &package_dir,
                )
                .ok()?;
            let record_kind = match validation.manifest.record_kind {
                ConversationAdapterPackageRecordKind::Session => {
                    ConversationScriptRecordKind::Session
                }
                ConversationAdapterPackageRecordKind::Web => ConversationScriptRecordKind::Web,
            };
            let manifest_file = Path::new(&validation.adapter_manifest_path)
                .file_name()
                .and_then(|value| value.to_str())
                .map(str::to_string);
            Some(ConversationScriptCatalogItem {
                id: validation.manifest.package_id,
                name: validation.manifest.name,
                version: validation.manifest.version,
                record_kind,
                provider: Some("local_directory".to_string()),
                adapter_id: Some(validation.adapter_validation.manifest.id),
                description: None,
                homepage_url: None,
                repository_url: None,
                tags: Vec::new(),
                manifest_file,
                package_manifest_file: Some("conversation-adapter-package.json".to_string()),
                expected_content_hash: None,
                expected_package_hash: Some(validation.content_hash),
                expected_artifact_hash: None,
                artifact_size: None,
                source: ConversationScriptCatalogSource {
                    kind: ConversationScriptCatalogSourceKind::LocalDirectory,
                    url: package_dir.to_string_lossy().to_string(),
                    branch: None,
                    path: None,
                },
            })
        })
        .collect())
}

pub(crate) fn infer_unmanaged_adapter_origin(
    adapter: Option<&ConversationAdapter>,
) -> ConversationAdapterPackageOrigin {
    match adapter.map(|adapter| adapter.trust_state) {
        Some(crate::backend::domain::ConversationAdapterTrustState::BuiltIn) => {
            ConversationAdapterPackageOrigin::BuiltIn
        }
        _ => ConversationAdapterPackageOrigin::LegacyExternal,
    }
}

pub(crate) fn reject_conversation_package_task_conflicts(
    preflight: &ConversationAdapterPackageChangePreflight,
) -> AppResult<()> {
    if preflight.task_conflicts.is_empty() {
        Ok(())
    } else {
        Err(AppError::Conflict(format!(
            "conversation adapter package change conflicts with running tasks: {}",
            preflight.task_conflicts.join(", ")
        )))
    }
}

pub(crate) fn select_rollback_version<'a>(
    versions: &'a [crate::backend::domain::ConversationAdapterPackageVersion],
    active_version: &str,
) -> Option<&'a crate::backend::domain::ConversationAdapterPackageVersion> {
    versions
        .iter()
        .filter(|version| version.version != active_version)
        .max_by(|left, right| {
            left.installed_at
                .cmp(&right.installed_at)
                .then_with(|| left.version.cmp(&right.version))
        })
}
