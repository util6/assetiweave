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

impl AppService {
    pub(crate) async fn upgrade_conversation_adapter_workspace(
        &self,
        params: ConversationAdapterWorkspaceUpgradeParams,
    ) -> AppResult<Value> {
        if params.developer && params.package_dir.is_some() {
            return Err(AppError::Validation(
                "developer conversation adapter upgrade cannot be combined with package_dir"
                    .to_string(),
            ));
        }

        let managed_root =
            crate::backend::infrastructure::app_settings::conversation_adapter_dir()?;
        let (source_root, package_dirs) = if let Some(package_dir) = params.package_dir {
            let package_dir =
                crate::backend::infrastructure::path_utils::expand_path(&package_dir)?;
            (package_dir.clone(), vec![package_dir])
        } else if params.developer {
            let current_dir = env::current_dir().map_err(AppError::external)?;
            let adapter_root = find_developer_conversation_adapter_root(&current_dir)?;
            let package_dirs = current_dir
                .ancestors()
                .find(|candidate| {
                    candidate
                        .join("conversation-adapter-package.json")
                        .is_file()
                        && candidate.parent() == Some(adapter_root.as_path())
                })
                .map(|package_dir| vec![package_dir.to_path_buf()])
                .unwrap_or(discover_conversation_adapter_workspace_dirs(&adapter_root)?);
            (adapter_root, package_dirs)
        } else {
            let package_dirs = discover_conversation_adapter_workspace_dirs(&managed_root)?;
            (managed_root.clone(), package_dirs)
        };
        if package_dirs.is_empty() {
            return Err(AppError::Validation(format!(
                "no conversation adapter package directories found under {}",
                source_root.display()
            )));
        }

        let mut upgraded = Vec::with_capacity(package_dirs.len());
        for package_dir in package_dirs {
            upgraded.push(
                promote_conversation_adapter_workspace_package(
                    self,
                    &package_dir,
                    &managed_root,
                    params.dry_run,
                )
                .await?,
            );
        }
        if !params.dry_run {
            self.runtime.refresh_conversation_adapter_catalog().await?;
        }
        Ok(json!({
            "dry_run": params.dry_run,
            "developer": params.developer,
            "source_root": source_root,
            "upgraded": upgraded,
        }))
    }

