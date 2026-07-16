use super::prelude::*;
use crate::backend::models::{
    ConversationAdapterPackageChangeAction, ConversationAdapterPackageChangeRisk,
    ConversationAdapterPackageOrigin, ConversationAdapterPackageRecordKind,
    ConversationAdapterRuntimeGateStatus, ConversationPackageUpdatePolicy,
};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read};

const DEFAULT_CONVERSATION_SCRIPT_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/util6/assetiweave/main/parser-catalog/catalog.json";
const LOCAL_DEFAULT_CONVERSATION_SCRIPT_CATALOG: &str =
    include_str!("../../../../parser-catalog/catalog.json");
const CONVERSATION_SCRIPT_SECURITY_NOTICE: &str =
    "Review remote conversation adapter package contents before installing; AssetIWeave registers the downloaded adapter package as trusted for local execution.";

impl AppService {
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
            return Err(
                "conversation adapter package inspection requires package_id or adapter_id"
                    .to_string(),
            );
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
            return Err(format!("conversation adapter package not found: {id}"));
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
            return Err(
                "local conversation adapter registration requires local_directory, git_ref, or dev_override origin"
                    .to_string(),
            );
        }
        if !params.dry_run && !params.yes {
            return Err(
                "conversation adapter local registration requires confirmation".to_string(),
            );
        }
        if params.origin == ConversationAdapterPackageOrigin::GitRef
            && params
                .git_ref
                .as_deref()
                .and_then(clean_non_empty_string)
                .is_none()
        {
            return Err("git_ref conversation adapter registration requires git_ref".to_string());
        }

        let package_dir = crate::backend::path_utils::expand_path(&params.package_dir)?;
        let validation =
            crate::backend::conversations::validate_conversation_adapter_package_dir(&package_dir)?;
        if let Some(existing) =
            self.load_conversation_adapter_package(&validation.manifest.package_id)?
        {
            if existing.origin == ConversationAdapterPackageOrigin::ManagedRelease {
                return Err(format!(
                    "managed conversation adapter package is already installed: {}",
                    existing.package_id
                ));
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
            && matches!(
                params.action,
                ConversationAdapterPackageChangeAction::Unregister
                    | ConversationAdapterPackageChangeAction::Uninstall
            )
        {
            return Err(
                "built-in conversation adapters cannot be unregistered or uninstalled".to_string(),
            );
        }
        if params.action == ConversationAdapterPackageChangeAction::Unregister
            && origin == ConversationAdapterPackageOrigin::ManagedRelease
        {
            return Err(
                "managed conversation adapter packages must be uninstalled, not unregistered"
                    .to_string(),
            );
        }
        if params.action == ConversationAdapterPackageChangeAction::Uninstall
            && origin != ConversationAdapterPackageOrigin::ManagedRelease
        {
            return Err(
                "only managed conversation adapter packages can be uninstalled".to_string(),
            );
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
                    if !Path::new(&install_dir).exists() {
                        continue;
                    }
                    let package_root = validate_managed_package_delete_target(
                        &managed_root,
                        &package.package_id,
                        Path::new(&install_dir),
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
        let catalog = load_conversation_script_catalog(params.catalog_url.as_deref())?;
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
            return Err("conversation adapter package install requires --yes".to_string());
        }
        if params
            .version
            .as_deref()
            .and_then(clean_non_empty_string)
            .is_some()
        {
            return self.install_conversation_adapter_package_release(params);
        }

        let catalog = load_conversation_script_catalog(params.catalog_url.as_deref())?;
        let package_id = params.package_id.trim();
        let item = catalog
            .items
            .into_iter()
            .find(|item| item.package_id() == package_id)
            .ok_or_else(|| format!("conversation adapter package not found: {package_id}"))?;
        validate_conversation_script_catalog_item(&item)?;

        install_conversation_adapter_package_from_item(
            self,
            &item,
            params.dry_run,
            params.catalog_url.as_deref(),
        )
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
            return Err("conversation adapter package update requires --yes".to_string());
        }
        if params
            .version
            .as_deref()
            .and_then(clean_non_empty_string)
            .is_some()
        {
            return self.install_conversation_adapter_package_release(params);
        }

        let catalog = load_conversation_script_catalog(params.catalog_url.as_deref())?;
        let package_id = params.package_id.trim();
        let item = catalog
            .items
            .into_iter()
            .find(|item| item.package_id() == package_id)
            .ok_or_else(|| format!("conversation adapter package not found: {package_id}"))?;
        validate_conversation_script_catalog_item(&item)?;
        install_conversation_adapter_package_from_item(
            self,
            &item,
            params.dry_run,
            params.catalog_url.as_deref(),
        )
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
            return Err("conversation adapter package uninstall requires --yes".to_string());
        }
        let package_id = params.package_id.trim();
        if package_id.is_empty() {
            return Err("conversation adapter package id is required".to_string());
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
        self.db.block_on(async move {
            crate::backend::store::list_conversation_adapter_packages_sqlx(&pool, &tenant_id).await
        })
    }

    pub(crate) fn load_conversation_adapter_package(
        &self,
        package_id: &str,
    ) -> AppResult<Option<ConversationAdapterPackage>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let package_id = package_id.to_string();
        self.db.block_on(async move {
            crate::backend::store::load_conversation_adapter_package_sqlx(
                &pool,
                &tenant_id,
                &package_id,
            )
            .await
        })
    }

    pub(crate) fn load_conversation_adapter_package_versions(
        &self,
        package_id: &str,
    ) -> AppResult<Vec<crate::backend::models::ConversationAdapterPackageVersion>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let package_id = package_id.to_string();
        self.db.block_on(async move {
            crate::backend::store::list_conversation_adapter_package_versions_sqlx(
                &pool,
                &tenant_id,
                &package_id,
            )
            .await
        })
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
            return Err("only managed package versions can be deleted".to_string());
        }
        let runtime_registered = self
            .list_conversation_adapters()?
            .iter()
            .any(|adapter| adapter.id == package.adapter_id);
        if package.version == version && runtime_registered {
            return Err(
                "active conversation adapter package version must be uninstalled or switched before deletion"
                    .to_string(),
            );
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
        let version_dir = validate_managed_package_version_delete_target(
            &managed_root,
            package_id,
            &version,
            Path::new(&target.install_dir),
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
            return Err("conversation adapter package version deletion requires --yes".to_string());
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
                return Err("installed package version record was not found".to_string());
            }
            Err(error) => {
                let _ = fs::rename(&staged, &version_dir);
                return Err(error);
            }
        }
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
            return Err(
                "conversation adapter package version activation requires --yes".to_string(),
            );
        }
        let mut package = self
            .load_conversation_adapter_package(package_id)?
            .ok_or_else(|| format!("conversation adapter package not found: {package_id}"))?;
        if package.origin != ConversationAdapterPackageOrigin::ManagedRelease {
            return Err("only managed package versions can be activated".to_string());
        }
        let versions = self.load_conversation_adapter_package_versions(package_id)?;
        let target = versions
            .iter()
            .find(|candidate| candidate.version == version)
            .ok_or_else(|| {
                format!("installed package version not found: {package_id}@{version}")
            })?;
        let validation = crate::backend::conversations::validate_conversation_adapter_package_dir(
            Path::new(&target.install_dir),
        )?;
        if validation.manifest.package_id != package_id
            || validation.manifest.version != version
            || validation.content_hash != target.content_hash
        {
            return Err(
                "installed conversation adapter package version failed immutable validation"
                    .to_string(),
            );
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
        self.db.block_on(async move {
            crate::backend::store::load_conversation_adapter_package_by_adapter_sqlx(
                &pool,
                &tenant_id,
                &adapter_id,
            )
            .await
        })
    }

    pub(crate) fn save_conversation_adapter_package(
        &self,
        package: &ConversationAdapterPackage,
    ) -> AppResult<()> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let package = package.clone();
        self.db.block_on(async move {
            crate::backend::store::upsert_conversation_adapter_package_sqlx(
                &pool, &tenant_id, &package,
            )
            .await
        })
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
            Err(format_package_not_ready_error(&package))
        }
    }

    fn refresh_conversation_adapter_package_runtime(
        &self,
        package: &mut ConversationAdapterPackage,
        adapter: Option<&ConversationAdapter>,
    ) -> AppResult<()> {
        let install_dir = Path::new(&package.install_dir);
        let evaluated = crate::backend::conversations::validate_conversation_adapter_package_dir(
            install_dir,
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
                package.runtime_gate_status = classify_runtime_gate_error(install_dir, &error);
                package.error_message = Some(error);
            }
        }
        package.runtime_validated_at = Some(now.clone());
        package.updated_at = now;
        self.save_conversation_adapter_package(package)
    }
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
        Err(format!(
            "conversation adapter package change conflicts with running tasks: {}",
            preflight.task_conflicts.join(", ")
        ))
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
        return Err(format!(
            "conversation adapter package id is not a safe path segment: {package_id}"
        ));
    }

    let packages_root = managed_root.join("packages");
    if !install_dir.exists() || !install_dir.starts_with(&packages_root) {
        return Err(format!(
            "conversation adapter package delete target does not exist in the managed library: {}",
            install_dir.display()
        ));
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
        return Err(format!(
            "conversation adapter package root escapes the managed library: {}",
            package_root.display()
        ));
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
        return Err("conversation adapter version delete target is not the requested managed version directory".to_string());
    }
    Ok(canonical_install)
}

