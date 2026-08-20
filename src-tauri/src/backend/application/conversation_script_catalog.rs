use super::prelude::*;
use crate::backend::conversations::{
    ConversationAdapterPackageInstallSourceKind, ConversationAdapterPackageInstallSpec,
};
use crate::backend::extension_kernel::DomainPackageSystem;
use crate::backend::models::{
    ConversationAdapterPackageChangeAction, ConversationAdapterPackageChangeRisk,
    ConversationAdapterPackageOrigin, ConversationAdapterPackageRecordKind,
    ConversationAdapterRuntimeGateStatus, ConversationPackageUpdatePolicy,
};

const DEFAULT_CONVERSATION_SCRIPT_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/util6/assetiweave/main/builtin-assets/catalog.json";
const LOCAL_DEFAULT_CONVERSATION_SCRIPT_CATALOG: &str =
    include_str!("../../../../builtin-assets/catalog.json");
impl AppService {
    pub(crate) fn upgrade_conversation_adapter_workspace(
        &self,
        params: ConversationAdapterWorkspaceUpgradeParams,
    ) -> AppResult<Value> {
        if params.developer && params.package_dir.is_some() {
            return Err(AppError::Validation(
                "developer conversation adapter upgrade cannot be combined with package_dir"
                    .to_string(),
            ));
        }

        let managed_root = crate::backend::app_settings::conversation_adapter_dir()?;
        let (source_root, package_dirs) = if let Some(package_dir) = params.package_dir {
            let package_dir = crate::backend::path_utils::expand_path(&package_dir)?;
            (package_dir.clone(), vec![package_dir])
        } else if params.developer {
            let current_dir = env::current_dir().map_err(|error| error.to_string())?;
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
            upgraded.push(promote_conversation_adapter_workspace_package(
                self,
                &package_dir,
                &managed_root,
                params.dry_run,
            )?);
        }
        if !params.dry_run {
            self.runtime.refresh_conversation_adapter_catalog()?;
        }
        Ok(json!({
            "dry_run": params.dry_run,
            "developer": params.developer,
            "source_root": source_root,
            "upgraded": upgraded,
        }))
    }

    pub(crate) fn inspect_conversation_adapter_package(
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
            Some(package_id) => self.load_conversation_adapter_package(package_id)?,
            None => self.load_conversation_adapter_package_by_adapter(
                adapter_id.as_deref().expect("adapter id checked above"),
            )?,
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
            self.refresh_conversation_adapter_package_runtime(package, adapter.as_ref())?;
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
        let affected_sources = resolved_adapter_id
            .as_deref()
            .map(|adapter_id| {
                self.list_conversation_sources().map(|sources| {
                    sources
                        .into_iter()
                        .filter(|source| source.adapter_id == adapter_id)
                        .collect::<Vec<_>>()
                })
            })
            .transpose()?
            .unwrap_or_default();
        Ok(ConversationAdapterPackageInspection {
            origin,
            package,
            adapter,
            affected_sources,
        })
    }

    pub(crate) fn register_conversation_adapter_local(
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

        let package_dir = crate::backend::path_utils::expand_path(&params.package_dir)?;
        let validation =
            crate::backend::conversations::validate_conversation_adapter_package_dir(&package_dir)?;
        if let Some(existing) =
            self.load_conversation_adapter_package(&validation.manifest.package_id)?
        {
            if existing.origin == ConversationAdapterPackageOrigin::ManagedRelease {
                return Err(AppError::Validation(format!(
                    "managed conversation adapter package is already installed: {}",
                    existing.package_id
                )));
            }
        }

        let preflight = self.prepare_conversation_adapter_package_change(
            ConversationAdapterPackageChangeParams {
                action: ConversationAdapterPackageChangeAction::Register,
                package_id: None,
                adapter_id: Some(validation.adapter_validation.manifest.id.clone()),
            },
        )?;
        reject_conversation_package_task_conflicts(&preflight)?;
        let preview = crate::backend::conversations::register_external_adapter(
            crate::backend::conversations::ExternalAdapterRegisterParams {
                manifest_path: validation.adapter_manifest_path.clone(),
                dry_run: params.dry_run,
                yes: params.yes,
            },
        )?;
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

        let adapter = crate::backend::conversations::adapter_from_registration_preview(preview)?;
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
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let adapter_to_save = adapter.clone();
        self.db.block_on(async move {
            crate::backend::store::upsert_conversation_adapter_sqlx(
                &pool,
                &tenant_id,
                &adapter_to_save,
            )
            .await
        })?;
        self.save_conversation_adapter_package(&package)?;
        self.runtime.refresh_conversation_adapter_catalog()?;

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

    pub(crate) fn prepare_conversation_adapter_package_change(
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
            _ => Some(self.inspect_conversation_adapter_package(
                ConversationAdapterPackageInspectParams {
                    package_id: package_id.clone(),
                    adapter_id: adapter_id.clone(),
                },
            )?),
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
                let managed_root = crate::backend::app_settings::conversation_adapter_dir()?;
                let mut install_dirs = vec![package.install_dir.clone()];
                install_dirs.extend(
                    self.load_conversation_adapter_package_versions(&package.package_id)?
                        .into_iter()
                        .map(|version| version.install_dir),
                );
                for install_dir in install_dirs {
                    let install_dir = crate::backend::path_utils::expand_path(&install_dir)?;
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
            let pool = self.db.pool().clone();
            let tenant_id = self.tenant_id().to_string();
            let adapter_id = adapter_id.to_string();
            if self.db.block_on(async move {
                crate::backend::store::has_running_conversation_sync_for_adapter_sqlx(
                    &pool,
                    &tenant_id,
                    &adapter_id,
                )
                .await
            })? {
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

    pub(crate) fn list_conversation_adapter_packages(
        &self,
        params: ConversationAdapterPackageCatalogParams,
    ) -> AppResult<Vec<ConversationAdapterPackageCatalogEntry>> {
        let mut catalog = load_conversation_script_catalog(params.catalog_url.as_deref())?;
        for item in discover_local_conversation_adapter_packages(
            &crate::backend::app_settings::conversation_adapter_dir()?,
        )? {
            if let Some(existing) = catalog
                .items
                .iter_mut()
                .find(|existing| existing.package_id() == item.package_id())
            {
                *existing = item;
            } else {
                catalog.items.push(item);
            }
        }
        let adapters = self.list_conversation_adapters()?;
        let mut packages = self.load_conversation_adapter_packages()?;
        for package in &mut packages {
            let adapter = adapters
                .iter()
                .find(|adapter| adapter.id == package.adapter_id);
            self.refresh_conversation_adapter_package_runtime(package, adapter)?;
        }
        Ok(resolve_conversation_adapter_package_catalog_entries(
            catalog.items,
            &adapters,
            &packages,
        ))
    }

    pub(crate) fn list_conversation_script_catalog(
        &self,
        params: ConversationScriptCatalogParams,
    ) -> AppResult<Vec<ConversationScriptCatalogEntry>> {
        let entries =
            self.list_conversation_adapter_packages(ConversationAdapterPackageCatalogParams {
                catalog_url: params.catalog_url,
            })?;
        Ok(entries
            .into_iter()
            .map(ConversationScriptCatalogEntry::from)
            .collect())
    }

    pub(crate) fn install_conversation_adapter_package(
        &self,
        params: ConversationAdapterPackageInstallParams,
    ) -> AppResult<Value> {
        let preflight = self.prepare_conversation_adapter_package_change(
            ConversationAdapterPackageChangeParams {
                action: ConversationAdapterPackageChangeAction::Install,
                package_id: Some(params.package_id.clone()),
                adapter_id: None,
            },
        )?;
        reject_conversation_package_task_conflicts(&preflight)?;
        if !params.dry_run && !params.yes {
            return Err(AppError::Validation(
                "conversation adapter package install requires --yes".to_string(),
            ));
        }
        if params
            .version
            .as_deref()
            .and_then(clean_non_empty_string)
            .is_some()
        {
            let result = self.install_conversation_adapter_package_release(params)?;
            self.runtime.refresh_conversation_adapter_catalog()?;
            return Ok(result);
        }

        let catalog = load_conversation_script_catalog(params.catalog_url.as_deref())?;
        let package_id = params.package_id.trim();
        let item = catalog
            .items
            .into_iter()
            .find(|item| item.package_id() == package_id)
            .ok_or_else(|| format!("conversation adapter package not found: {package_id}"))?;
        validate_conversation_script_catalog_item(&item)?;

        let result = install_conversation_adapter_package_from_item(
            self,
            &item,
            params.dry_run,
            params.catalog_url.as_deref(),
        )?;
        if !params.dry_run {
            self.runtime.refresh_conversation_adapter_catalog()?;
        }
        Ok(result)
    }

    pub(crate) fn update_conversation_adapter_package(
        &self,
        params: ConversationAdapterPackageInstallParams,
    ) -> AppResult<Value> {
        let preflight = self.prepare_conversation_adapter_package_change(
            ConversationAdapterPackageChangeParams {
                action: ConversationAdapterPackageChangeAction::Update,
                package_id: Some(params.package_id.clone()),
                adapter_id: None,
            },
        )?;
        reject_conversation_package_task_conflicts(&preflight)?;
        if !params.dry_run && !params.yes {
            return Err(AppError::Validation(
                "conversation adapter package update requires --yes".to_string(),
            ));
        }
        if params
            .version
            .as_deref()
            .and_then(clean_non_empty_string)
            .is_some()
        {
            let result = self.install_conversation_adapter_package_release(params)?;
            self.runtime.refresh_conversation_adapter_catalog()?;
            return Ok(result);
        }

        let catalog = load_conversation_script_catalog(params.catalog_url.as_deref())?;
        let package_id = params.package_id.trim();
        let item = catalog
            .items
            .into_iter()
            .find(|item| item.package_id() == package_id)
            .ok_or_else(|| format!("conversation adapter package not found: {package_id}"))?;
        validate_conversation_script_catalog_item(&item)?;
        let result = install_conversation_adapter_package_from_item(
            self,
            &item,
            params.dry_run,
            params.catalog_url.as_deref(),
        )?;
        if !params.dry_run {
            self.runtime.refresh_conversation_adapter_catalog()?;
        }
        Ok(result)
    }

    pub(crate) fn uninstall_conversation_adapter_package(
        &self,
        params: ConversationAdapterPackageUninstallParams,
    ) -> AppResult<Value> {
        let preflight = self.prepare_conversation_adapter_package_change(
            ConversationAdapterPackageChangeParams {
                action: ConversationAdapterPackageChangeAction::Uninstall,
                package_id: Some(params.package_id.clone()),
                adapter_id: None,
            },
        )?;
        reject_conversation_package_task_conflicts(&preflight)?;
        if !params.dry_run && !params.yes {
            return Err(AppError::Validation(
                "conversation adapter package uninstall requires --yes".to_string(),
            ));
        }
        let package_id = params.package_id.trim();
        if package_id.is_empty() {
            return Err(AppError::Validation(
                "conversation adapter package id is required".to_string(),
            ));
        }
        let package = self
            .load_conversation_adapter_package(package_id)?
            .ok_or_else(|| format!("conversation adapter package not found: {package_id}"))?;

        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "uninstalled": false,
                "package": package,
                "preflight": preflight
            }));
        }

        let package_identity = crate::backend::extension_kernel::PackageIdentity {
            kind: crate::backend::extension_kernel::PackageKind::ConversationAdapter,
            package_id: package.package_id.clone(),
            version: semver::Version::parse(&package.version)
                .map_err(|error| format!("invalid installed package version: {error}"))?,
        };
        crate::backend::conversations::ConversationAdapterPackageSystem
            .on_removed(&package_identity)
            .map_err(|error| error.to_string())?;

        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let package_id = package.package_id.clone();
        let adapter_id = package.adapter_id.clone();
        let uninstalled = self.db.block_on(async move {
            crate::backend::store::deactivate_conversation_adapter_package_sqlx(
                &pool,
                &tenant_id,
                &package_id,
                &adapter_id,
            )
            .await
        })?;
        self.runtime.refresh_conversation_adapter_catalog()?;
        Ok(json!({
            "dry_run": false,
            "uninstalled": true,
            "package": uninstalled,
            "preserved_managed_paths": preflight.managed_paths
        }))
    }