    pub(crate) async fn inspect_conversation_adapter_package(
        &self,
        params: ConversationAdapterPackageInspectParams,
    ) -> AppResult<ConversationAdapterPackageInspection> {
        let package_id = params
            .package_id
            .as_deref()
            .and_then(clean_non_empty_string);
        let adapter_id = params
            .adapter_id
            .as_deref()
            .and_then(clean_non_empty_string);
        if package_id.is_none() && adapter_id.is_none() {
            return Err(AppError::Validation(
                "conversation adapter package inspection requires package_id or adapter_id"
                    .to_string(),
            ));
        }

        let mut package = match package_id.as_deref() {
            Some(package_id) => self.load_conversation_adapter_package(package_id).await?,
            None => {
                self.load_conversation_adapter_package_by_adapter(
                    adapter_id.as_deref().expect("adapter id checked above"),
                )
                .await?
            }
        };
        let resolved_adapter_id = package
            .as_ref()
            .map(|package| package.adapter_id.clone())
            .or_else(|| adapter_id.clone());
        let adapter = match resolved_adapter_id.as_deref() {
            Some(adapter_id) => self
                .list_conversation_adapters()?
                .into_iter()
                .find(|adapter| adapter.id == adapter_id),
            None => None,
        };
        if let Some(package) = package.as_mut() {
            self.refresh_conversation_adapter_package_runtime(package, adapter.as_ref())
                .await?;
        }
        if package.is_none() && adapter.is_none() {
            let id = package_id.or(adapter_id).unwrap_or_default();
            return Err(AppError::NotFound(format!(
                "conversation adapter package not found: {id}"
            )));
        }

        let origin = package
            .as_ref()
            .map(|package| package.origin)
            .unwrap_or_else(|| infer_unmanaged_adapter_origin(adapter.as_ref()));
        let affected_sources = if let Some(adapter_id) = resolved_adapter_id.as_deref() {
            self.list_conversation_sources()
                .await?
                .into_iter()
                .filter(|source| source.adapter_id == adapter_id)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        Ok(ConversationAdapterPackageInspection {
            origin,
            package,
            adapter,
            affected_sources,
        })
    }

    pub(crate) async fn register_conversation_adapter_local(
        &self,
        params: ConversationAdapterLocalRegisterParams,
    ) -> AppResult<Value> {
        if !matches!(
            params.origin,
            ConversationAdapterPackageOrigin::LocalDirectory
                | ConversationAdapterPackageOrigin::GitRef
                | ConversationAdapterPackageOrigin::DevOverride
        ) {
            return Err(AppError::Validation(
                "local conversation adapter registration requires local_directory, git_ref, or dev_override origin"
                    .to_string(),
            ));
        }
        if !params.dry_run && !params.yes {
            return Err(AppError::Validation(
                "conversation adapter local registration requires confirmation".to_string(),
            ));
        }
        if params.origin == ConversationAdapterPackageOrigin::GitRef
            && params
                .git_ref
                .as_deref()
                .and_then(clean_non_empty_string)
                .is_none()
        {
            return Err(AppError::Validation(
                "git_ref conversation adapter registration requires git_ref".to_string(),
            ));
        }

        let package_dir =
            crate::backend::infrastructure::path_utils::expand_path(&params.package_dir)?;
        let validation =
            crate::backend::infrastructure::conversations::validate_conversation_adapter_package_dir(&package_dir)
                .map_err(AppError::external)?;
        if let Some(existing) = self
            .load_conversation_adapter_package(&validation.manifest.package_id)
            .await?
        {
            if existing.origin == ConversationAdapterPackageOrigin::ManagedRelease {
                return Err(AppError::Validation(format!(
                    "managed conversation adapter package is already installed: {}",
                    existing.package_id
                )));
            }
        }

        let preflight = self
            .prepare_conversation_adapter_package_change(ConversationAdapterPackageChangeParams {
                action: ConversationAdapterPackageChangeAction::Register,
                package_id: None,
                adapter_id: Some(validation.adapter_validation.manifest.id.clone()),
            })
            .await
            .map_err(AppError::external)?;
        reject_conversation_package_task_conflicts(&preflight)?;
        let settings = self.app_settings_value();
        let preview =
            crate::backend::infrastructure::conversations::register_external_adapter_with_settings(
                crate::backend::infrastructure::conversations::ExternalAdapterRegisterParams {
                    manifest_path: validation.adapter_manifest_path.clone(),
                    dry_run: params.dry_run,
                    yes: params.yes,
                },
                &settings,
            )
            .await
            .map_err(AppError::external)?;
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "registered": false,
                "origin": params.origin,
                "package_dir": package_dir,
                "validation": validation,
                "preflight": preflight,
                "registration": preview
            }));
        }

        let adapter =
            crate::backend::infrastructure::conversations::adapter_from_registration_preview(
                preview,
            )
            .map_err(AppError::external)?;
        let now = Utc::now().to_rfc3339();
        let package = ConversationAdapterPackage {
            package_id: validation.manifest.package_id.clone(),
            adapter_id: adapter.id.clone(),
            name: validation.manifest.name.clone(),
            version: validation.manifest.version.clone(),
            record_kind: validation.manifest.record_kind,
            install_dir: package_dir.to_string_lossy().to_string(),
            manifest_path: validation.manifest_path.clone(),
            adapter_manifest_path: validation.adapter_manifest_path.clone(),
            runtime_protocol: validation.manifest.runtime.protocol.as_str().to_string(),
            runtime_ready: true,
            origin: params.origin,
            source_url: params
                .source_url
                .as_deref()
                .and_then(clean_non_empty_string),
            git_ref: params.git_ref.as_deref().and_then(clean_non_empty_string),
            git_commit: params
                .git_commit
                .as_deref()
                .and_then(clean_non_empty_string),
            catalog_url: None,
            update_policy: ConversationPackageUpdatePolicy::PinExact,
            latest_version: None,
            last_checked_at: None,
            runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
            runtime_validated_at: Some(now.clone()),
            installed_content_hash: Some(validation.content_hash.clone()),
            trusted_package_hash: (params.origin != ConversationAdapterPackageOrigin::DevOverride)
                .then(|| validation.content_hash.clone()),
            error_message: None,
            created_at: now.clone(),
            updated_at: now,
        };
        super::super::conversation_storage::activate_workspace(self.db.pool(), &adapter, &package)
            .await
            .map_err(AppError::external)?;
        self.runtime.refresh_conversation_adapter_catalog().await?;

        Ok(json!({
            "dry_run": false,
            "registered": true,
            "origin": params.origin,
            "package": package,
            "adapter": adapter,
            "validation": validation,
            "preflight": preflight
        }))
    }

    pub(crate) async fn prepare_conversation_adapter_package_change(
        &self,
        params: ConversationAdapterPackageChangeParams,
    ) -> AppResult<ConversationAdapterPackageChangePreflight> {
        let package_id = params
            .package_id
            .as_deref()
            .and_then(clean_non_empty_string);
        let adapter_id = params
            .adapter_id
            .as_deref()
            .and_then(clean_non_empty_string);
        let inspection = match params.action {
            ConversationAdapterPackageChangeAction::Install if package_id.is_some() => None,
            ConversationAdapterPackageChangeAction::Register if adapter_id.is_some() => None,
            _ => Some(
                self.inspect_conversation_adapter_package(
                    ConversationAdapterPackageInspectParams {
                        package_id: package_id.clone(),
                        adapter_id: adapter_id.clone(),
                    },
                )
                .await?,
            ),
        };
        let origin = inspection
            .as_ref()
            .map(|inspection| inspection.origin)
            .unwrap_or(match params.action {
                ConversationAdapterPackageChangeAction::Register => {
                    ConversationAdapterPackageOrigin::LocalDirectory
                }
                _ => ConversationAdapterPackageOrigin::ManagedRelease,
            });

        if origin == ConversationAdapterPackageOrigin::BuiltIn
            && params.action == ConversationAdapterPackageChangeAction::Uninstall
        {
            return Err(AppError::Validation(
                "built-in conversation adapters use disable, not package uninstall".to_string(),
            ));
        }
        if params.action == ConversationAdapterPackageChangeAction::Unregister
            && origin == ConversationAdapterPackageOrigin::ManagedRelease
        {
            return Err(AppError::Validation(
                "managed conversation adapter packages must be uninstalled, not unregistered"
                    .to_string(),
            ));
        }
        if params.action == ConversationAdapterPackageChangeAction::Uninstall
            && origin != ConversationAdapterPackageOrigin::ManagedRelease
        {
            return Err(AppError::Validation(
                "only managed conversation adapter packages can be uninstalled".to_string(),
            ));
        }

        let mut managed_paths = BTreeSet::new();
        if origin == ConversationAdapterPackageOrigin::ManagedRelease {
            if let Some(package) = inspection
                .as_ref()
                .and_then(|inspection| inspection.package.as_ref())
            {
                let managed_root =
                    crate::backend::infrastructure::app_settings::conversation_adapter_dir()?;
                let mut install_dirs = vec![package.install_dir.clone()];
                install_dirs.extend(
                    self.load_conversation_adapter_package_versions(&package.package_id)
                        .await?
                        .into_iter()
                        .map(|version| version.install_dir),
                );
                for install_dir in install_dirs {
                    let install_dir =
                        crate::backend::infrastructure::path_utils::expand_path(&install_dir)?;
                    if !install_dir.exists() {
                        continue;
                    }
                    let package_root = validate_managed_package_delete_target(
                        &managed_root,
                        &package.package_id,
                        &install_dir,
                    )?;
                    managed_paths.insert(package_root.to_string_lossy().to_string());
                }
            }
        }

        let resolved_adapter_id = inspection
            .as_ref()
            .and_then(|inspection| inspection.adapter.as_ref())
            .map(|adapter| adapter.id.clone())
            .or(adapter_id);
        let mut task_conflicts = Vec::new();
        if let Some(adapter_id) = resolved_adapter_id.as_deref() {
            if crate::backend::store::has_running_conversation_sync_for_adapter_sqlx(
                self.db.pool(),
                adapter_id,
            )
            .await
            .map_err(AppError::external)?
            {
                task_conflicts.push("conversation_sync".to_string());
            }
        }

        let risk = match params.action {
            ConversationAdapterPackageChangeAction::Revalidate => {
                ConversationAdapterPackageChangeRisk::ReadOnly
            }
            ConversationAdapterPackageChangeAction::Unregister => {
                ConversationAdapterPackageChangeRisk::Write
            }
            ConversationAdapterPackageChangeAction::Register
            | ConversationAdapterPackageChangeAction::Install
            | ConversationAdapterPackageChangeAction::Update
            | ConversationAdapterPackageChangeAction::Uninstall
            | ConversationAdapterPackageChangeAction::SwitchVersion
            | ConversationAdapterPackageChangeAction::Rollback
            | ConversationAdapterPackageChangeAction::DeleteVersion => {
                ConversationAdapterPackageChangeRisk::HighRiskWrite
            }
        };
        Ok(ConversationAdapterPackageChangePreflight {
            action: params.action,
            origin,
            package_id: inspection
                .as_ref()
                .and_then(|inspection| inspection.package.as_ref())
                .map(|package| package.package_id.clone())
                .or(package_id),
            adapter_id: resolved_adapter_id,
            managed_paths: managed_paths.into_iter().collect(),
            affected_sources: inspection
                .map(|inspection| inspection.affected_sources)
                .unwrap_or_default(),
            task_conflicts,
            preserves_conversation_records: true,
            risk,
            confirmation_required: risk != ConversationAdapterPackageChangeRisk::ReadOnly,
        })
    }
}