pub(super) fn install_conversation_adapter_package_from_item(
    service: &AppService,
    item: &ConversationScriptCatalogItem,
    dry_run: bool,
    catalog_url: Option<&str>,
) -> AppResult<Value> {
    let version_dir = conversation_adapter_package_version_dir(item)?;
    let package_manifest_path = version_dir.join(item.package_manifest_file_name()?);
    let adapter_manifest_path = version_dir.join(item.manifest_file_name()?);

    if dry_run {
        return Ok(json!({
            "dry_run": true,
            "installed": false,
            "package_id": item.package_id(),
            "item": item,
            "install_path": version_dir,
            "package_manifest_path": package_manifest_path,
            "manifest_path": adapter_manifest_path,
            "security_notice": CONVERSATION_SCRIPT_SECURITY_NOTICE,
        }));
    }

    let previous_package = service.load_conversation_adapter_package(item.package_id())?;
    let installed = match install_conversation_adapter_package_files(item, &version_dir) {
        Ok(installed) => installed,
        Err(error) => {
            if previous_package.is_none() {
                persist_failed_conversation_adapter_package(service, item, &version_dir, &error)?;
            }
            return Err(error);
        }
    };

    let preview = crate::backend::conversations::register_external_adapter(
        crate::backend::conversations::ExternalAdapterRegisterParams {
            manifest_path: installed.validation.adapter_manifest_path.clone(),
            dry_run: false,
            yes: true,
        },
    )?;
    let adapter = crate::backend::conversations::adapter_from_registration_preview(preview)?;
    let now = Utc::now().to_rfc3339();
    let package = ConversationAdapterPackage {
        package_id: item.package_id().to_string(),
        adapter_id: adapter.id.clone(),
        name: installed.validation.manifest.name.clone(),
        version: installed.validation.manifest.version.clone(),
        record_kind: item.record_kind.as_package_record_kind(),
        install_dir: version_dir.to_string_lossy().to_string(),
        manifest_path: installed.validation.manifest_path.clone(),
        adapter_manifest_path: installed.validation.adapter_manifest_path.clone(),
        runtime_protocol: installed
            .validation
            .manifest
            .runtime
            .protocol
            .as_str()
            .to_string(),
        runtime_ready: true,
        origin: ConversationAdapterPackageOrigin::ManagedRelease,
        source_url: Some(item.source.url.clone()),
        git_ref: item.source.branch.clone(),
        git_commit: None,
        catalog_url: catalog_url.and_then(clean_non_empty_string),
        update_policy: ConversationPackageUpdatePolicy::Manual,
        latest_version: Some(item.version.clone()),
        last_checked_at: Some(now.clone()),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        runtime_validated_at: Some(now.clone()),
        installed_content_hash: Some(installed.validation.content_hash.clone()),
        trusted_package_hash: Some(
            item.expected_package_hash
                .as_deref()
                .and_then(clean_non_empty_string)
                .unwrap_or_else(|| installed.validation.content_hash.clone()),
        ),
        error_message: None,
        created_at: previous_package
            .as_ref()
            .map(|package| package.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    let version = crate::backend::models::ConversationAdapterPackageVersion {
        package_id: package.package_id.clone(),
        version: package.version.clone(),
        install_dir: package.install_dir.clone(),
        artifact_hash: item
            .expected_artifact_hash
            .as_deref()
            .and_then(clean_non_empty_string),
        content_hash: installed.validation.content_hash.clone(),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        installed_at: package.updated_at.clone(),
    };
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    let adapter_to_save = adapter.clone();
    let package_to_save = package.clone();
    let activation = service.db.block_on(async move {
        crate::backend::store::activate_conversation_adapter_package_sqlx(
            &pool,
            &tenant_id,
            &adapter_to_save,
            &package_to_save,
            &version,
        )
        .await
    });
    if let Err(error) = activation {
        if installed.created_version_dir {
            let _ = fs::remove_dir_all(&version_dir);
        }
        if previous_package.is_none() {
            persist_failed_conversation_adapter_package(service, item, &version_dir, &error)?;
        }
        return Err(error);
    }

    Ok(json!({
        "dry_run": false,
        "installed": true,
        "package_id": item.package_id(),
        "item": item,
        "install_path": version_dir,
        "package_manifest_path": installed.validation.manifest_path,
        "manifest_path": installed.validation.adapter_manifest_path,
        "package": package,
        "adapter": adapter,
        "validation": installed.validation,
        "security_notice": CONVERSATION_SCRIPT_SECURITY_NOTICE,
    }))
}

struct InstalledConversationAdapterPackage {
    validation: crate::backend::conversations::ConversationAdapterPackageValidationResult,
    created_version_dir: bool,
}

fn install_conversation_adapter_package_files(
    item: &ConversationScriptCatalogItem,
    version_dir: &Path,
) -> AppResult<InstalledConversationAdapterPackage> {
    let staging_dir = conversation_script_staging_dir(item)?;
    let prepared_dir = conversation_adapter_package_prepared_dir(item)?;
    let install_result = (|| {
        let source_dir = match item.source.kind {
            ConversationScriptCatalogSourceKind::Github => {
                let location = parse_github_catalog_location(&item.source)?;
                clone_github_catalog_source(&location, &staging_dir)?;
                location.source_dir(&staging_dir)
            }
            ConversationScriptCatalogSourceKind::ArtifactZip => {
                download_and_extract_catalog_artifact(item, &staging_dir)?
            }
            ConversationScriptCatalogSourceKind::LocalDirectory => {
                return Err("local registered packages cannot be installed from Catalog".to_string())
            }
        };
        if !source_dir.is_dir() {
            return Err(format!(
                "conversation adapter package source path is not a directory: {}",
                source_dir.display()
            ));
        }
        let package_manifest_file = item.package_manifest_file_name()?;
        if !source_dir.join(&package_manifest_file).is_file() {
            return Err(format!(
                "conversation adapter package source does not contain {}: {}",
                package_manifest_file,
                source_dir.display()
            ));
        }

        if prepared_dir.exists() {
            return Err(format!(
                "conversation adapter package prepared path already exists: {}",
                prepared_dir.display()
            ));
        }
        capabilities::copy_dir(&source_dir, &prepared_dir)?;
        let prepared_validation =
            crate::backend::conversations::validate_conversation_adapter_package_dir(
                &prepared_dir,
            )?;
        validate_installed_package_for_catalog_item(item, &prepared_validation)?;

        if version_dir.exists() {
            let existing =
                crate::backend::conversations::validate_conversation_adapter_package_dir(
                    version_dir,
                )?;
            validate_installed_package_for_catalog_item(item, &existing)?;
            if existing.content_hash != prepared_validation.content_hash {
                return Err(format!(
                    "conversation adapter package version is immutable: {}@{}",
                    item.package_id(),
                    item.version
                ));
            }
            fs::remove_dir_all(&prepared_dir).map_err(|error| error.to_string())?;
            return Ok(InstalledConversationAdapterPackage {
                validation: existing,
                created_version_dir: false,
            });
        }
        let parent = version_dir
            .parent()
            .ok_or_else(|| "conversation adapter version directory has no parent".to_string())?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        fs::rename(&prepared_dir, version_dir).map_err(|error| error.to_string())?;
        let final_validation =
            crate::backend::conversations::validate_conversation_adapter_package_dir(version_dir)
                .and_then(|validation| {
                    validate_installed_package_for_catalog_item(item, &validation)?;
                    Ok(validation)
                });
        match final_validation {
            Ok(validation) => Ok(InstalledConversationAdapterPackage {
                validation,
                created_version_dir: true,
            }),
            Err(error) => {
                let _ = fs::remove_dir_all(version_dir);
                Err(error)
            }
        }
    })();

    let _ = fs::remove_dir_all(&staging_dir);
    if install_result.is_err() {
        let _ = fs::remove_dir_all(&prepared_dir);
    }
    install_result
}

fn download_and_extract_catalog_artifact(
    item: &ConversationScriptCatalogItem,
    staging_dir: &Path,
) -> AppResult<PathBuf> {
    if !item.source.url.starts_with("https://") {
        return Err("conversation adapter package artifacts require HTTPS".to_string());
    }
    let expected_hash = item
        .expected_artifact_hash
        .as_deref()
        .and_then(clean_non_empty_string)
        .ok_or_else(|| "conversation adapter package artifact sha256 is required".to_string())?;
    let response = ureq::get(&item.source.url)
        .set(
            "User-Agent",
            "AssetIWeave/0.5 conversation-adapter-package-artifact",
        )
        .call()
        .map_err(|error| {
            format!("download conversation adapter package artifact failed: {error}")
        })?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(512 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read conversation adapter package artifact failed: {error}"))?;
    if let Some(expected_size) = item.artifact_size {
        if bytes.len() as u64 != expected_size {
            return Err(format!(
                "conversation adapter package artifact size mismatch: expected {expected_size}, got {}",
                bytes.len()
            ));
        }
    }
    let actual_hash = format!("{:x}", Sha256::digest(&bytes));
    if !actual_hash.eq_ignore_ascii_case(&expected_hash) {
        return Err("conversation adapter package artifact hash mismatch".to_string());
    }

    extract_catalog_artifact_bytes(item, bytes, staging_dir)
}

fn extract_catalog_artifact_bytes(
    item: &ConversationScriptCatalogItem,
    bytes: Vec<u8>,
    staging_dir: &Path,
) -> AppResult<PathBuf> {
    let extract_root = staging_dir.join("extracted");
    fs::create_dir_all(&extract_root).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("open conversation adapter package artifact failed: {error}"))?;
    if archive.len() > 10_000 {
        return Err("conversation adapter package artifact contains too many entries".to_string());
    }
    let mut extracted_size = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            format!("read conversation adapter package artifact entry failed: {error}")
        })?;
        let enclosed = entry.enclosed_name().ok_or_else(|| {
            "conversation adapter package artifact contains an unsafe path".to_string()
        })?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(
                "conversation adapter package artifact must not contain symlinks".to_string(),
            );
        }
        extracted_size = extracted_size.saturating_add(entry.size());
        if extracted_size > 1024 * 1024 * 1024 {
            return Err("conversation adapter package artifact expands beyond 1 GiB".to_string());
        }
        let destination = extract_root.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut output = fs::File::create(&destination).map_err(|error| error.to_string())?;
        std::io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&destination, fs::Permissions::from_mode(mode & 0o777))
                .map_err(|error| error.to_string())?;
        }
    }

    let package_manifest = item.package_manifest_file_name()?;
    if extract_root.join(&package_manifest).is_file() {
        return Ok(extract_root);
    }
    let candidates = fs::read_dir(&extract_root)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join(&package_manifest).is_file())
        .collect::<Vec<_>>();
    if candidates.len() == 1 {
        Ok(candidates[0].clone())
    } else {
        Err("conversation adapter package artifact must contain one package root".to_string())
    }
}