    pub(crate) fn install_conversation_script(
        &self,
        params: ConversationScriptInstallParams,
    ) -> AppResult<Value> {
        self.install_conversation_adapter_package(ConversationAdapterPackageInstallParams {
            catalog_url: params.catalog_url,
            package_id: params.item_id,
            version: None,
            dry_run: params.dry_run,
            yes: params.yes,
        })
    }

    pub(crate) fn load_conversation_adapter_packages(
        &self,
    ) -> AppResult<Vec<ConversationAdapterPackage>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db
            .block_on(async move {
                crate::backend::store::list_conversation_adapter_packages_sqlx(&pool, &tenant_id)
                    .await
            })
            .map_err(AppError::Storage)
    }

    pub(crate) fn load_conversation_adapter_package(
        &self,
        package_id: &str,
    ) -> AppResult<Option<ConversationAdapterPackage>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let package_id = package_id.to_string();
        self.db
            .block_on(async move {
                crate::backend::store::load_conversation_adapter_package_sqlx(
                    &pool,
                    &tenant_id,
                    &package_id,
                )
                .await
            })
            .map_err(AppError::Storage)
    }

    pub(crate) fn load_conversation_adapter_package_versions(
        &self,
        package_id: &str,
    ) -> AppResult<Vec<crate::backend::models::ConversationAdapterPackageVersion>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let package_id = package_id.to_string();
        self.db
            .block_on(async move {
                crate::backend::store::list_conversation_adapter_package_versions_sqlx(
                    &pool,
                    &tenant_id,
                    &package_id,
                )
                .await
            })
            .map_err(AppError::Storage)
    }

    pub(crate) fn list_installed_conversation_adapter_package_versions(
        &self,
        params: ConversationAdapterPackageVersionChangeParams,
    ) -> AppResult<Vec<crate::backend::models::ConversationAdapterPackageVersion>> {
        self.load_conversation_adapter_package_versions(params.package_id.trim())
    }

    pub(crate) fn switch_conversation_adapter_package_version(
        &self,
        params: ConversationAdapterPackageVersionChangeParams,
    ) -> AppResult<Value> {
        let version = params
            .version
            .as_deref()
            .and_then(clean_non_empty_string)
            .ok_or_else(|| "conversation adapter package version is required".to_string())?;
        self.activate_installed_conversation_adapter_package_version(
            &params.package_id,
            &version,
            ConversationAdapterPackageChangeAction::SwitchVersion,
            params.dry_run,
            params.yes,
        )
    }

    pub(crate) fn rollback_conversation_adapter_package_version(
        &self,
        params: ConversationAdapterPackageVersionChangeParams,
    ) -> AppResult<Value> {
        let package = self
            .load_conversation_adapter_package(params.package_id.trim())?
            .ok_or_else(|| "conversation adapter package not found".to_string())?;
        let versions = self.load_conversation_adapter_package_versions(&package.package_id)?;
        let target = select_rollback_version(&versions, &package.version)
            .ok_or_else(|| "no inactive installed version is available for rollback".to_string())?;
        self.activate_installed_conversation_adapter_package_version(
            &package.package_id,
            &target.version,
            ConversationAdapterPackageChangeAction::Rollback,
            params.dry_run,
            params.yes,
        )
    }

    pub(crate) fn delete_conversation_adapter_package_version(
        &self,
        params: ConversationAdapterPackageVersionChangeParams,
    ) -> AppResult<Value> {
        let package_id = params.package_id.trim();
        let version = params
            .version
            .as_deref()
            .and_then(clean_non_empty_string)
            .ok_or_else(|| "conversation adapter package version is required".to_string())?;
        let package = self
            .load_conversation_adapter_package(package_id)?
            .ok_or_else(|| format!("conversation adapter package not found: {package_id}"))?;
        if package.origin != ConversationAdapterPackageOrigin::ManagedRelease {
            return Err(AppError::Validation(
                "only managed package versions can be deleted".to_string(),
            ));
        }
        let runtime_registered = self
            .list_conversation_adapters()?
            .iter()
            .any(|adapter| adapter.id == package.adapter_id);
        if package.version == version && runtime_registered {
            return Err(AppError::Validation(
                "active conversation adapter package version must be uninstalled or switched before deletion"
                    .to_string(),
            ));
        }
        let versions = self.load_conversation_adapter_package_versions(package_id)?;
        let target = versions
            .iter()
            .find(|candidate| candidate.version == version)
            .cloned()
            .ok_or_else(|| {
                format!("installed package version not found: {package_id}@{version}")
            })?;
        let remaining_versions = versions
            .iter()
            .filter(|candidate| candidate.version != version)
            .cloned()
            .collect::<Vec<_>>();
        let replacement_package = if package.version == version && !runtime_registered {
            remaining_versions
                .first()
                .map(|replacement| package_for_uninstalled_replacement(&package, replacement))
        } else {
            None
        };
        let delete_package =
            package.version == version && !runtime_registered && remaining_versions.is_empty();
        let managed_root = crate::backend::app_settings::conversation_adapter_dir()?;
        let target_install_dir = crate::backend::path_utils::expand_path(&target.install_dir)?;
        let version_dir = validate_managed_package_version_delete_target(
            &managed_root,
            package_id,
            &version,
            &target_install_dir,
        )?;
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "package_id": package_id,
                "version": version,
                "managed_path": version_dir,
                "delete_package_record": delete_package,
                "replacement_version": replacement_package.as_ref().map(|package| package.version.clone())
            }));
        }
        if !params.yes {
            return Err(AppError::Validation(
                "conversation adapter package version deletion requires --yes".to_string(),
            ));
        }
        let staged = version_dir.with_file_name(format!(".{}-delete-{}", version, short_uuid()));
        fs::rename(&version_dir, &staged).map_err(|error| error.to_string())?;
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let package_id_owned = package_id.to_string();
        let version_owned = version.clone();
        let replacement_package_owned = replacement_package.clone();
        let deleted = self.db.block_on(async move {
            crate::backend::store::delete_conversation_adapter_package_version_sqlx(
                &pool,
                &tenant_id,
                &package_id_owned,
                &version_owned,
                replacement_package_owned.as_ref(),
                delete_package,
            )
            .await
        });
        match deleted {
            Ok(true) => fs::remove_dir_all(&staged).map_err(|error| error.to_string())?,
            Ok(false) => {
                let _ = fs::rename(&staged, &version_dir);
                return Err(AppError::Validation(
                    "installed package version record was not found".to_string(),
                ));
            }
            Err(error) => {
                let _ = fs::rename(&staged, &version_dir);
                return Err(AppError::Storage(error));
            }
        }
        self.runtime.refresh_conversation_adapter_catalog()?;
        Ok(json!({
            "dry_run": false,
            "deleted": true,
            "package_id": package_id,
            "version": version,
            "package_removed": delete_package,
            "replacement_version": replacement_package.map(|package| package.version)
        }))
    }

    fn activate_installed_conversation_adapter_package_version(
        &self,
        package_id: &str,
        version: &str,
        action: ConversationAdapterPackageChangeAction,
        dry_run: bool,
        yes: bool,
    ) -> AppResult<Value> {
        let preflight = self.prepare_conversation_adapter_package_change(
            ConversationAdapterPackageChangeParams {
                action,
                package_id: Some(package_id.to_string()),
                adapter_id: None,
            },
        )?;
        reject_conversation_package_task_conflicts(&preflight)?;
        if !dry_run && !yes {
            return Err(AppError::Validation(
                "conversation adapter package version activation requires --yes".to_string(),
            ));
        }
        let mut package = self
            .load_conversation_adapter_package(package_id)?
            .ok_or_else(|| format!("conversation adapter package not found: {package_id}"))?;
        if package.origin != ConversationAdapterPackageOrigin::ManagedRelease {
            return Err(AppError::Validation(
                "only managed package versions can be activated".to_string(),
            ));
        }
        let versions = self.load_conversation_adapter_package_versions(package_id)?;
        let target = versions
            .iter()
            .find(|candidate| candidate.version == version)
            .ok_or_else(|| {
                format!("installed package version not found: {package_id}@{version}")
            })?;
        let target_install_dir = crate::backend::path_utils::expand_path(&target.install_dir)?;
        let validation = crate::backend::conversations::validate_conversation_adapter_package_dir(
            &target_install_dir,
        )?;
        if validation.manifest.package_id != package_id
            || validation.manifest.version != version
            || validation.content_hash != target.content_hash
        {
            return Err(AppError::Validation(
                "installed conversation adapter package version failed immutable validation"
                    .to_string(),
            ));
        }
        if dry_run {
            return Ok(
                json!({"dry_run": true, "package_id": package_id, "version": version, "install_path": target.install_dir}),
            );
        }
        let preview = crate::backend::conversations::register_external_adapter(
            crate::backend::conversations::ExternalAdapterRegisterParams {
                manifest_path: validation.adapter_manifest_path.clone(),
                dry_run: false,
                yes: true,
            },
        )?;
        let adapter = crate::backend::conversations::adapter_from_registration_preview(preview)?;
        let now = Utc::now().to_rfc3339();
        package.version = version.to_string();
        package.install_dir = target.install_dir.clone();
        package.manifest_path = validation.manifest_path.clone();
        package.adapter_manifest_path = validation.adapter_manifest_path.clone();
        package.runtime_ready = true;
        package.runtime_gate_status = ConversationAdapterRuntimeGateStatus::Ready;
        package.runtime_validated_at = Some(now.clone());
        package.installed_content_hash = Some(validation.content_hash.clone());
        package.trusted_package_hash = Some(target.content_hash.clone());
        package.error_message = None;
        package.updated_at = now;
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let version_record = target.clone();
        self.db.block_on(async move {
            crate::backend::store::activate_conversation_adapter_package_sqlx(
                &pool,
                &tenant_id,
                &adapter,
                &package,
                &version_record,
            )
            .await
        })?;
        self.runtime.refresh_conversation_adapter_catalog()?;
        Ok(
            json!({"dry_run": false, "activated": true, "package_id": package_id, "version": version}),
        )
    }

    pub(crate) fn load_conversation_adapter_package_by_adapter(
        &self,
        adapter_id: &str,
    ) -> AppResult<Option<ConversationAdapterPackage>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let adapter_id = adapter_id.to_string();
        self.db
            .block_on(async move {
                crate::backend::store::load_conversation_adapter_package_by_adapter_sqlx(
                    &pool,
                    &tenant_id,
                    &adapter_id,
                )
                .await
            })
            .map_err(AppError::Storage)
    }

    pub(crate) fn save_conversation_adapter_package(
        &self,
        package: &ConversationAdapterPackage,
    ) -> AppResult<()> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let package = package.clone();
        self.db
            .block_on(async move {
                crate::backend::store::upsert_conversation_adapter_package_sqlx(
                    &pool, &tenant_id, &package,
                )
                .await
            })
            .map_err(AppError::Storage)
    }

    pub(crate) fn ensure_conversation_adapter_package_runtime_ready(
        &self,
        adapter: &ConversationAdapter,
    ) -> AppResult<()> {
        let Some(mut package) = self.load_conversation_adapter_package_by_adapter(&adapter.id)?
        else {
            return Ok(());
        };
        self.refresh_conversation_adapter_package_runtime(&mut package, Some(adapter))?;
        if package.runtime_ready {
            Ok(())
        } else {
            Err(AppError::External(format_package_not_ready_error(&package)))
        }
    }

    fn refresh_conversation_adapter_package_runtime(
        &self,
        package: &mut ConversationAdapterPackage,
        adapter: Option<&ConversationAdapter>,
    ) -> AppResult<()> {
        let install_dir = crate::backend::path_utils::expand_path(&package.install_dir)?;
        let evaluated = crate::backend::conversations::validate_conversation_adapter_package_dir(
            &install_dir,
        )
        .and_then(|validation| {
            if validation.manifest.package_id != package.package_id {
                return Err(format!(
                    "conversation adapter package manifest id {} does not match registered package {}",
                    validation.manifest.package_id, package.package_id
                ));
            }
            if validation.manifest.version != package.version {
                return Err(format!(
                    "conversation adapter package manifest version {} does not match active version {}",
                    validation.manifest.version, package.version
                ));
            }
            let adapter = adapter.ok_or_else(|| {
                format!(
                    "conversation adapter runtime is not registered: {}",
                    package.adapter_id
                )
            })?;
            if validation.adapter_validation.manifest.id != adapter.id {
                return Err(format!(
                    "conversation adapter package {} manifest adapter id {} does not match registered adapter {}",
                    package.package_id, validation.adapter_validation.manifest.id, adapter.id
                ));
            }
            if package.origin != ConversationAdapterPackageOrigin::DevOverride {
                let trusted_hash = package
                    .trusted_package_hash
                    .as_deref()
                    .or(package.installed_content_hash.as_deref())
                    .ok_or_else(|| {
                        format!(
                            "conversation adapter package has no trusted hash: {}",
                            package.package_id
                        )
                    })?;
                if validation.content_hash != trusted_hash {
                    return Err(format!(
                        "conversation adapter package content hash mismatch: {}",
                        package.package_id
                    ));
                }
            }
            Ok(validation.content_hash)
        });

        let now = Utc::now().to_rfc3339();
        match evaluated {
            Ok(content_hash) => {
                package.runtime_ready = true;
                package.runtime_gate_status = ConversationAdapterRuntimeGateStatus::Ready;
                package.installed_content_hash = Some(content_hash);
                package.error_message = None;
            }
            Err(error) => {
                package.runtime_ready = false;
                package.runtime_gate_status = classify_runtime_gate_error(&install_dir, &error);
                package.error_message = Some(error);
            }
        }
        package.runtime_validated_at = Some(now.clone());
        package.updated_at = now;
        self.save_conversation_adapter_package(package)
    }
}

