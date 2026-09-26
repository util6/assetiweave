use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::infrastructure::tasks::{StageStatus, TaskActivity, TaskOutcome, TaskStage};

pub(crate) fn conversation_storage_error<E: Into<AppError>>(error: E) -> AppError {
    error.into()
}

pub(crate) fn conversation_external_error(error: impl std::fmt::Display) -> AppError {
    AppError::external(error)
}

impl AppService {
    pub(crate) async fn conversation_payload_policy_reparse_required(&self) -> AppResult<bool> {
        crate::backend::store::conversation_payload_policy_reparse_required_sqlx(
            self.db.pool(),
            self.tenant_id(),
            crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        )
        .await
        .map_err(conversation_storage_error)
    }

    pub(crate) fn list_conversation_adapters(&self) -> AppResult<Vec<ConversationAdapter>> {
        let current = self.runtime.context();
        let catalog = if current.tenant.id == self.tenant_id() {
            current.conversation_adapter_catalog.clone()
        } else {
            self.conversation_adapter_catalog.clone()
        };
        Ok(catalog.adapters.clone())
    }

    pub(crate) fn scaffold_conversation_adapter(
        &self,
        params: crate::backend::infrastructure::conversations::ExternalAdapterScaffoldParams,
    ) -> AppResult<crate::backend::infrastructure::conversations::ExternalAdapterScaffoldResult>
    {
        crate::backend::infrastructure::conversations::scaffold_external_adapter(params)
            .map_err(conversation_external_error)
    }

    pub(crate) fn validate_conversation_adapter(
        &self,
        params: crate::backend::infrastructure::conversations::ExternalAdapterValidateParams,
    ) -> AppResult<crate::backend::infrastructure::conversations::ExternalAdapterValidationResult>
    {
        Ok(crate::backend::infrastructure::conversations::validate_external_adapter(params)?)
    }

    pub(crate) async fn list_conversation_adapter_runtime_statuses(
        &self,
    ) -> AppResult<
        Vec<crate::backend::infrastructure::conversations::ConversationAdapterRuntimeStatus>,
    > {
        let adapters = self.list_conversation_adapters()?;
        let sources = self.list_conversation_sources().await?;
        let settings = self.app_settings_value();
        crate::backend::infrastructure::conversations::list_conversation_adapter_runtime_statuses_with_settings(
            &adapters, &sources, &settings,
        )
        .await
        .map_err(conversation_external_error)
    }

    pub(crate) async fn register_conversation_adapter(
        &self,
        params: crate::backend::infrastructure::conversations::ExternalAdapterRegisterParams,
    ) -> AppResult<Value> {
        let dry_run = params.dry_run;
        let settings = self.app_settings_value();
        let preview =
            crate::backend::infrastructure::conversations::register_external_adapter_with_settings(
                params, &settings,
            )
            .await
            .map_err(conversation_external_error)?;
        let mut adapter =
            crate::backend::infrastructure::conversations::adapter_from_registration_preview(
                preview.clone(),
            )
            .map_err(|error| error)?;
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let existing = super::conversation_storage::load_adapter(pool, tenant_id, &adapter.id)
            .await
            .map_err(conversation_storage_error)?;
        let reactivating_builtin = existing.as_ref().is_some_and(|existing| {
            existing.trust_state == crate::backend::domain::ConversationAdapterTrustState::BuiltIn
        });
        if reactivating_builtin {
            adapter.trust_state = crate::backend::domain::ConversationAdapterTrustState::BuiltIn;
            adapter.enabled = true;
        }
        let preflight = self
            .prepare_conversation_adapter_package_change(ConversationAdapterPackageChangeParams {
                action: crate::backend::domain::ConversationAdapterPackageChangeAction::Register,
                package_id: None,
                adapter_id: Some(adapter.id.clone()),
            })
            .await?;
        if !preflight.task_conflicts.is_empty() {
            return Err(AppError::Conflict(format!(
                "conversation adapter registration conflicts with running tasks: {}",
                preflight.task_conflicts.join(", ")
            )));
        }
        if !dry_run {
            super::conversation_storage::save_adapter(pool, tenant_id, &adapter)
                .await
                .map_err(conversation_storage_error)?;
            if reactivating_builtin {
                crate::backend::store::enable_conversation_sources_by_adapter_sqlx(
                    pool,
                    tenant_id,
                    &adapter.id,
                )
                .await
                .map_err(conversation_storage_error)?;
            }
            self.runtime.refresh_conversation_adapter_catalog().await?;
        }
        Ok(preview)
    }

