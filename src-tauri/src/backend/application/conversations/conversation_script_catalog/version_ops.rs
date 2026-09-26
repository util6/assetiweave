use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::{
    ConversationAdapterPackageChangeAction, ConversationAdapterPackageChangeRisk,
    ConversationAdapterPackageOrigin, ConversationAdapterPackageRecordKind,
    ConversationAdapterRuntimeGateStatus, ConversationPackageUpdatePolicy,
};
use crate::backend::infrastructure::conversations::{
    ConversationAdapterPackageInstallSourceKind, ConversationAdapterPackageInstallSpec,
};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};

impl AppService {
    pub(crate) async fn switch_conversation_adapter_package_version(
        &self,
        params: ConversationAdapterPackageVersionChangeParams,
    ) -> AppResult<Value> {
        let version = params
            .version
            .as_deref()
            .and_then(clean_non_empty_string)
            .ok_or_else(|| "conversation adapter package version is required".to_string())
            .map_err(AppError::external)?;
        self.activate_installed_conversation_adapter_package_version(
            &params.package_id,
            &version,
            ConversationAdapterPackageChangeAction::SwitchVersion,
            params.dry_run,
            params.yes,
        )
        .await
    }

    pub(crate) async fn rollback_conversation_adapter_package_version(
        &self,
        params: ConversationAdapterPackageVersionChangeParams,
    ) -> AppResult<Value> {
        let package = self
            .load_conversation_adapter_package(params.package_id.trim())
            .await?
            .ok_or_else(|| "conversation adapter package not found".to_string())
            .map_err(AppError::external)?;
        let versions = self
            .load_conversation_adapter_package_versions(&package.package_id)
            .await?;
        let target = select_rollback_version(&versions, &package.version)
            .ok_or_else(|| "no inactive installed version is available for rollback".to_string())
            .map_err(AppError::external)?;
        self.activate_installed_conversation_adapter_package_version(
            &package.package_id,
            &target.version,
            ConversationAdapterPackageChangeAction::Rollback,
            params.dry_run,
            params.yes,
        )
        .await
    }

    pub(crate) async fn delete_conversation_adapter_package_version(
        &self,
        params: ConversationAdapterPackageVersionChangeParams,
    ) -> AppResult<Value> {
        let package_id = params.package_id.trim();
        let version = params
            .version
            .as_deref()
            .and_then(clean_non_empty_string)
            .ok_or_else(|| "conversation adapter package version is required".to_string())
            .map_err(AppError::external)?;
        let package = self
            .load_conversation_adapter_package(package_id)
            .await?
            .ok_or_else(|| format!("conversation adapter package not found: {package_id}"))
            .map_err(AppError::external)?;
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
        let versions = self
            .load_conversation_adapter_package_versions(package_id)
            .await?;
        let target = versions
            .iter()
            .find(|candidate| candidate.version == version)
            .cloned()
            .ok_or_else(|| format!("installed package version not found: {package_id}@{version}"))
            .map_err(AppError::external)?;
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
        let managed_root =
            crate::backend::infrastructure::app_settings::conversation_adapter_dir()?;
        let target_install_dir =
            crate::backend::infrastructure::path_utils::expand_path(&target.install_dir)?;
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
        fs::rename(&version_dir, &staged).map_err(AppError::external)?;
        let deleted = crate::backend::store::delete_conversation_adapter_package_version_sqlx(
            self.db.pool(),
            package_id,
            &version,
            replacement_package.as_ref(),
            delete_package,
        )
        .await;
        match deleted {
            Ok(true) => fs::remove_dir_all(&staged).map_err(AppError::external)?,
            Ok(false) => {
                let _ = fs::rename(&staged, &version_dir);
                return Err(AppError::Validation(
                    "installed package version record was not found".to_string(),
                ));
            }
            Err(error) => {
                let _ = fs::rename(&staged, &version_dir);
                return Err(AppError::from(error));
            }
        }
        self.runtime.refresh_conversation_adapter_catalog().await?;
        Ok(json!({
            "dry_run": false,
            "deleted": true,
            "package_id": package_id,
            "version": version,
            "package_removed": delete_package,
            "replacement_version": replacement_package.map(|package| package.version)
        }))
    }