fn persist_failed_conversation_adapter_package(
    service: &AppService,
    item: &ConversationScriptCatalogItem,
    current_dir: &Path,
    error: &str,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    let package = ConversationAdapterPackage {
        package_id: item.package_id().to_string(),
        adapter_id: item.adapter_key().to_string(),
        name: item.name.clone(),
        version: item.version.clone(),
        record_kind: item.record_kind.as_package_record_kind(),
        install_dir: current_dir.to_string_lossy().to_string(),
        manifest_path: current_dir
            .join(item.package_manifest_file_name()?)
            .to_string_lossy()
            .to_string(),
        adapter_manifest_path: current_dir
            .join(item.manifest_file_name()?)
            .to_string_lossy()
            .to_string(),
        runtime_protocol: "stdio-ndjson-v1".to_string(),
        runtime_ready: false,
        origin: ConversationAdapterPackageOrigin::ManagedRelease,
        source_url: Some(item.source.url.clone()),
        git_ref: item.source.branch.clone(),
        git_commit: None,
        catalog_url: None,
        update_policy: ConversationPackageUpdatePolicy::Manual,
        latest_version: Some(item.version.clone()),
        last_checked_at: Some(now.clone()),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::ManifestInvalid,
        runtime_validated_at: Some(now.clone()),
        installed_content_hash: None,
        trusted_package_hash: item
            .expected_package_hash
            .as_deref()
            .and_then(clean_non_empty_string),
        error_message: Some(error.to_string()),
        created_at: now.clone(),
        updated_at: now,
    };
    service.save_conversation_adapter_package(&package)
}

