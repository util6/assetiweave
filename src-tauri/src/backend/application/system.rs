use super::prelude::*;
use crate::backend::runtime::{AppError, AppResult};

impl AppService {
    pub(crate) async fn for_tenant(&self, tenant_id: &str) -> AppResult<Self> {
        let tenant = crate::backend::store::load_tenant_sqlx(self.db.pool(), tenant_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("tenant not found: {tenant_id}")))?;
        let membership = crate::backend::store::load_tenant_membership_sqlx(
            self.db.pool(),
            tenant_id,
            &self.request_context().principal.id,
        )
        .await?
        .ok_or_else(|| AppError::Conflict(format!("tenant membership not found: {tenant_id}")))?;
        let adapters =
            crate::backend::store::list_conversation_adapters_sqlx(self.db.pool(), tenant_id)
                .await?;
        let mut context = self.context.clone();
        context.tenant = tenant;
        context.membership = membership;
        Ok(Self {
            runtime: self.runtime.clone(),
            db: self.db.clone(),
            db_path: self.db_path.clone(),
            context,
            agent_runtime_manager: self.agent_runtime_manager.clone(),
            agent_runtime: self.agent_runtime.clone(),
            conversation_adapter_catalog: std::sync::Arc::new(
                crate::backend::conversations::ConversationAdapterCatalog::new(adapters),
            ),
        })
    }

    pub(crate) async fn open_for_engine() -> AppResult<Self> {
        if let Some(runtime) = crate::backend::runtime::current_process_runtime() {
            return Ok(Self::from_runtime(&runtime));
        }

        let config = crate::backend::runtime::RuntimeConfig::from_environment()?;
        let runtime = crate::backend::runtime::AppRuntime::bootstrap(
            config.db_path,
            crate::backend::runtime::RuntimeRole::OneShot,
        )
        .await?;
        Ok(Self::from_runtime(&runtime))
    }

    /// Bind a request to the process-level runtime without I/O.
    pub(crate) fn from_runtime(
        runtime: &std::sync::Arc<crate::backend::runtime::AppRuntime>,
    ) -> Self {
        let snapshot = runtime.context();
        Self {
            runtime: runtime.clone(),
            db: runtime.db().clone(),
            db_path: runtime.db_path().to_path_buf(),
            context: snapshot.request_context.clone(),
            agent_runtime_manager: snapshot.agent_runtime_manager.clone(),
            agent_runtime: snapshot.agent_runtime.clone(),
            conversation_adapter_catalog: snapshot.conversation_adapter_catalog.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) async fn open_with_db_path(db_path: PathBuf) -> AppResult<Self> {
        let runtime = crate::backend::runtime::AppRuntime::bootstrap(
            db_path,
            crate::backend::runtime::RuntimeRole::OneShot,
        )
        .await?;
        Ok(Self::from_runtime(&runtime))
    }

    #[cfg(test)]
    pub(crate) async fn open_with_db_path_and_runtime(
        db_path: PathBuf,
        agent_runtime: std::sync::Arc<dyn crate::backend::ai_execution::AgentExecutionRuntime>,
    ) -> AppResult<Self> {
        let service = Self::open_with_db_path(db_path).await?;
        let runtime = crate::backend::runtime::AppRuntime::for_test(
            service.db_path.clone(),
            service.db.clone(),
            service.context.clone(),
            service.agent_runtime_manager.clone(),
            agent_runtime.clone(),
        )
        .await;
        Ok(Self {
            runtime: runtime.clone(),
            db: service.db,
            db_path: service.db_path,
            context: service.context,
            agent_runtime_manager: service.agent_runtime_manager,
            agent_runtime,
            conversation_adapter_catalog: runtime.conversation_adapter_catalog(),
        })
    }

    pub(crate) fn request_context(&self) -> &RequestContext {
        &self.context
    }

    pub(crate) fn tenant_id(&self) -> &str {
        &self.context.tenant.id
    }

    pub(crate) fn pool(&self) -> &sqlx::SqlitePool {
        self.db.pool()
    }

    pub(crate) async fn overview(&self) -> AppResult<AppOverview> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        Ok(AppOverview {
            source_count: crate::backend::store::count_rows_sqlx(pool, tenant_id, "sources")
                .await
                .map_err(AppError::external)?,
            asset_count: crate::backend::store::count_rows_sqlx(pool, tenant_id, "assets")
                .await
                .map_err(AppError::external)?,
            profile_count: crate::backend::store::count_rows_sqlx(pool, tenant_id, "profiles")
                .await
                .map_err(AppError::external)?,
            last_scan_status: crate::backend::store::latest_scan_status_sqlx(pool, tenant_id)
                .await
                .map_err(AppError::external)?,
        })
    }

    pub(crate) fn list_target_profile_descriptors(
        &self,
    ) -> AppResult<Vec<crate::backend::models::TargetProfileDescriptor>> {
        Ok(self.runtime.target_catalog().descriptors().to_vec())
    }

    pub(crate) async fn refresh_target_profile_descriptors(
        &self,
    ) -> AppResult<Vec<crate::backend::models::TargetProfileDescriptor>> {
        Ok(self
            .runtime
            .refresh_target_catalog_from_disk()
            .await?
            .descriptors()
            .to_vec())
    }

    pub(crate) fn logs_get_snapshot(
        &self,
        file_name: Option<String>,
        line_limit: Option<usize>,
    ) -> AppResult<crate::backend::logs::LogSnapshot> {
        Ok(crate::backend::logs::logs_get_snapshot(
            file_name, line_limit,
        )?)
    }

    pub(crate) fn logs_open_log_directory(&self) -> AppResult<()> {
        Ok(crate::backend::logs::logs_open_log_directory()?)
    }

    pub(crate) fn logs_write_operation(
        &self,
        level: String,
        operation: String,
        message: String,
        fields: Option<BTreeMap<String, String>>,
    ) -> AppResult<()> {
        Ok(crate::backend::logs::logs_write_operation(
            level, operation, message, fields,
        )?)
    }

    pub(crate) fn app_settings_value(&self) -> Value {
        self.runtime.app_settings_value()
    }

    pub(crate) async fn get_app_settings(
        &self,
    ) -> AppResult<crate::backend::app_settings::AppSettingsFile> {
        let file = crate::backend::app_settings::get_app_settings_sqlx(self.db.pool()).await?;
        self.runtime
            .update_app_settings_value(file.settings.clone());
        Ok(file)
    }

    pub(crate) async fn save_app_settings(
        &self,
        settings: Value,
    ) -> AppResult<crate::backend::app_settings::AppSettingsFile> {
        self.validate_agent_capability_assignments(&settings)
            .await?;
        self.validate_memory_settings(&settings).await?;
        let file =
            crate::backend::app_settings::save_app_settings_sqlx(self.db.pool(), settings).await?;
        self.runtime
            .update_app_settings_value(file.settings.clone());
        Ok(file)
    }

    pub(crate) async fn initialize_app_locale_if_unset(
        &self,
        locale: crate::backend::app_settings::AppLocale,
    ) -> AppResult<crate::backend::app_settings::AppSettingsFile> {
        let file = crate::backend::app_settings::initialize_app_locale_sqlx(self.db.pool(), locale)
            .await?;
        self.runtime
            .update_app_settings_value(file.settings.clone());
        Ok(file)
    }

    async fn validate_agent_capability_assignments(&self, settings: &Value) -> AppResult<()> {
        let Some(assignments) = settings.get("agentAssignments").and_then(Value::as_object) else {
            return Ok(());
        };
        let previous = self.app_settings_value();
        let previous_assignments = previous.get("agentAssignments").and_then(Value::as_object);
        let repository =
            crate::backend::agent_market::AgentInstallationRepository::new(self.db.pool().clone());
        for (action_id, value) in assignments {
            let Some(agent_id) = value
                .get("agentId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                return Err(AppError::Validation(format!(
                    "agent_not_installed: invalid assignment for {action_id}"
                )));
            };
            if previous_assignments
                .and_then(|values| values.get(action_id))
                .and_then(|assignment| assignment.get("agentId"))
                .and_then(Value::as_str)
                == Some(agent_id)
            {
                continue;
            }
            let installation = repository
                .get(agent_id)
                .await
                .map_err(AppError::external)?
                .ok_or_else(|| AppError::NotFound(format!("agent_not_installed: {agent_id}")))?;
            if !installation.enabled || !installation.execution_ready() {
                return Err(AppError::Conflict(format!("agent_not_ready: {agent_id}")));
            }
            let catalog = crate::backend::agent_market::CatalogCache::best_available()
                .map_err(AppError::external)?;
            let item = catalog.item(agent_id).ok_or_else(|| {
                AppError::Validation(format!("agent_capability_unsupported: {agent_id}"))
            })?;
            let purpose = match action_id.as_str() {
                "translation.card" => "card_translation",
                "memory.extraction" | "memory.generation" | "memory.project" | "memory.global"
                | "memory.recall" => "memory",
                "prompt.optimization" => "prompt_optimization",
                other => {
                    return Err(AppError::Validation(format!(
                        "agent_capability_unsupported: unknown action {other}"
                    )));
                }
            };
            if !item
                .capabilities
                .purposes
                .iter()
                .any(|candidate| candidate == purpose)
            {
                return Err(AppError::Validation(format!(
                    "agent_capability_unsupported: {agent_id}/{purpose}"
                )));
            }
        }
        Ok(())
    }

    pub(crate) async fn run_doctor(&self) -> AppResult<Value> {
        let backup_root =
            capabilities::skill_backup_root_sqlx(self.db.pool(), self.tenant_id()).await?;
        let runtime_statuses = self.list_conversation_adapter_runtime_statuses().await?;
        let (runtime_status, runtime_message) =
            conversation_runtime_doctor_summary(&runtime_statuses);
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let source_count = crate::backend::store::count_rows_sqlx(pool, tenant_id, "sources")
            .await
            .map_err(AppError::external)?;
        Ok(json!({
            "checks": [
                { "name": "database", "status": "pass", "message": self.db_path.to_string_lossy() },
                {
                    "name": "skill_backup_root",
                    "status": if backup_root.exists() { "pass" } else { "fail" },
                    "message": backup_root.to_string_lossy()
                },
                {
                    "name": "sources",
                    "status": "pass",
                    "message": format!("{source_count} sources")
                },
                {
                    "name": "tenant",
                    "status": "pass",
                    "message": self.tenant_id()
                },
                {
                    "name": "conversation_adapter_runtimes",
                    "status": runtime_status,
                    "message": runtime_message,
                    "details": runtime_statuses
                }
            ]
        }))
    }
}

fn conversation_runtime_doctor_summary(
    runtime_statuses: &[crate::backend::conversations::ConversationAdapterRuntimeStatus],
) -> (&'static str, String) {
    let available_runtime_count = runtime_statuses
        .iter()
        .filter(|status| status.available)
        .count();
    let unavailable_required = runtime_statuses
        .iter()
        .filter(|status| status.required_version.is_some() && !status.available)
        .map(|status| {
            let requirement = status.required_version.as_deref().unwrap_or_default();
            format!("{:?} {requirement}", status.kind).to_ascii_lowercase()
        })
        .collect::<Vec<_>>();
    if unavailable_required.is_empty() {
        (
            "pass",
            format!(
                "{available_runtime_count}/{} runtimes available; all required conversation plugin runtimes available",
                runtime_statuses.len()
            ),
        )
    } else {
        (
            "warn",
            format!(
                "missing required conversation plugin runtimes: {}; {available_runtime_count}/{} runtimes available",
                unavailable_required.join(", "),
                runtime_statuses.len()
            ),
        )
    }
}

#[cfg(test)]
#[path = "system_tests.rs"]
mod tests;