    pub(crate) async fn activate_installed_conversation_adapter_package_version(
        &self,
        package_id: &str,
        version: &str,
        action: ConversationAdapterPackageChangeAction,
        dry_run: bool,
        yes: bool,
    ) -> AppResult<Value> {
        let preflight = self
            .prepare_conversation_adapter_package_change(ConversationAdapterPackageChangeParams {
                action,
                package_id: Some(package_id.to_string()),
                adapter_id: None,
            })
            .await?;
        reject_conversation_package_task_conflicts(&preflight)?;
        if !dry_run && !yes {
            return Err(AppError::Validation(
                "conversation adapter package version activation requires --yes".to_string(),
            ));
        }
        let mut package = self
            .load_conversation_adapter_package(package_id)
            .await?
            .ok_or_else(|| format!("conversation adapter package not found: {package_id}"))
            .map_err(AppError::external)?;
        if package.origin != ConversationAdapterPackageOrigin::ManagedRelease {
            return Err(AppError::Validation(
                "only managed package versions can be activated".to_string(),
            ));
        }
        let versions = self
            .load_conversation_adapter_package_versions(package_id)
            .await?;
        let target = versions
            .iter()
            .find(|candidate| candidate.version == version)
            .ok_or_else(|| format!("installed package version not found: {package_id}@{version}"))
            .map_err(AppError::external)?;
        let target_install_dir =
            crate::backend::infrastructure::path_utils::expand_path(&target.install_dir)?;
        let validation = crate::backend::infrastructure::conversations::validate_conversation_adapter_package_dir(
            &target_install_dir,
        )
        .map_err(AppError::external)?;
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
        let settings = self.app_settings_value();
        let preview =
            crate::backend::infrastructure::conversations::register_external_adapter_with_settings(
                crate::backend::infrastructure::conversations::ExternalAdapterRegisterParams {
                    manifest_path: validation.adapter_manifest_path.clone(),
                    dry_run: false,
                    yes: true,
                },
                &settings,
            )
            .await
            .map_err(AppError::external)?;
        let adapter =
            crate::backend::infrastructure::conversations::adapter_from_registration_preview(
                preview,
            )
            .map_err(AppError::external)?;
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
        super::super::conversation_storage::activate_package(
            self.db.pool(),
            &adapter,
            &package,
            target,
        )
        .await
        .map_err(AppError::external)?;
        self.runtime.refresh_conversation_adapter_catalog().await?;
        Ok(
            json!({"dry_run": false, "activated": true, "package_id": package_id, "version": version}),
        )
    }

    pub(crate) async fn ensure_conversation_adapter_package_runtime_ready(
        &self,
        adapter: &ConversationAdapter,
    ) -> AppResult<()> {
        let Some(mut package) = self
            .load_conversation_adapter_package_by_adapter(&adapter.id)
            .await?
        else {
            return Ok(());
        };
        self.refresh_conversation_adapter_package_runtime(&mut package, Some(adapter))
            .await?;
        if package.runtime_ready {
            Ok(())
        } else {
            Err(AppError::External(format_package_not_ready_error(&package)))
        }
    }

    pub(crate) async fn refresh_conversation_adapter_package_runtime(
        &self,
        package: &mut ConversationAdapterPackage,
        adapter: Option<&ConversationAdapter>,
    ) -> AppResult<()> {
        let install_dir =
            crate::backend::infrastructure::path_utils::expand_path(&package.install_dir)?;
        let is_official = crate::backend::infrastructure::conversations::is_official_adapter_id(
            &package.adapter_id,
        );
        if is_official {
            let _ =
                crate::backend::infrastructure::conversations::sync_official_adapter_to_package_dir(
                    &package.adapter_id,
                    &install_dir,
                );
        }
        let evaluated = crate::backend::infrastructure::conversations::validate_conversation_adapter_package_dir(
            &install_dir,
        )
        .map_err(AppError::from)
        .and_then(|validation| {
            if validation.manifest.package_id != package.package_id {
                return Err(AppError::Validation(format!(
                    "conversation adapter package manifest id {} does not match registered package {}",
                    validation.manifest.package_id, package.package_id
                )));
            }
            if validation.manifest.version != package.version {
                if is_official {
                    package.version = validation.manifest.version.clone();
                } else {
                    return Err(AppError::Validation(format!(
                        "conversation adapter package manifest version {} does not match active version {}",
                        validation.manifest.version, package.version
                    )));
                }
            }
            let adapter = adapter.ok_or_else(|| {
                AppError::NotFound(format!(
                    "conversation adapter runtime is not registered: {}",
                    package.adapter_id
                ))
            })?;
            if validation.adapter_validation.manifest.id != adapter.id {
                return Err(AppError::Validation(format!(
                    "conversation adapter package {} manifest adapter id {} does not match registered adapter {}",
                    package.package_id, validation.adapter_validation.manifest.id, adapter.id
                )));
            }
            if package.origin != ConversationAdapterPackageOrigin::DevOverride {
                if is_official {
                    package.trusted_package_hash = Some(validation.content_hash.clone());
                    package.installed_content_hash = Some(validation.content_hash.clone());
                } else {
                    let trusted_hash = package
                        .trusted_package_hash
                        .as_deref()
                        .or(package.installed_content_hash.as_deref())
                        .ok_or_else(|| {
                            AppError::Validation(format!(
                                "conversation adapter package has no trusted hash: {}",
                                package.package_id
                            ))
                        })?;
                    if validation.content_hash != trusted_hash {
                        return Err(AppError::Validation(format!(
                            "conversation adapter package content hash mismatch: {}",
                            package.package_id
                        )));
                    }
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
                let error_message = error.to_string();
                package.runtime_ready = false;
                package.runtime_gate_status =
                    classify_runtime_gate_error(&install_dir, &error_message);
                package.error_message = Some(error_message);
            }
        }
        package.runtime_validated_at = Some(now.clone());
        package.updated_at = now;
        self.save_conversation_adapter_package(package).await
    }
}