fn validate_installed_package_for_catalog_item(
    item: &ConversationScriptCatalogItem,
    validation: &crate::backend::conversations::ConversationAdapterPackageValidationResult,
) -> AppResult<()> {
    if validation.manifest.package_id != item.package_id() {
        return Err(format!(
            "installed package id {} does not match catalog package id {}",
            validation.manifest.package_id,
            item.package_id()
        ));
    }
    if validation.manifest.version != item.version {
        return Err(format!(
            "installed package version {} does not match catalog version {}",
            validation.manifest.version, item.version
        ));
    }
    if validation.manifest.record_kind != item.record_kind.as_package_record_kind() {
        return Err(format!(
            "installed package record kind does not match catalog item: {}",
            item.id
        ));
    }
    if validation.manifest.runtime.protocol
        != crate::backend::conversations::ConversationAdapterPackageRuntimeProtocol::StdioNdjsonV1
    {
        return Err(format!(
            "conversation adapter package {} only supports stdio-ndjson-v1 in this release",
            item.id
        ));
    }
    validate_installed_manifest_for_catalog_item(item, &validation.adapter_validation)?;
    if let Some(expected) = item
        .expected_package_hash
        .as_deref()
        .and_then(clean_non_empty_string)
    {
        if validation.content_hash != expected {
            return Err(format!(
                "conversation adapter package {} content hash mismatch",
                item.id
            ));
        }
    }
    Ok(())
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
    pub(crate) runtime_ready: bool,
    pub(crate) status: String,
    pub(crate) installed_package: Option<ConversationAdapterPackage>,
    pub(crate) installed_adapter: Option<ConversationAdapter>,
    pub(crate) install_path: Option<String>,
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

impl GitHubCatalogLocation {
    fn source_dir(&self, staging_dir: &Path) -> PathBuf {
        self.path
            .as_deref()
            .map(|path| staging_dir.join(path))
            .unwrap_or_else(|| staging_dir.to_path_buf())
    }
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
        format!("conversation adapter package catalog response was not text: {error}")
    })
}