    pub(crate) async fn unregister_conversation_adapter(
        &self,
        params: ConversationAdapterUnregisterParams,
    ) -> AppResult<Value> {
        let preflight = self
            .prepare_conversation_adapter_package_change(ConversationAdapterPackageChangeParams {
                action: crate::backend::domain::ConversationAdapterPackageChangeAction::Unregister,
                package_id: None,
                adapter_id: Some(params.adapter_id.clone()),
            })
            .await?;
        if !preflight.task_conflicts.is_empty() {
            return Err(AppError::Conflict(format!(
                "conversation adapter unregister conflicts with running tasks: {}",
                preflight.task_conflicts.join(", ")
            )));
        }
        if !params.dry_run && !params.yes {
            return Err(AppError::Validation(
                "conversation.adapter.unregister requires --yes".to_string(),
            ));
        }
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let adapter =
            super::conversation_storage::load_adapter(pool, tenant_id, &params.adapter_id)
                .await
                .map_err(conversation_storage_error)?
                .ok_or_else(|| {
                    AppError::NotFound(format!(
                        "conversation adapter not found: {}",
                        params.adapter_id
                    ))
                })?;
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "unregistered": false,
                "adapter": adapter,
                "preflight": preflight
            }));
        }
        if adapter.trust_state == crate::backend::domain::ConversationAdapterTrustState::BuiltIn {
            let adapter = crate::backend::store::disable_builtin_conversation_adapter_sqlx(
                pool,
                tenant_id,
                &params.adapter_id,
            )
            .await
            .map_err(conversation_storage_error)?;
            self.runtime.refresh_conversation_adapter_catalog().await?;
            return Ok(json!({
                "dry_run": false,
                "unregistered": false,
                "disabled": true,
                "adapter": adapter
            }));
        }
        let adapter = crate::backend::store::delete_conversation_adapter_registration_sqlx(
            pool,
            tenant_id,
            &params.adapter_id,
            preflight.package_id.as_deref(),
        )
        .await
        .map_err(conversation_storage_error)?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "conversation adapter not found: {}",
                params.adapter_id
            ))
        })?;
        self.runtime.refresh_conversation_adapter_catalog().await?;
        Ok(json!({
            "dry_run": false,
            "unregistered": true,
            "adapter": adapter
        }))
    }

    pub(crate) async fn try_run_conversation_adapter(
        &self,
        params: crate::backend::infrastructure::conversations::ExternalAdapterTryRunParams,
    ) -> AppResult<crate::backend::infrastructure::conversations::ExternalAdapterRunResult> {
        let settings = self.app_settings_value();
        crate::backend::infrastructure::conversations::try_run_external_adapter_with_settings(
            params, &settings,
        )
        .await
        .map_err(conversation_external_error)
    }

    pub(crate) async fn project_conversation_command_parts(
        &self,
        params: crate::backend::infrastructure::conversations::ConversationCommandProjectionParams,
    ) -> AppResult<Vec<crate::backend::infrastructure::conversations::ConversationCommandProjection>>
    {
        let adapter = self
            .list_conversation_adapters()?
            .into_iter()
            .find(|adapter| adapter.id == params.adapter_id)
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "conversation adapter not found: {}",
                    params.adapter_id
                ))
            })?;
        let supports_adapter_projection = adapter
            .capabilities
            .iter()
            .any(|capability| capability == "project_command_parts");
        let settings = self.app_settings_value();
        if supports_adapter_projection {
            match crate::backend::infrastructure::conversations::project_external_adapter_command_parts_with_settings(
                &adapter,
                &params.parts,
                &settings,
            )
            .await
            {
                Ok(projections) => return Ok(projections),
                Err(error) => tracing::warn!(
                    action = "conversation.command_projection.fallback",
                    adapter_id = %adapter.id,
                    error = %error,
                    "adapter command projector failed; using the core projector"
                ),
            }
        }

        let core_projector =
            crate::backend::infrastructure::conversations::ensure_shell_command_projector()?;
        crate::backend::infrastructure::conversations::project_external_adapter_command_parts_with_settings(
            &core_projector,
            &params.parts,
            &settings,
        )
        .await
        .map_err(conversation_external_error)
    }

    pub(crate) async fn list_conversation_sources(&self) -> AppResult<Vec<ConversationSource>> {
        super::conversation_sources::list_sources(self.db.pool(), self.tenant_id())
            .await
            .map_err(conversation_storage_error)
    }

    pub(crate) async fn upsert_conversation_source(
        &self,
        params: ConversationSourceUpsertParams,
    ) -> AppResult<Value> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        if super::conversation_storage::load_adapter(pool, tenant_id, &params.source.adapter_id)
            .await
            .map_err(conversation_storage_error)?
            .is_none()
        {
            return Err(AppError::NotFound(format!(
                "conversation adapter not found: {}",
                params.source.adapter_id
            )));
        }
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "source": params.source
            }));
        }
        super::conversation_sources::save_source(pool, tenant_id, &params.source)
            .await
            .map_err(conversation_storage_error)?;
        Ok(json!({
            "dry_run": false,
            "source": params.source
        }))
    }

    pub(crate) async fn disable_conversation_source(
        &self,
        params: ConversationSourceDisableParams,
    ) -> AppResult<Value> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let source = super::conversation_sources::load_source(pool, tenant_id, &params.id)
            .await
            .map_err(conversation_storage_error)?
            .ok_or_else(|| {
                AppError::NotFound(format!("conversation source not found: {}", params.id))
            })?;
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "disabled": false,
                "source": source
            }));
        }
        let source = super::conversation_sources::disable_source(pool, tenant_id, &params.id)
            .await
            .map_err(conversation_storage_error)?;
        Ok(json!({
            "dry_run": false,
            "disabled": true,
            "source": source
        }))
    }
}

#[cfg(test)]
#[path = "conversation_adapters_tests.rs"]
mod tests;