fn find_developer_conversation_adapter_root(start: &Path) -> AppResult<PathBuf> {
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

fn discover_conversation_adapter_workspace_dirs(root: &Path) -> AppResult<Vec<PathBuf>> {
    if !root.is_dir() {
        return Err(AppError::Validation(format!(
            "conversation adapter workspace root is not a directory: {}",
            root.display()
        )));
    }
    let mut package_dirs = fs::read_dir(root)
        .map_err(|error| error.to_string())?
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

fn promote_conversation_adapter_workspace_package(
    service: &AppService,
    package_dir: &Path,
    managed_root: &Path,
    dry_run: bool,
) -> AppResult<Value> {
    let source_dir = package_dir
        .canonicalize()
        .map_err(|error| format!("resolve adapter workspace failed: {error}"))?;
    let source_validation =
        crate::backend::conversations::validate_conversation_adapter_package_dir(&source_dir)?;
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
        .map_err(|error| format!("conversation adapter package version must be SemVer: {error}"))?;
    if let Some(active_package) =
        service.load_conversation_adapter_package_by_adapter(adapter_id)?
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
    let preflight = service.prepare_conversation_adapter_package_change(
        ConversationAdapterPackageChangeParams {
            action: ConversationAdapterPackageChangeAction::Register,
            package_id: None,
            adapter_id: Some(adapter_id.to_string()),
        },
    )?;
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

    let prepared_dir = package_root.join("prepared").join(short_uuid());
    if let Some(parent) = prepared_dir.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    capabilities::copy_dir(&source_dir, &prepared_dir)?;
    let promotion = (|| {
        let prepared_validation =
            crate::backend::conversations::validate_conversation_adapter_package_dir(
                &prepared_dir,
            )?;
        if prepared_validation.content_hash != source_validation.content_hash {
            return Err(AppError::Validation(
                "conversation adapter workspace changed while its runtime snapshot was being created"
                    .to_string(),
            ));
        }
        crate::backend::conversations::register_external_adapter(
            crate::backend::conversations::ExternalAdapterRegisterParams {
                manifest_path: prepared_validation.adapter_manifest_path,
                dry_run: false,
                yes: true,
            },
        )?;

        let created_version_dir = if version_dir.exists() {
            let existing =
                crate::backend::conversations::validate_conversation_adapter_package_dir(
                    &version_dir,
                )?;
            if existing.content_hash != source_validation.content_hash {
                return Err(AppError::Validation(format!(
                    "conversation adapter runtime revision is immutable: {}",
                    version_dir.display()
                )));
            }
            fs::remove_dir_all(&prepared_dir).map_err(|error| error.to_string())?;
            false
        } else {
            let parent = version_dir.parent().ok_or_else(|| {
                "conversation adapter runtime has no versions directory".to_string()
            })?;
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            fs::rename(&prepared_dir, &version_dir).map_err(|error| error.to_string())?;
            true
        };

        let final_validation =
            crate::backend::conversations::validate_conversation_adapter_package_dir(&version_dir)?;
        let preview = crate::backend::conversations::register_external_adapter(
            crate::backend::conversations::ExternalAdapterRegisterParams {
                manifest_path: final_validation.adapter_manifest_path.clone(),
                dry_run: true,
                yes: true,
            },
        )?;
        let adapter = crate::backend::conversations::adapter_from_registration_preview(preview)?;
        let previous_package =
            service.load_conversation_adapter_package(&final_validation.manifest.package_id)?;
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
        let pool = service.db.pool().clone();
        let tenant_id = service.tenant_id().to_string();
        let adapter_to_save = adapter.clone();
        let package_to_save = package.clone();
        let activation = service.db.block_on(async move {
            crate::backend::store::activate_conversation_adapter_workspace_sqlx(
                &pool,
                &tenant_id,
                &adapter_to_save,
                &package_to_save,
            )
            .await
        });
        if let Err(error) = activation {
            if created_version_dir {
                let _ = fs::remove_dir_all(&version_dir);
            }
            return Err(AppError::Storage(error));
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
    })();
    if promotion.is_err() {
        let _ = fs::remove_dir_all(&prepared_dir);
    }
    promotion
}

fn retain_only_active_workspace_runtime(package_root: &Path, active_dir: &Path) -> AppResult<()> {
    let versions_dir = package_root.join("versions");
    if !versions_dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(&versions_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path == active_dir {
            continue;
        }
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() {
            fs::remove_dir_all(path).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn discover_local_conversation_adapter_packages(
    adapter_root: &Path,
) -> AppResult<Vec<ConversationScriptCatalogItem>> {
    if !adapter_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut package_dirs = fs::read_dir(adapter_root)
        .map_err(|error| error.to_string())?
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
                crate::backend::conversations::validate_conversation_adapter_package_dir(
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

fn infer_unmanaged_adapter_origin(
    adapter: Option<&ConversationAdapter>,
) -> ConversationAdapterPackageOrigin {
    match adapter.map(|adapter| adapter.trust_state) {
        Some(crate::backend::models::ConversationAdapterTrustState::BuiltIn) => {
            ConversationAdapterPackageOrigin::BuiltIn
        }
        _ => ConversationAdapterPackageOrigin::LegacyExternal,
    }
}

fn reject_conversation_package_task_conflicts(
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

fn select_rollback_version<'a>(
    versions: &'a [crate::backend::models::ConversationAdapterPackageVersion],
    active_version: &str,
) -> Option<&'a crate::backend::models::ConversationAdapterPackageVersion> {
    versions
        .iter()
        .filter(|version| version.version != active_version)
        .max_by(|left, right| {
            left.installed_at
                .cmp(&right.installed_at)
                .then_with(|| left.version.cmp(&right.version))
        })
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageInspection {
    pub(crate) origin: ConversationAdapterPackageOrigin,
    pub(crate) package: Option<ConversationAdapterPackage>,
    pub(crate) adapter: Option<ConversationAdapter>,
    pub(crate) affected_sources: Vec<ConversationSource>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageChangePreflight {
    pub(crate) action: ConversationAdapterPackageChangeAction,
    pub(crate) origin: ConversationAdapterPackageOrigin,
    pub(crate) package_id: Option<String>,
    pub(crate) adapter_id: Option<String>,
    pub(crate) managed_paths: Vec<String>,
    pub(crate) affected_sources: Vec<ConversationSource>,
    pub(crate) task_conflicts: Vec<String>,
    pub(crate) preserves_conversation_records: bool,
    pub(crate) risk: ConversationAdapterPackageChangeRisk,
    pub(crate) confirmation_required: bool,
}

fn classify_runtime_gate_error(
    _install_dir: &Path,
    error: &str,
) -> ConversationAdapterRuntimeGateStatus {
    if error.contains("hash mismatch") || error.contains("no trusted hash") {
        ConversationAdapterRuntimeGateStatus::HashMismatch
    } else if error.contains("requires AssetIWeave core") {
        ConversationAdapterRuntimeGateStatus::CoreIncompatible
    } else if error.contains("root is not a directory")
        || error.contains("runtime is not registered")
    {
        ConversationAdapterRuntimeGateStatus::RuntimeMissing
    } else {
        ConversationAdapterRuntimeGateStatus::ManifestInvalid
    }
}

fn validate_managed_package_delete_target(
    managed_root: &Path,
    package_id: &str,
    install_dir: &Path,
) -> AppResult<PathBuf> {
    let package_id = package_id.trim();
    let package_id_path = Path::new(package_id);
    if package_id.is_empty()
        || package_id == "."
        || package_id == ".."
        || package_id_path.components().count() != 1
        || !matches!(
            package_id_path.components().next(),
            Some(std::path::Component::Normal(_))
        )
    {
        return Err(AppError::Validation(format!(
            "conversation adapter package id is not a safe path segment: {package_id}"
        )));
    }

    let packages_root = managed_root.join("packages");
    if !install_dir.exists() || !install_dir.starts_with(&packages_root) {
        return Err(AppError::Validation(format!(
            "conversation adapter package delete target does not exist in the managed library: {}",
            install_dir.display()
        )));
    }

    let canonical_packages_root = packages_root.canonicalize().map_err(|error| {
        format!(
            "resolve managed conversation adapter packages root failed ({}): {error}",
            packages_root.display()
        )
    })?;
    let canonical_install_dir = install_dir.canonicalize().map_err(|error| {
        format!(
            "resolve conversation adapter package install directory failed ({}): {error}",
            install_dir.display()
        )
    })?;
    let relative_install = canonical_install_dir
        .strip_prefix(&canonical_packages_root)
        .map_err(|_| {
            format!(
                "conversation adapter package install directory escapes the managed library: {}",
                install_dir.display()
            )
        })?;
    let package_segment = relative_install
        .components()
        .next()
        .and_then(|component| match component {
            std::path::Component::Normal(value) => Some(value),
            _ => None,
        })
        .ok_or_else(|| {
            "conversation adapter package install directory has no package root".to_string()
        })?;
    let package_root = packages_root.join(package_segment);
    let canonical_package_root = package_root.canonicalize().map_err(|error| {
        format!(
            "resolve managed conversation adapter package root failed ({}): {error}",
            package_root.display()
        )
    })?;
    if canonical_package_root.parent() != Some(canonical_packages_root.as_path())
        || canonical_install_dir == canonical_package_root
        || !canonical_install_dir.starts_with(&canonical_package_root)
    {
        return Err(AppError::Validation(format!(
            "conversation adapter package root escapes the managed library: {}",
            package_root.display()
        )));
    }

    Ok(package_root)
}

fn validate_managed_package_version_delete_target(
    managed_root: &Path,
    package_id: &str,
    version: &str,
    install_dir: &Path,
) -> AppResult<PathBuf> {
    let version = validated_package_version(version)?;
    let package_root =
        validate_managed_package_delete_target(managed_root, package_id, install_dir)?;
    let expected = package_root.join("versions").join(version);
    let canonical_expected = expected
        .canonicalize()
        .map_err(|error| format!("resolve managed package version directory failed: {error}"))?;
    let canonical_install = install_dir
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if canonical_install != canonical_expected {
        return Err(AppError::Validation("conversation adapter version delete target is not the requested managed version directory".to_string()));
    }
    Ok(canonical_install)
}

pub(super) fn install_conversation_adapter_package_from_item(
    service: &AppService,
    item: &ConversationScriptCatalogItem,
    dry_run: bool,
    catalog_url: Option<&str>,
) -> AppResult<Value> {
    let spec = item.to_install_spec();
    super::conversation_adapter_installer::install_conversation_adapter_package_from_spec(
        service,
        &spec,
        dry_run,
        catalog_url,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ConversationScriptCatalog {
    #[serde(alias = "schemaVersion")]
    pub(crate) schema_version: u32,
    #[serde(default, alias = "updatedAt")]
    pub(crate) updated_at: Option<String>,
    #[serde(default)]
    pub(crate) items: Vec<ConversationScriptCatalogItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ConversationScriptCatalogItem {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    #[serde(alias = "recordKind")]
    pub(crate) record_kind: ConversationScriptRecordKind,
    #[serde(default)]
    pub(crate) provider: Option<String>,
    #[serde(default, alias = "adapterId")]
    pub(crate) adapter_id: Option<String>,
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default, alias = "homepageUrl")]
    pub(crate) homepage_url: Option<String>,
    #[serde(default, alias = "repositoryUrl")]
    pub(crate) repository_url: Option<String>,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    #[serde(default, alias = "manifestFile")]
    pub(crate) manifest_file: Option<String>,
    #[serde(default, alias = "packageManifestFile")]
    pub(crate) package_manifest_file: Option<String>,
    #[serde(default, alias = "expectedContentHash")]
    pub(crate) expected_content_hash: Option<String>,
    #[serde(default, alias = "expectedPackageHash")]
    pub(crate) expected_package_hash: Option<String>,
    #[serde(default, alias = "expectedArtifactHash")]
    pub(crate) expected_artifact_hash: Option<String>,
    #[serde(default, alias = "artifactSize")]
    pub(crate) artifact_size: Option<u64>,
    pub(crate) source: ConversationScriptCatalogSource,
}

impl ConversationScriptCatalogItem {
    fn to_install_spec(&self) -> ConversationAdapterPackageInstallSpec {
        ConversationAdapterPackageInstallSpec {
            id: self.id.clone(),
            name: self.name.clone(),
            version: self.version.clone(),
            record_kind: self.record_kind.as_package_record_kind(),
            provider: self.provider.clone(),
            adapter_id: self.adapter_id.clone(),
            description: self.description.clone(),
            homepage_url: self.homepage_url.clone(),
            repository_url: self.repository_url.clone(),
            tags: self.tags.clone(),
            manifest_file: self.manifest_file.clone(),
            package_manifest_file: self.package_manifest_file.clone(),
            expected_content_hash: self.expected_content_hash.clone(),
            expected_package_hash: self.expected_package_hash.clone(),
            expected_artifact_hash: self.expected_artifact_hash.clone(),
            artifact_size: self.artifact_size,
            source: crate::backend::conversations::ConversationAdapterPackageInstallSource {
                kind: match self.source.kind {
                    ConversationScriptCatalogSourceKind::Github => {
                        ConversationAdapterPackageInstallSourceKind::Github
                    }
                    ConversationScriptCatalogSourceKind::ArtifactZip => {
                        ConversationAdapterPackageInstallSourceKind::ArtifactZip
                    }
                    ConversationScriptCatalogSourceKind::LocalDirectory => {
                        ConversationAdapterPackageInstallSourceKind::LocalDirectory
                    }
                },
                url: self.source.url.clone(),
                branch: self.source.branch.clone(),
                path: self.source.path.clone(),
            },
        }
    }

    fn package_id(&self) -> &str {
        self.id.as_str()
    }

    fn adapter_key(&self) -> &str {
        self.adapter_id.as_deref().unwrap_or(self.id.as_str())
    }

    fn manifest_file_name(&self) -> AppResult<String> {
        let value = self
            .manifest_file
            .as_deref()
            .unwrap_or("conversation-adapter.json");
        clean_relative_file_name(value)
    }

    fn package_manifest_file_name(&self) -> AppResult<String> {
        let value = self
            .package_manifest_file
            .as_deref()
            .unwrap_or("conversation-adapter-package.json");
        clean_relative_file_name(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ConversationScriptCatalogSource {
    #[serde(rename = "type")]
    pub(crate) kind: ConversationScriptCatalogSourceKind,
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) branch: Option<String>,
    #[serde(default)]
    pub(crate) path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConversationScriptCatalogSourceKind {
    Github,
    ArtifactZip,
    LocalDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConversationScriptRecordKind {
    Session,
    Web,
}

impl ConversationScriptRecordKind {
    fn as_package_record_kind(self) -> ConversationAdapterPackageRecordKind {
        match self {
            Self::Session => ConversationAdapterPackageRecordKind::Session,
            Self::Web => ConversationAdapterPackageRecordKind::Web,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageCatalogEntry {
    pub(crate) item: ConversationScriptCatalogItem,
    pub(crate) installed: bool,
    pub(crate) update_available: bool,
    pub(crate) ahead_of_release: bool,
    pub(crate) runtime_ready: bool,
    pub(crate) status: String,
    pub(crate) installed_package: Option<ConversationAdapterPackage>,
    pub(crate) installed_adapter: Option<ConversationAdapter>,
    pub(crate) install_path: Option<String>,
    pub(crate) display_install_path: Option<String>,
    pub(crate) display_manifest_path: Option<String>,
    pub(crate) error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConversationScriptCatalogEntry {
    pub(crate) item: ConversationScriptCatalogItem,
    pub(crate) installed: bool,
    pub(crate) update_available: bool,
    pub(crate) installed_adapter: Option<ConversationAdapter>,
    pub(crate) install_path: Option<String>,
}

impl From<ConversationAdapterPackageCatalogEntry> for ConversationScriptCatalogEntry {
    fn from(entry: ConversationAdapterPackageCatalogEntry) -> Self {
        Self {
            item: entry.item,
            installed: entry.installed,
            update_available: entry.update_available,
            installed_adapter: entry.installed_adapter,
            install_path: entry.install_path,
        }
    }
}

#[derive(Debug)]
struct GitHubCatalogLocation {
    repo_url: String,
    branch: Option<String>,
    path: Option<String>,
}

fn load_conversation_script_catalog(
    catalog_url: Option<&str>,
) -> AppResult<ConversationScriptCatalog> {
    let catalog_url = catalog_url
        .and_then(clean_non_empty_string)
        .unwrap_or_else(|| DEFAULT_CONVERSATION_SCRIPT_CATALOG_URL.to_string());
    let text = if catalog_url.starts_with("https://") || catalog_url.starts_with("http://") {
        match fetch_catalog_text(&catalog_url) {
            Ok(text) => text,
            Err(error) if catalog_url == DEFAULT_CONVERSATION_SCRIPT_CATALOG_URL => {
                read_local_default_catalog().map_err(|fallback_error| {
                    format!("{error}; local default catalog fallback failed: {fallback_error}")
                })?
            }
            Err(error) => return Err(error),
        }
    } else {
        let path = crate::backend::path_utils::expand_path(&catalog_url)?;
        fs::read_to_string(&path)
            .map_err(|error| format!("read conversation adapter package catalog failed: {error}"))?
    };
    let catalog: ConversationScriptCatalog = serde_json::from_str(&text).map_err(|error| {
        format!("conversation adapter package catalog was not valid JSON: {error}")
    })?;
    validate_conversation_script_catalog(&catalog)?;
    Ok(catalog)
}

fn fetch_catalog_text(url: &str) -> AppResult<String> {
    let response = ureq::get(url)
        .set(
            "User-Agent",
            "AssetIWeave/0.5 conversation-adapter-package-catalog",
        )
        .call()
        .map_err(|error| format!("conversation adapter package catalog request failed: {error}"))?;
    response.into_string().map_err(|error| {
        AppError::External(format!(
            "conversation adapter package catalog response was not text: {error}"
        ))
    })
}

fn read_local_default_catalog() -> AppResult<String> {
    Ok(LOCAL_DEFAULT_CONVERSATION_SCRIPT_CATALOG.to_string())
}

fn validate_conversation_script_catalog(catalog: &ConversationScriptCatalog) -> AppResult<()> {
    if catalog.schema_version != 1 {
        return Err(AppError::Validation(
            "conversation adapter package catalog schema_version must be 1".to_string(),
        ));
    }
    let mut seen_ids = HashSet::new();
    for item in &catalog.items {
        validate_conversation_script_catalog_item(item)?;
        if !seen_ids.insert(item.id.clone()) {
            return Err(AppError::Validation(format!(
                "duplicate conversation adapter package catalog item: {}",
                item.id
            )));
        }
    }
    Ok(())
}

fn validate_conversation_script_catalog_item(
    item: &ConversationScriptCatalogItem,
) -> AppResult<()> {
    if item.id.trim().is_empty() {
        return Err(AppError::Validation(
            "conversation adapter package catalog item id is required".to_string(),
        ));
    }
    let id_path = Path::new(&item.id);
    if item.id == "."
        || item.id == ".."
        || id_path.components().count() != 1
        || !item.id.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(AppError::Validation(format!(
            "conversation adapter package catalog item id must be a safe path segment: {}",
            item.id
        )));
    }
    if item.name.trim().is_empty() {
        return Err(AppError::Validation(format!(
            "conversation adapter package catalog item name is required: {}",
            item.id
        )));
    }
    if item.version.trim().is_empty() {
        return Err(AppError::Validation(format!(
            "conversation adapter package catalog item version is required: {}",
            item.id
        )));
    }
    validated_package_version(&item.version)?;
    if let Some(adapter_id) = item.adapter_id.as_deref() {
        if adapter_id.trim().is_empty() {
            return Err(AppError::Validation(format!(
                "conversation adapter package catalog item adapter_id must not be empty: {}",
                item.id
            )));
        }
    }
    item.manifest_file_name()?;
    item.package_manifest_file_name()?;
    parse_github_catalog_location(&item.source)?;
    Ok(())
}

fn resolve_conversation_adapter_package_catalog_entries(
    items: Vec<ConversationScriptCatalogItem>,
    adapters: &[ConversationAdapter],
    packages: &[ConversationAdapterPackage],
) -> Vec<ConversationAdapterPackageCatalogEntry> {
    let mut entries = items
        .into_iter()
        .map(|item| {
            let installed_package = packages
                .iter()
                .find(|package| package.package_id == item.package_id())
                .cloned();
            let installed_adapter = adapters
                .iter()
                .find(|adapter| adapter.id == item.adapter_key())
                .cloned();
            let install_path = installed_package
                .as_ref()
                .map(|package| package.install_dir.clone())
                .or_else(|| {
                    installed_adapter
                        .as_ref()
                        .and_then(|adapter| adapter.manifest_path.as_deref())
                        .and_then(|path| {
                            Path::new(path).parent().map(|parent| parent.to_path_buf())
                        })
                        .map(|path| path.to_string_lossy().to_string())
                })
                .or_else(|| {
                    (item.source.kind == ConversationScriptCatalogSourceKind::LocalDirectory)
                        .then(|| item.source.url.clone())
                });
            let installed = installed_package.is_some() || installed_adapter.is_some();
            let installed_version = installed_package
                .as_ref()
                .map(|p| &p.version)
                .or_else(|| installed_adapter.as_ref().map(|a| &a.version));

            let (update_available, ahead_of_release) =
                if let Some(installed_ver) = installed_version {
                    if let (Ok(installed_semver), Ok(item_semver)) = (
                        semver::Version::parse(installed_ver),
                        semver::Version::parse(&item.version),
                    ) {
                        (
                            item_semver > installed_semver,
                            installed_semver > item_semver,
                        )
                    } else {
                        (installed_ver != &item.version, false)
                    }
                } else {
                    (false, false)
                };
            let runtime_ready = installed_package
                .as_ref()
                .map(|package| package.runtime_ready)
                .unwrap_or_else(|| {
                    installed_adapter
                        .as_ref()
                        .is_some_and(|adapter| adapter.enabled)
                });
            let error_message = installed_package
                .as_ref()
                .and_then(|package| package.error_message.clone());
            let status = conversation_adapter_package_status(
                installed,
                installed_package.as_ref(),
                update_available,
                ahead_of_release,
                runtime_ready,
                installed_adapter.as_ref(),
            );
            let display_install_path = install_path
                .as_deref()
                .map(crate::backend::path_utils::display_path_or_original);
            let display_manifest_path = conversation_catalog_manifest_path(
                installed_package.as_ref(),
                installed_adapter.as_ref(),
                install_path.as_deref(),
                item.manifest_file.as_deref(),
            )
            .as_deref()
            .map(crate::backend::path_utils::display_path_or_original);
            ConversationAdapterPackageCatalogEntry {
                item,
                installed,
                update_available,
                ahead_of_release,
                runtime_ready,
                status,
                installed_package,
                installed_adapter,
                install_path,
                display_install_path,
                display_manifest_path,
                error_message,
            }
        })
        .collect::<Vec<_>>();
    let mut seen_packages = entries
        .iter()
        .filter_map(|entry| {
            entry
                .installed_package
                .as_ref()
                .map(|package| package.package_id.clone())
        })
        .collect::<HashSet<_>>();
    let mut seen_adapters = entries
        .iter()
        .filter_map(|entry| {
            entry
                .installed_adapter
                .as_ref()
                .map(|adapter| adapter.id.clone())
        })
        .collect::<HashSet<_>>();

    for package in packages {
        if !seen_packages.insert(package.package_id.clone()) {
            continue;
        }
        let adapter = adapters
            .iter()
            .find(|adapter| adapter.id == package.adapter_id)
            .cloned();
        if let Some(adapter) = adapter.as_ref() {
            seen_adapters.insert(adapter.id.clone());
        }
        let latest_version = package
            .latest_version
            .clone()
            .unwrap_or_else(|| package.version.clone());
        let (update_available, ahead_of_release) = semver::Version::parse(&latest_version)
            .ok()
            .zip(semver::Version::parse(&package.version).ok())
            .map(|(latest, current)| (latest > current, current > latest))
            .unwrap_or((false, false));
        let item = ConversationScriptCatalogItem {
            id: package.package_id.clone(),
            name: package.name.clone(),
            version: latest_version,
            record_kind: match package.record_kind {
                ConversationAdapterPackageRecordKind::Session => {
                    ConversationScriptRecordKind::Session
                }
                ConversationAdapterPackageRecordKind::Web => ConversationScriptRecordKind::Web,
            },
            provider: Some(package_origin_label(package.origin).to_string()),
            adapter_id: Some(package.adapter_id.clone()),
            description: None,
            homepage_url: None,
            repository_url: package.source_url.clone(),
            tags: Vec::new(),
            manifest_file: Some(
                Path::new(&package.adapter_manifest_path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("conversation-adapter.json")
                    .to_string(),
            ),
            package_manifest_file: Some(
                Path::new(&package.manifest_path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("conversation-adapter-package.json")
                    .to_string(),
            ),
            expected_content_hash: None,
            expected_package_hash: package.trusted_package_hash.clone(),
            expected_artifact_hash: None,
            artifact_size: None,
            source: ConversationScriptCatalogSource {
                kind: ConversationScriptCatalogSourceKind::LocalDirectory,
                url: package
                    .source_url
                    .clone()
                    .unwrap_or_else(|| package.install_dir.clone()),
                branch: package.git_ref.clone(),
                path: None,
            },
        };
        entries.push(ConversationAdapterPackageCatalogEntry {
            status: conversation_adapter_package_status(
                true,
                Some(package),
                update_available,
                ahead_of_release,
                package.runtime_ready,
                adapter.as_ref(),
            ),
            installed: true,
            update_available,
            ahead_of_release,
            runtime_ready: package.runtime_ready,
            install_path: Some(package.install_dir.clone()),
            display_install_path: Some(crate::backend::path_utils::display_path_or_original(
                &package.install_dir,
            )),
            display_manifest_path: Some(crate::backend::path_utils::display_path_or_original(
                &package.adapter_manifest_path,
            )),
            error_message: package.error_message.clone(),
            installed_package: Some(package.clone()),
            installed_adapter: adapter,
            item,
        });
    }

    for adapter in adapters {
        if !seen_adapters.insert(adapter.id.clone()) {
            continue;
        }
        let record_kind = if adapter
            .capabilities
            .iter()
            .any(|capability| capability == "web_records")
        {
            ConversationScriptRecordKind::Web
        } else {
            ConversationScriptRecordKind::Session
        };
        let install_path = adapter
            .manifest_path
            .as_deref()
            .and_then(|path| Path::new(path).parent())
            .map(|path| path.to_string_lossy().to_string());
        let display_install_path = install_path
            .as_deref()
            .map(crate::backend::path_utils::display_path_or_original);
        let display_manifest_path = adapter
            .manifest_path
            .as_deref()
            .map(crate::backend::path_utils::display_path_or_original);
        entries.push(ConversationAdapterPackageCatalogEntry {
            item: ConversationScriptCatalogItem {
                id: adapter.id.clone(),
                name: adapter.name.clone(),
                version: adapter.version.clone(),
                record_kind,
                provider: Some(
                    if adapter.trust_state
                        == crate::backend::models::ConversationAdapterTrustState::BuiltIn
                    {
                        "built_in".to_string()
                    } else {
                        "legacy_external".to_string()
                    },
                ),
                adapter_id: Some(adapter.id.clone()),
                description: None,
                homepage_url: None,
                repository_url: None,
                tags: Vec::new(),
                manifest_file: adapter
                    .manifest_path
                    .as_deref()
                    .and_then(|path| Path::new(path).file_name())
                    .and_then(|value| value.to_str())
                    .map(str::to_string),
                package_manifest_file: None,
                expected_content_hash: adapter.trusted_hash.clone(),
                expected_package_hash: None,
                expected_artifact_hash: None,
                artifact_size: None,
                source: ConversationScriptCatalogSource {
                    kind: ConversationScriptCatalogSourceKind::LocalDirectory,
                    url: install_path.clone().unwrap_or_default(),
                    branch: None,
                    path: None,
                },
            },
            installed: true,
            update_available: false,
            ahead_of_release: false,
            runtime_ready: adapter.enabled,
            status: conversation_adapter_package_status(
                true,
                None,
                false,
                false,
                adapter.enabled,
                Some(adapter),
            ),
            installed_package: None,
            installed_adapter: Some(adapter.clone()),
            install_path,
            display_install_path,
            display_manifest_path,
            error_message: None,
        });
    }
    entries.sort_by(|left, right| left.item.name.cmp(&right.item.name));
    entries
}

fn conversation_catalog_manifest_path(
    package: Option<&ConversationAdapterPackage>,
    adapter: Option<&ConversationAdapter>,
    install_path: Option<&str>,
    manifest_file: Option<&str>,
) -> Option<String> {
    package
        .map(|package| package.adapter_manifest_path.clone())
        .or_else(|| adapter.and_then(|adapter| adapter.manifest_path.clone()))
        .or_else(|| {
            install_path.map(|install_path| {
                format!(
                    "{}/{}",
                    install_path.trim_end_matches(['/', '\\']),
                    manifest_file
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or("conversation-adapter.json")
                )
            })
        })
}

fn package_origin_label(origin: ConversationAdapterPackageOrigin) -> &'static str {
    match origin {
        ConversationAdapterPackageOrigin::BuiltIn => "built_in",
        ConversationAdapterPackageOrigin::ManagedRelease => "managed_release",
        ConversationAdapterPackageOrigin::LocalDirectory => "local_directory",
        ConversationAdapterPackageOrigin::GitRef => "git_ref",
        ConversationAdapterPackageOrigin::LegacyExternal => "legacy_external",
        ConversationAdapterPackageOrigin::DevOverride => "dev_override",
    }
}

fn conversation_adapter_package_status(
    installed: bool,
    package: Option<&ConversationAdapterPackage>,
    update_available: bool,
    ahead_of_release: bool,
    runtime_ready: bool,
    adapter: Option<&ConversationAdapter>,
) -> String {
    if !installed {
        return "not_installed".to_string();
    }
    if let Some(package) = package {
        if package.origin == ConversationAdapterPackageOrigin::ManagedRelease && adapter.is_none() {
            return "uninstalled".to_string();
        }
        if !package.runtime_ready {
            return match package.runtime_gate_status {
                ConversationAdapterRuntimeGateStatus::RuntimeMissing => "runtime_missing",
                ConversationAdapterRuntimeGateStatus::HashMismatch => "hash_mismatch",
                ConversationAdapterRuntimeGateStatus::ManifestInvalid => "manifest_invalid",
                ConversationAdapterRuntimeGateStatus::CoreIncompatible => "core_incompatible",
                ConversationAdapterRuntimeGateStatus::Ready => "verification_failed",
            }
            .to_string();
        }
        match package.origin {
            ConversationAdapterPackageOrigin::LocalDirectory => {
                return "local_registered".to_string()
            }
            ConversationAdapterPackageOrigin::GitRef => return "git_registered".to_string(),
            ConversationAdapterPackageOrigin::DevOverride => return "dev_override".to_string(),
            ConversationAdapterPackageOrigin::BuiltIn => return "built_in".to_string(),
            ConversationAdapterPackageOrigin::ManagedRelease
            | ConversationAdapterPackageOrigin::LegacyExternal => {}
        }
    } else if adapter.is_some_and(|adapter| {
        adapter.trust_state == crate::backend::models::ConversationAdapterTrustState::BuiltIn
    }) {
        return if runtime_ready {
            "built_in"
        } else {
            "uninstalled"
        }
        .to_string();
    } else if adapter.is_some() {
        return "legacy_installed".to_string();
    }
    if update_available {
        return "update_available".to_string();
    }
    if ahead_of_release {
        return "ahead_of_release".to_string();
    }
    if runtime_ready {
        "installed".to_string()
    } else {
        "verification_failed".to_string()
    }
}

fn format_package_not_ready_error(package: &ConversationAdapterPackage) -> String {
    format!(
        "conversation adapter package runtime is not ready: {}{}",
        package.package_id,
        package
            .error_message
            .as_deref()
            .map(|message| format!(": {message}"))
            .unwrap_or_default()
    )
}

fn validated_package_version(value: &str) -> AppResult<String> {
    semver::Version::parse(value.trim())
        .map(|version| version.to_string())
        .map_err(|error| {
            AppError::Validation(format!(
                "conversation adapter package version must be SemVer: {error}"
            ))
        })
}

fn parse_github_catalog_location(
    source: &ConversationScriptCatalogSource,
) -> AppResult<GitHubCatalogLocation> {
    if source.kind != ConversationScriptCatalogSourceKind::Github {
        return Err(AppError::Validation(
            "conversation adapter package source must be github".to_string(),
        ));
    }
    let trimmed = source
        .url
        .trim()
        .split('#')
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    let path = trimmed.strip_prefix("https://github.com/").ok_or_else(|| {
        "conversation adapter package source only supports https://github.com URLs".to_string()
    })?;
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(AppError::Validation(
            "GitHub URL must include owner and repository".to_string(),
        ));
    }

    let owner = parts[0];
    let repo = parts[1].trim_end_matches(".git");
    if repo.is_empty() {
        return Err(AppError::Validation(
            "GitHub URL must include repository name".to_string(),
        ));
    }

    let mut branch = source.branch.as_deref().and_then(clean_non_empty_string);
    let mut source_path = source.path.as_deref().and_then(clean_catalog_subpath);
    if source_path.is_none() && parts.len() >= 4 && matches!(parts[2], "tree" | "blob") {
        branch = branch.or_else(|| clean_non_empty_string(parts[3]));
        if parts.len() > 4 {
            source_path = clean_catalog_subpath(&parts[4..].join("/"));
        }
    }

    Ok(GitHubCatalogLocation {
        repo_url: format!("https://github.com/{owner}/{repo}.git"),
        branch,
        path: source_path,
    })
}

fn clean_non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn clean_catalog_subpath(value: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in value.trim().trim_matches('/').split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part == ".git" || part.contains('\\') || part.contains(':') {
            return None;
        }
        parts.push(part);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn clean_relative_file_name(value: &str) -> AppResult<String> {
    let trimmed = value.trim().trim_matches('/');
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains(':')
    {
        return Err(AppError::Validation(format!(
            "manifest_file must be a file name: {value}"
        )));
    }
    Ok(trimmed.to_string())
}

fn short_uuid() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}

fn replacement_manifest_path(install_dir: &str, previous_path: &str, fallback: &str) -> String {
    let file_name = Path::new(previous_path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback);
    Path::new(install_dir)
        .join(file_name)
        .to_string_lossy()
        .to_string()
}

fn package_for_uninstalled_replacement(
    package: &ConversationAdapterPackage,
    replacement: &crate::backend::models::ConversationAdapterPackageVersion,
) -> ConversationAdapterPackage {
    let mut replacement_package = package.clone();
    replacement_package.version = replacement.version.clone();
    replacement_package.install_dir = replacement.install_dir.clone();
    replacement_package.manifest_path = replacement_manifest_path(
        &replacement.install_dir,
        &package.manifest_path,
        "conversation-adapter-package.json",
    );
    replacement_package.adapter_manifest_path = replacement_manifest_path(
        &replacement.install_dir,
        &package.adapter_manifest_path,
        "conversation-adapter.json",
    );
    replacement_package.installed_content_hash = Some(replacement.content_hash.clone());
    replacement_package.trusted_package_hash = Some(replacement.content_hash.clone());
    replacement_package.updated_at = Utc::now().to_rfc3339();
    replacement_package
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn catalog_item(id: &str, adapter_id: Option<&str>) -> ConversationScriptCatalogItem {
        ConversationScriptCatalogItem {
            id: id.to_string(),
            name: "Codex Session Parser".to_string(),
            version: "1.0.0".to_string(),
            record_kind: ConversationScriptRecordKind::Session,
            provider: Some("codex".to_string()),
            adapter_id: adapter_id.map(str::to_string),
            description: None,
            homepage_url: None,
            repository_url: None,
            tags: Vec::new(),
            manifest_file: None,
            package_manifest_file: None,
            expected_content_hash: None,
            expected_package_hash: None,
            expected_artifact_hash: None,
            artifact_size: None,
            source: ConversationScriptCatalogSource {
                kind: ConversationScriptCatalogSourceKind::Github,
                url: "https://github.com/util6/assetiweave/tree/main/builtin-assets/adapters/codex"
                    .to_string(),
                branch: None,
                path: None,
            },
        }
    }

    #[test]
    fn legacy_catalog_item_reverse_maps_to_native_install_spec() {
        let mut item = catalog_item("io.github.util6.codex-session", Some("codex"));
        item.expected_package_hash = Some("package-hash".to_string());
        item.expected_artifact_hash = Some("artifact-hash".to_string());
        item.artifact_size = Some(42);

        let spec = item.to_install_spec();

        assert_eq!(spec.id, item.id);
        assert_eq!(spec.adapter_id, item.adapter_id);
        assert_eq!(
            spec.record_kind,
            ConversationAdapterPackageRecordKind::Session
        );
        assert_eq!(spec.expected_package_hash, item.expected_package_hash);
        assert_eq!(spec.expected_artifact_hash, item.expected_artifact_hash);
        assert_eq!(spec.artifact_size, item.artifact_size);
        assert_eq!(
            spec.source.kind,
            ConversationAdapterPackageInstallSourceKind::Github
        );
        assert_eq!(spec.source.url, item.source.url);
    }

    #[test]
    fn native_install_spec_github_source_preserves_tree_location() {
        let item = catalog_item("io.github.util6.codex-session", Some("codex"));
        let spec = item.to_install_spec();

        let location =
            super::super::conversation_adapter_installer::parse_github_install_source(&spec.source)
                .expect("parse install spec");

        assert_eq!(
            location.repo_url,
            "https://github.com/util6/assetiweave.git"
        );
        assert_eq!(location.branch.as_deref(), Some("main"));
        assert_eq!(
            location.path.as_deref(),
            Some("builtin-assets/adapters/codex")
        );
    }

    fn adapter(id: &str, version: &str) -> ConversationAdapter {
        ConversationAdapter {
            id: id.to_string(),
            name: "Codex".to_string(),
            kind: crate::backend::models::ConversationAdapterKind::External,
            version: version.to_string(),
            enabled: true,
            manifest_path: Some("/tmp/codex/conversation-adapter.json".to_string()),
            executable_path: Some("/tmp/codex/adapter.mjs".to_string()),
            content_hash: Some("hash".to_string()),
            trusted_hash: Some("hash".to_string()),
            trust_state: crate::backend::models::ConversationAdapterTrustState::Trusted,
            protocol_version: Some(1),
            capabilities: vec!["read_session".to_string()],
            input_kinds: Vec::new(),
            card_contract_version: None,
            card_kinds: Vec::new(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn package(id: &str, adapter_id: &str, version: &str) -> ConversationAdapterPackage {
        ConversationAdapterPackage {
            package_id: id.to_string(),
            adapter_id: adapter_id.to_string(),
            name: "Codex Session Parser".to_string(),
            version: version.to_string(),
            record_kind: ConversationAdapterPackageRecordKind::Session,
            install_dir: format!("/tmp/{id}/current"),
            manifest_path: format!("/tmp/{id}/current/conversation-adapter-package.json"),
            adapter_manifest_path: format!("/tmp/{id}/current/conversation-adapter.json"),
            runtime_protocol: "stdio-ndjson-v1".to_string(),
            runtime_ready: true,
            origin: ConversationAdapterPackageOrigin::ManagedRelease,
            source_url: None,
            git_ref: None,
            git_commit: None,
            catalog_url: None,
            update_policy: ConversationPackageUpdatePolicy::Manual,
            latest_version: Some(version.to_string()),
            last_checked_at: None,
            runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
            runtime_validated_at: None,
            installed_content_hash: Some("package-hash".to_string()),
            trusted_package_hash: Some("package-hash".to_string()),
            error_message: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn resolves_installed_state_from_declared_adapter_id() {
        let entries = resolve_conversation_adapter_package_catalog_entries(
            vec![catalog_item("codex-session", Some("codex"))],
            &[adapter("codex", "1.0.0")],
            &[],
        );

        assert!(entries[0].installed);
        assert_eq!(entries[0].status, "legacy_installed");
        assert!(!entries[0].update_available);
        assert_eq!(entries[0].installed_adapter.as_ref().unwrap().id, "codex");
    }

    #[test]
    fn marks_installed_package_with_different_version_as_update_available() {
        let entries = resolve_conversation_adapter_package_catalog_entries(
            vec![catalog_item("codex-session", Some("codex"))],
            &[adapter("codex", "0.9.0")],
            &[package("codex-session", "codex", "0.9.0")],
        );

        assert!(entries[0].installed);
        assert!(entries[0].update_available);
        assert!(!entries[0].ahead_of_release);
        assert_eq!(entries[0].status, "update_available");
    }

    #[test]
    fn marks_installed_package_newer_than_catalog_as_ahead_of_release() {
        let entries = resolve_conversation_adapter_package_catalog_entries(
            vec![catalog_item("codex-session", Some("codex"))],
            &[adapter("codex", "1.1.0")],
            &[package("codex-session", "codex", "1.1.0")],
        );

        assert!(entries[0].installed);
        assert!(!entries[0].update_available);
        assert!(entries[0].ahead_of_release);
        assert_eq!(entries[0].status, "ahead_of_release");
    }

    #[test]
    fn managed_package_without_registered_runtime_is_reported_as_uninstalled() {
        let mut package = package("codex-session", "codex", "1.0.0");
        package.runtime_ready = false;
        package.runtime_gate_status = ConversationAdapterRuntimeGateStatus::RuntimeMissing;

        let entries = resolve_conversation_adapter_package_catalog_entries(
            vec![catalog_item("codex-session", Some("codex"))],
            &[],
            &[package],
        );

        assert!(entries[0].installed);
        assert_eq!(entries[0].status, "uninstalled");
        assert!(!entries[0].runtime_ready);
    }

    #[test]
    #[cfg(unix)]
    fn uninstalled_replacement_uses_content_hash_and_rebases_manifest_paths() {
        let package = package("codex-session", "codex", "2.0.0");
        let replacement = crate::backend::models::ConversationAdapterPackageVersion {
            package_id: package.package_id.clone(),
            version: "1.0.0".to_string(),
            install_dir: "/tmp/codex-session/versions/1.0.0".to_string(),
            artifact_hash: Some("artifact-zip-hash".to_string()),
            content_hash: "unpacked-content-hash".to_string(),
            runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
            installed_at: "2026-07-17T00:00:00Z".to_string(),
        };

        let replaced = package_for_uninstalled_replacement(&package, &replacement);

        assert_eq!(replaced.version, "1.0.0");
        assert_eq!(
            replaced.trusted_package_hash.as_deref(),
            Some("unpacked-content-hash")
        );
        assert_eq!(
            replaced.manifest_path,
            "/tmp/codex-session/versions/1.0.0/conversation-adapter-package.json"
        );
        assert_eq!(
            replaced.adapter_manifest_path,
            "/tmp/codex-session/versions/1.0.0/conversation-adapter.json"
        );
    }

    #[test]
    fn parses_github_tree_url_into_repo_branch_and_path() {
        let source = ConversationScriptCatalogSource {
            kind: ConversationScriptCatalogSourceKind::Github,
            url: "https://github.com/util6/assetiweave/tree/main/builtin-assets/adapters/codex"
                .to_string(),
            branch: None,
            path: None,
        };

        let location = parse_github_catalog_location(&source).expect("parse GitHub tree URL");

        assert_eq!(
            location.repo_url,
            "https://github.com/util6/assetiweave.git"
        );
        assert_eq!(location.branch.as_deref(), Some("main"));
        assert_eq!(
            location.path.as_deref(),
            Some("builtin-assets/adapters/codex"),
        );
    }

    #[test]
    fn rejects_unsafe_manifest_file_names() {
        let mut item = catalog_item("codex-session", Some("codex"));
        item.manifest_file = Some("../conversation-adapter.json".to_string());

        assert!(validate_conversation_script_catalog_item(&item).is_err());

        let mut item = catalog_item("codex-session", Some("codex"));
        item.package_manifest_file = Some("../conversation-adapter-package.json".to_string());

        assert!(validate_conversation_script_catalog_item(&item).is_err());
    }

    #[test]
    fn package_versions_require_semver_before_becoming_path_segments() {
        assert_eq!(
            validated_package_version("1.2.3-beta.1").unwrap(),
            "1.2.3-beta.1"
        );
        assert!(validated_package_version("latest").is_err());
        assert!(validated_package_version("1/../../external").is_err());
    }

    #[test]
    fn rollback_selects_the_most_recently_installed_inactive_version() {
        let versions = vec![
            package_version("1.2.0", "2026-07-16T02:00:00Z"),
            package_version("1.1.0", "2026-07-16T01:00:00Z"),
            package_version("1.0.0", "2026-07-16T00:00:00Z"),
        ];

        assert_eq!(
            select_rollback_version(&versions, "1.2.0").map(|version| version.version.as_str()),
            Some("1.1.0")
        );
    }

    fn package_version(
        version: &str,
        installed_at: &str,
    ) -> crate::backend::models::ConversationAdapterPackageVersion {
        crate::backend::models::ConversationAdapterPackageVersion {
            package_id: "io.github.util6.test".to_string(),
            version: version.to_string(),
            install_dir: format!("/tmp/versions/{version}"),
            artifact_hash: None,
            content_hash: format!("hash-{version}"),
            runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
            installed_at: installed_at.to_string(),
        }
    }

    #[test]
    fn artifact_zip_rejects_path_traversal() {
        use std::io::Write;

        let root =
            std::env::temp_dir().join(format!("assetiweave-artifact-traversal-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create artifact test root");
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("../escape.txt", zip::write::SimpleFileOptions::default())
            .expect("start unsafe zip entry");
        writer.write_all(b"escape").expect("write unsafe zip entry");
        let bytes = writer.finish().expect("finish zip").into_inner();
        let mut item = catalog_item("io.github.util6.escape-test", Some("escape-test"));
        item.source.kind = ConversationScriptCatalogSourceKind::ArtifactZip;

        let spec = item.to_install_spec();
        let result = super::super::conversation_adapter_installer::extract_install_artifact_bytes(
            &spec,
            bytes,
            &root.join("staging"),
        );

        assert!(result.is_err());
        assert!(!root.join("escape.txt").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_zip_rejects_windows_reserved_file_names() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!(
            "assetiweave-artifact-reserved-name-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create artifact test root");
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("package/CON.txt", zip::write::SimpleFileOptions::default())
            .expect("start reserved zip entry");
        writer.write_all(b"reserved").expect("write zip entry");
        let bytes = writer.finish().expect("finish zip").into_inner();
        let mut item = catalog_item("io.github.util6.reserved-test", Some("reserved-test"));
        item.source.kind = ConversationScriptCatalogSourceKind::ArtifactZip;

        let spec = item.to_install_spec();
        let error = super::super::conversation_adapter_installer::extract_install_artifact_bytes(
            &spec,
            bytes,
            &root.join("staging"),
        )
        .expect_err("reserved Windows name must be rejected");

        assert!(error.to_string().contains("reserved on Windows"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_zip_rejects_case_insensitive_path_collisions() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!(
            "assetiweave-artifact-case-collision-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create artifact test root");
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for entry_name in ["Package/Adapter.js", "package/adapter.js"] {
            writer
                .start_file(entry_name, zip::write::SimpleFileOptions::default())
                .expect("start colliding zip entry");
            writer.write_all(b"entry").expect("write zip entry");
        }
        let bytes = writer.finish().expect("finish zip").into_inner();
        let mut item = catalog_item("io.github.util6.collision-test", Some("collision-test"));
        item.source.kind = ConversationScriptCatalogSourceKind::ArtifactZip;

        let spec = item.to_install_spec();
        let error = super::super::conversation_adapter_installer::extract_install_artifact_bytes(
            &spec,
            bytes,
            &root.join("staging"),
        )
        .expect_err("case-insensitive collision must be rejected");

        assert!(error.to_string().contains("colliding paths"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn managed_package_delete_target_rejects_external_install_dir() {
        let root =
            std::env::temp_dir().join(format!("assetiweave-package-delete-{}", Uuid::new_v4()));
        let managed_root = root.join("conversation-adapters");
        let external_dir = root.join("external").join("current");
        fs::create_dir_all(managed_root.join("packages").join("publisher.package"))
            .expect("create managed package root");
        fs::create_dir_all(&external_dir).expect("create external package");

        let result = validate_managed_package_delete_target(
            &managed_root,
            "publisher.package",
            &external_dir,
        );

        assert!(result.is_err());
        assert!(external_dir.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn managed_package_delete_target_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root =
            std::env::temp_dir().join(format!("assetiweave-package-symlink-{}", Uuid::new_v4()));
        let managed_root = root.join("conversation-adapters");
        let package_root = managed_root.join("packages").join("publisher.package");
        let external_dir = root.join("external");
        fs::create_dir_all(&package_root).expect("create managed package root");
        fs::create_dir_all(&external_dir).expect("create external package");
        symlink(&external_dir, package_root.join("current")).expect("create current symlink");

        let result = validate_managed_package_delete_target(
            &managed_root,
            "publisher.package",
            &package_root.join("current"),
        );

        assert!(result.is_err());
        assert!(external_dir.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_gate_errors_have_distinct_repair_states() {
        assert_eq!(
            classify_runtime_gate_error(
                Path::new("/missing/package"),
                "conversation adapter package root is not a directory"
            ),
            ConversationAdapterRuntimeGateStatus::RuntimeMissing
        );
        assert_eq!(
            classify_runtime_gate_error(
                Path::new("/existing/package"),
                "conversation adapter package content hash mismatch"
            ),
            ConversationAdapterRuntimeGateStatus::HashMismatch
        );
        assert_eq!(
            classify_runtime_gate_error(
                Path::new("/existing/package"),
                "conversation adapter package requires AssetIWeave core >= 9.0.0"
            ),
            ConversationAdapterRuntimeGateStatus::CoreIncompatible
        );
        assert_eq!(
            classify_runtime_gate_error(
                Path::new("/existing/package"),
                "conversation adapter package was not valid JSON"
            ),
            ConversationAdapterRuntimeGateStatus::ManifestInvalid
        );
    }

    #[test]
    fn unregister_preflight_lists_affected_sources_and_running_sync_conflicts() {
        let root =
            std::env::temp_dir().join(format!("assetiweave-package-preflight-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create preflight test root");
        let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
        let adapter = adapter("external-preflight", "1.0.0");
        let source = ConversationSource {
            id: "external-preflight-source".to_string(),
            adapter_id: adapter.id.clone(),
            name: "External preflight source".to_string(),
            kind: crate::backend::models::ConversationSourceKind::Directory,
            location: root.join("sessions").to_string_lossy().to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: "2026-07-15T00:00:00Z".to_string(),
            updated_at: "2026-07-15T00:00:00Z".to_string(),
        };
        let pool = service.db.pool().clone();
        let tenant_id = service.tenant_id().to_string();
        service
            .db
            .block_on(async move {
                crate::backend::store::upsert_conversation_adapter_sqlx(
                    &pool, &tenant_id, &adapter,
                )
                .await?;
                crate::backend::store::upsert_conversation_source_sqlx(&pool, &tenant_id, &source)
                    .await?;
                sqlx::query(
                    r#"
                    INSERT INTO conversation_sync_runs (
                        tenant_id, id, source_id, adapter_id, status, started_at,
                        session_count, turn_count, warning_count
                    ) VALUES (?1, 'running-sync', ?2, ?3, 'running',
                              '2026-07-15T00:00:00Z', 0, 0, 0)
                    "#,
                )
                .bind(&tenant_id)
                .bind(&source.id)
                .bind(&adapter.id)
                .execute(&pool)
                .await
                .map_err(|error| error.to_string())?;
                AppResult::Ok(())
            })
            .expect("seed preflight records");
        service
            .runtime
            .refresh_conversation_adapter_catalog()
            .expect("refresh test adapter catalog");

        let preflight = service
            .prepare_conversation_adapter_package_change(ConversationAdapterPackageChangeParams {
                action: ConversationAdapterPackageChangeAction::Unregister,
                package_id: None,
                adapter_id: Some("external-preflight".to_string()),
            })
            .expect("prepare unregister");

        assert_eq!(
            preflight.origin,
            ConversationAdapterPackageOrigin::LegacyExternal
        );
        assert_eq!(preflight.affected_sources.len(), 1);
        assert_eq!(preflight.task_conflicts, vec!["conversation_sync"]);
        assert!(preflight.preserves_conversation_records);
        assert!(preflight.confirmation_required);

        drop(service);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn builtin_unregister_preflight_allows_disable_and_retains_registration() {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-builtin-disable-preflight-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create test root");
        let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
        let mut builtin = adapter("builtin-preflight", "1.0.0");
        builtin.trust_state = crate::backend::models::ConversationAdapterTrustState::BuiltIn;
        let pool = service.db.pool().clone();
        let tenant_id = service.tenant_id().to_string();
        service
            .db
            .block_on(async move {
                crate::backend::store::upsert_conversation_adapter_sqlx(&pool, &tenant_id, &builtin)
                    .await
            })
            .expect("seed built-in adapter");
        service
            .runtime
            .refresh_conversation_adapter_catalog()
            .expect("refresh test adapter catalog");

        let preflight = service
            .prepare_conversation_adapter_package_change(ConversationAdapterPackageChangeParams {
                action: ConversationAdapterPackageChangeAction::Unregister,
                package_id: None,
                adapter_id: Some("builtin-preflight".to_string()),
            })
            .expect("built-in disable preflight");
        assert_eq!(preflight.origin, ConversationAdapterPackageOrigin::BuiltIn);

        service
            .unregister_conversation_adapter(ConversationAdapterUnregisterParams {
                adapter_id: "builtin-preflight".to_string(),
                dry_run: false,
                yes: true,
            })
            .expect("disable built-in adapter");
        let retained = service
            .list_conversation_adapters()
            .expect("list adapters")
            .into_iter()
            .find(|adapter| adapter.id == "builtin-preflight")
            .expect("built-in registration retained");
        assert!(!retained.enabled);

        drop(service);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn workspace_upgrade_promotes_only_a_probed_immutable_runtime_copy() {
        use std::os::unix::fs::PermissionsExt;

        let root =
            std::env::temp_dir().join(format!("assetiweave-workspace-upgrade-{}", Uuid::new_v4()));
        let package_dir = root.join("workspace").join("external-test");
        let managed_root = root.join("managed");
        fs::create_dir_all(&package_dir).expect("create workspace package");
        fs::write(
            package_dir.join("conversation-adapter-package.json"),
            r#"{
  "schema_version": 1,
  "package_id": "com.util6.external-test",
  "name": "External Test",
  "version": "1.0.0",
  "min_core_version": "0.1.0",
  "record_kind": "session",
  "adapter_manifest": "conversation-adapter.json",
  "capabilities": ["probe", "read_session"],
  "runtime": { "protocol": "stdio-ndjson-v1" },
  "changelog": []
}"#,
        )
        .expect("write package manifest");
        fs::write(
            package_dir.join("conversation-adapter.json"),
            r#"{
  "schema_version": 1,
  "id": "external-test",
  "name": "External Test",
  "version": "1.0.0",
  "protocol_version": 1,
  "command": ["adapter.sh"],
  "capabilities": ["probe", "read_session"],
  "input_kinds": ["directory"]
}"#,
        )
        .expect("write adapter manifest");
        let executable = package_dir.join("adapter.sh");
        let write_executable = |body: &str| {
            fs::write(&executable, body).expect("write adapter executable");
            let mut permissions = fs::metadata(&executable)
                .expect("adapter metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&executable, permissions).expect("make adapter executable");
        };
        write_executable(
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"complete\",\"item\":{\"revision\":1}}'\n",
        );

        let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
        let first = promote_conversation_adapter_workspace_package(
            &service,
            &package_dir,
            &managed_root,
            false,
        )
        .expect("promote first workspace revision");
        let first_install = PathBuf::from(
            first["package"]["install_dir"]
                .as_str()
                .expect("first install dir"),
        );
        assert_ne!(first_install, package_dir);
        assert!(first_install.join("adapter.sh").is_file());

        write_executable(
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"complete\",\"item\":{\"revision\":2}}'\n",
        );
        let second = promote_conversation_adapter_workspace_package(
            &service,
            &package_dir,
            &managed_root,
            false,
        )
        .expect("promote second workspace revision");
        let second_install = PathBuf::from(
            second["package"]["install_dir"]
                .as_str()
                .expect("second install dir"),
        );
        assert_ne!(second_install, first_install);
        assert!(!first_install.exists());
        assert!(second_install.is_dir());

        write_executable("#!/bin/sh\ncat >/dev/null\nprintf 'invalid\\n'\n");
        let error = promote_conversation_adapter_workspace_package(
            &service,
            &package_dir,
            &managed_root,
            false,
        )
        .expect_err("reject invalid workspace revision");
        assert!(error.to_string().contains("probe failed"));
        let retained = service
            .load_conversation_adapter_package("com.util6.external-test")
            .expect("load retained package")
            .expect("retained package");
        assert_eq!(PathBuf::from(retained.install_dir), second_install);
        assert!(second_install.is_dir());

        fs::write(
            package_dir.join("conversation-adapter-package.json"),
            fs::read_to_string(package_dir.join("conversation-adapter-package.json"))
                .expect("read package manifest")
                .replace("\"version\": \"1.0.0\"", "\"version\": \"0.9.0\""),
        )
        .expect("write older package manifest");
        fs::write(
            package_dir.join("conversation-adapter.json"),
            fs::read_to_string(package_dir.join("conversation-adapter.json"))
                .expect("read adapter manifest")
                .replace("\"version\": \"1.0.0\"", "\"version\": \"0.9.0\""),
        )
        .expect("write older adapter manifest");
        write_executable(
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"complete\",\"item\":{\"revision\":3}}'\n",
        );
        let skipped = promote_conversation_adapter_workspace_package(
            &service,
            &package_dir,
            &managed_root,
            false,
        )
        .expect("skip older workspace revision");
        assert_eq!(skipped["upgraded"], false);
        assert_eq!(skipped["skipped"], true);
        assert_eq!(skipped["reason"], "active_version_newer");
        assert_eq!(skipped["active_version"], "1.0.0");
        let retained_after_skip = service
            .load_conversation_adapter_package("com.util6.external-test")
            .expect("load package after skipped downgrade")
            .expect("retained package after skipped downgrade");
        assert_eq!(
            PathBuf::from(retained_after_skip.install_dir),
            second_install
        );

        drop(service);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn local_registration_and_unregistration_never_modify_external_package_files() {
        use std::os::unix::fs::PermissionsExt;

        let root =
            std::env::temp_dir().join(format!("assetiweave-local-package-{}", Uuid::new_v4()));
        let package_dir = root.join("external-package");
        fs::create_dir_all(&package_dir).expect("create local package");
        fs::write(
            package_dir.join("conversation-adapter-package.json"),
            r#"{
  "schema_version": 1,
  "package_id": "com.util6.external-test",
  "name": "External Test",
  "version": "1.0.0",
  "min_core_version": "0.1.0",
  "record_kind": "session",
  "adapter_manifest": "conversation-adapter.json",
  "capabilities": ["probe", "read_session"],
  "runtime": { "protocol": "stdio-ndjson-v1" },
  "changelog": []
}"#,
        )
        .expect("write package manifest");
        fs::write(
            package_dir.join("conversation-adapter.json"),
            r#"{
  "schema_version": 1,
  "id": "external-test",
  "name": "External Test",
  "version": "1.0.0",
  "protocol_version": 1,
  "command": ["adapter.sh"],
  "capabilities": ["probe", "read_session"],
  "input_kinds": ["directory"]
}"#,
        )
        .expect("write adapter manifest");
        let executable = package_dir.join("adapter.sh");
        fs::write(
            &executable,
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"complete\",\"item\":{\"ok\":true}}'\n",
        )
        .expect("write adapter executable");
        let mut permissions = fs::metadata(&executable)
            .expect("adapter metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("make adapter executable");
        let discovered =
            discover_local_conversation_adapter_packages(&root).expect("discover local package");
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].id, "com.util6.external-test");
        assert_eq!(discovered[0].adapter_id.as_deref(), Some("external-test"));
        assert_eq!(
            discovered[0].source.kind,
            ConversationScriptCatalogSourceKind::LocalDirectory
        );
        assert_eq!(discovered[0].source.url, package_dir.to_string_lossy());
        let content_before =
            crate::backend::conversations::validate_conversation_adapter_package_dir(&package_dir)
                .expect("validate external package")
                .content_hash;

        let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
        service
            .register_conversation_adapter_local(ConversationAdapterLocalRegisterParams {
                package_dir: package_dir.to_string_lossy().to_string(),
                origin: ConversationAdapterPackageOrigin::LocalDirectory,
                source_url: None,
                git_ref: None,
                git_commit: None,
                dry_run: false,
                yes: true,
            })
            .expect("register local package");
        let registered = service
            .load_conversation_adapter_package("com.util6.external-test")
            .expect("load package")
            .expect("registered package");
        assert_eq!(
            registered.origin,
            ConversationAdapterPackageOrigin::LocalDirectory
        );

        service
            .unregister_conversation_adapter(ConversationAdapterUnregisterParams {
                adapter_id: "external-test".to_string(),
                dry_run: false,
                yes: true,
            })
            .expect("unregister local package");

        assert!(package_dir.is_dir());
        assert_eq!(
            crate::backend::conversations::validate_conversation_adapter_package_dir(&package_dir)
                .expect("revalidate external package")
                .content_hash,
            content_before
        );
        assert!(service
            .load_conversation_adapter_package("com.util6.external-test")
            .expect("load unregistered package")
            .is_none());

        drop(service);
        let _ = fs::remove_dir_all(root);
    }
}