fn read_local_default_catalog() -> AppResult<String> {
    Ok(LOCAL_DEFAULT_CONVERSATION_SCRIPT_CATALOG.to_string())
}

fn validate_conversation_script_catalog(catalog: &ConversationScriptCatalog) -> AppResult<()> {
    if catalog.schema_version != 1 {
        return Err("conversation adapter package catalog schema_version must be 1".to_string());
    }
    let mut seen_ids = HashSet::new();
    for item in &catalog.items {
        validate_conversation_script_catalog_item(item)?;
        if !seen_ids.insert(item.id.clone()) {
            return Err(format!(
                "duplicate conversation adapter package catalog item: {}",
                item.id
            ));
        }
    }
    Ok(())
}

fn validate_conversation_script_catalog_item(
    item: &ConversationScriptCatalogItem,
) -> AppResult<()> {
    if item.id.trim().is_empty() {
        return Err("conversation adapter package catalog item id is required".to_string());
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
        return Err(format!(
            "conversation adapter package catalog item id must be a safe path segment: {}",
            item.id
        ));
    }
    if item.name.trim().is_empty() {
        return Err(format!(
            "conversation adapter package catalog item name is required: {}",
            item.id
        ));
    }
    if item.version.trim().is_empty() {
        return Err(format!(
            "conversation adapter package catalog item version is required: {}",
            item.id
        ));
    }
    validated_package_version(&item.version)?;
    if let Some(adapter_id) = item.adapter_id.as_deref() {
        if adapter_id.trim().is_empty() {
            return Err(format!(
                "conversation adapter package catalog item adapter_id must not be empty: {}",
                item.id
            ));
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
                });
            let installed = installed_package.is_some() || installed_adapter.is_some();
            let update_available = installed_package
                .as_ref()
                .map(|package| package.version != item.version)
                .or_else(|| {
                    installed_adapter
                        .as_ref()
                        .map(|adapter| adapter.version != item.version)
                })
                .unwrap_or(false);
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
                runtime_ready,
                installed_adapter.as_ref(),
            );
            ConversationAdapterPackageCatalogEntry {
                item,
                installed,
                update_available,
                runtime_ready,
                status,
                installed_package,
                installed_adapter,
                install_path,
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
        let update_available = semver::Version::parse(&latest_version)
            .ok()
            .zip(semver::Version::parse(&package.version).ok())
            .is_some_and(|(latest, current)| latest > current);
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
                package.runtime_ready,
                adapter.as_ref(),
            ),
            installed: true,
            update_available,
            runtime_ready: package.runtime_ready,
            install_path: Some(package.install_dir.clone()),
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
            runtime_ready: adapter.enabled,
            status: conversation_adapter_package_status(
                true,
                None,
                false,
                adapter.enabled,
                Some(adapter),
            ),
            installed_package: None,
            installed_adapter: Some(adapter.clone()),
            install_path,
            error_message: None,
        });
    }
    entries.sort_by(|left, right| left.item.name.cmp(&right.item.name));
    entries
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
        return "built_in".to_string();
    } else if adapter.is_some() {
        return "legacy_installed".to_string();
    }
    if update_available {
        return "update_available".to_string();
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

fn validate_installed_manifest_for_catalog_item(
    item: &ConversationScriptCatalogItem,
    validation: &crate::backend::conversations::ExternalAdapterValidationResult,
) -> AppResult<()> {
    if validation.manifest.id != item.adapter_key() {
        return Err(format!(
            "installed adapter id {} does not match catalog adapter id {}",
            validation.manifest.id,
            item.adapter_key()
        ));
    }
    if !validation
        .manifest
        .capabilities
        .iter()
        .any(|capability| capability == "read_session")
    {
        return Err(format!(
            "conversation adapter package {} must declare read_session",
            item.id
        ));
    }
    if item.record_kind == ConversationScriptRecordKind::Web
        && !validation
            .manifest
            .capabilities
            .iter()
            .any(|capability| capability == "web_records")
    {
        return Err(format!(
            "web conversation adapter package {} must declare web_records",
            item.id
        ));
    }
    if let Some(expected) = item
        .expected_content_hash
        .as_deref()
        .and_then(clean_non_empty_string)
    {
        if validation.content_hash != expected {
            return Err(format!(
                "conversation adapter {} content hash mismatch",
                item.id
            ));
        }
    }
    Ok(())
}

fn conversation_adapter_package_dir(item: &ConversationScriptCatalogItem) -> AppResult<PathBuf> {
    Ok(crate::backend::app_settings::conversation_adapter_dir()?
        .join("packages")
        .join(item.package_id()))
}

fn conversation_adapter_package_version_dir(
    item: &ConversationScriptCatalogItem,
) -> AppResult<PathBuf> {
    Ok(conversation_adapter_package_dir(item)?
        .join("versions")
        .join(validated_package_version(&item.version)?))
}

fn validated_package_version(value: &str) -> AppResult<String> {
    semver::Version::parse(value.trim())
        .map(|version| version.to_string())
        .map_err(|error| format!("conversation adapter package version must be SemVer: {error}"))
}

fn conversation_adapter_package_prepared_dir(
    item: &ConversationScriptCatalogItem,
) -> AppResult<PathBuf> {
    Ok(conversation_adapter_package_dir(item)?
        .join("prepared")
        .join(short_uuid()))
}

fn conversation_script_staging_dir(item: &ConversationScriptCatalogItem) -> AppResult<PathBuf> {
    Ok(crate::backend::app_settings::conversation_adapter_dir()?
        .join("staging")
        .join(format!(
            "{}-{}",
            slug_path_segment(item.package_id()),
            short_uuid()
        )))
}

fn parse_github_catalog_location(
    source: &ConversationScriptCatalogSource,
) -> AppResult<GitHubCatalogLocation> {
    if source.kind != ConversationScriptCatalogSourceKind::Github {
        return Err("conversation adapter package source must be github".to_string());
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
        return Err("GitHub URL must include owner and repository".to_string());
    }

    let owner = parts[0];
    let repo = parts[1].trim_end_matches(".git");
    if repo.is_empty() {
        return Err("GitHub URL must include repository name".to_string());
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

fn clone_github_catalog_source(location: &GitHubCatalogLocation, target: &Path) -> AppResult<()> {
    if target.exists() {
        return Err(format!(
            "conversation adapter package staging path already exists: {}",
            target.display()
        ));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let mut command = Command::new("git");
    command.arg("clone").arg("--depth").arg("1");
    if let Some(branch) = &location.branch {
        command.arg("--branch").arg(branch);
    }
    let output = command
        .arg(&location.repo_url)
        .arg(target)
        .output()
        .map_err(|error| format!("failed to run git clone: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git clone failed: {stderr}"));
    }
    Ok(())
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
        return Err(format!("manifest_file must be a file name: {value}"));
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
                url: "https://github.com/util6/assetiweave/tree/main/src-tauri/bundled/conversation-adapters/codex".to_string(),
                branch: None,
                path: None,
            },
        }
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
        assert_eq!(entries[0].status, "update_available");
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
            url: "https://github.com/util6/assetiweave/tree/main/parser-catalog/adapters/codex"
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
            Some("parser-catalog/adapters/codex"),
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

        let result = extract_catalog_artifact_bytes(&item, bytes, &root.join("staging"));

        assert!(result.is_err());
        assert!(!root.join("escape.txt").exists());
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
