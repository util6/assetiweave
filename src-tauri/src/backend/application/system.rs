use super::prelude::*;
use crate::backend::runtime::{AppError, AppResult};

impl AppService {
    pub(crate) fn open_for_engine() -> AppResult<Self> {
        if let Some(runtime) = crate::backend::runtime::current_process_runtime() {
            return Ok(Self::from_runtime(&runtime));
        }

        // Engine unit tests use the same mandatory AppRuntime shape with a
        // temporary database. Production has no database fallback here.
        #[cfg(test)]
        {
            return Self::open_with_db_path(engine_db_path()?);
        }

        #[cfg(not(test))]
        {
            Err(AppError::Validation(
                "Engine AppRuntime has not been bootstrapped".to_string(),
            ))
        }
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
    pub(crate) fn open_with_db_path(db_path: PathBuf) -> AppResult<Self> {
        let manager =
            crate::backend::ai_execution::agent_runtime_manager(&db_path).map_err(|error| {
                let view = error.to_view();
                AppError::Domain {
                    code: view.code,
                    message: view.message,
                    retryable: view.retryable,
                    details: None,
                }
            })?;
        Self::open_with_db_path_and_manager(db_path, manager)
    }

    #[cfg(test)]
    pub(crate) fn open_with_db_path_and_manager(
        db_path: PathBuf,
        runtime_manager: std::sync::Arc<crate::backend::agent_market::AgentRuntimeManager>,
    ) -> AppResult<Self> {
        let db = crate::backend::store::Database::open_initialized(&db_path)
            .map_err(AppError::external)?;
        let pool = db.pool().clone();
        let context = db
            .block_on(
                async move { crate::backend::store::load_local_request_context_sqlx(&pool).await },
            )
            .map_err(AppError::external)?;
        let pool = db.pool().clone();
        let tenant_id = context.tenant.id.clone();
        let seed_tenant_id = tenant_id.clone();
        db.block_on(async move {
            crate::backend::store::seed_tenant_defaults_sqlx(&pool, &seed_tenant_id).await
        })
        .map_err(AppError::external)?;
        let pool = db.pool().clone();
        let prepared_builtin_adapters = db
            .block_on(crate::backend::store::list_conversation_adapters_sqlx(
                &pool, &tenant_id,
            ))
            .map_err(AppError::external)?
            .into_iter()
            .filter(|adapter| {
                adapter.trust_state
                    == crate::backend::models::ConversationAdapterTrustState::BuiltIn
            })
            .collect::<Vec<_>>();
        db.block_on(
            crate::backend::application::bootstrap::seed_prepared_builtin_adapters(
                &pool,
                &tenant_id,
                &prepared_builtin_adapters,
            ),
        )?;
        let runtime_root =
            crate::backend::agent_market::default_runtime_root().map_err(AppError::from)?;
        db.block_on(runtime_manager.recover_startup(&runtime_root))
            .map_err(AppError::external)?;
        let migration_scope = db_path.to_string_lossy().to_string();
        if let Err(error) = db.block_on(crate::backend::agent_market::migrate_legacy_assignments(
            db.pool().clone(),
            runtime_manager.clone(),
            &migration_scope,
        )) {
            eprintln!("agent market legacy migration deferred: {error}");
        }
        db.block_on(runtime_manager.reload())
            .map_err(AppError::external)?;
        let agent_runtime = runtime_manager.runtime();
        let runtime = crate::backend::runtime::AppRuntime::for_test(
            db_path.clone(),
            db.clone(),
            context.clone(),
            runtime_manager.clone(),
            agent_runtime.clone(),
        );
        Ok(Self {
            runtime: runtime.clone(),
            db,
            db_path,
            context,
            agent_runtime_manager: runtime_manager,
            agent_runtime,
            conversation_adapter_catalog: runtime.conversation_adapter_catalog(),
        })
    }

    #[cfg(test)]
    pub(crate) fn open_with_db_path_and_runtime(
        db_path: PathBuf,
        agent_runtime: std::sync::Arc<dyn crate::backend::ai_execution::AgentExecutionRuntime>,
    ) -> AppResult<Self> {
        let mut service = Self::open_with_db_path(db_path)?;
        let runtime = crate::backend::runtime::AppRuntime::for_test(
            service.db_path.clone(),
            service.db.clone(),
            service.context.clone(),
            service.agent_runtime_manager.clone(),
            agent_runtime.clone(),
        );
        service.runtime = runtime.clone();
        service.agent_runtime = agent_runtime;
        service.conversation_adapter_catalog = runtime.conversation_adapter_catalog();
        Ok(service)
    }

    pub(crate) fn request_context(&self) -> &RequestContext {
        &self.context
    }

    pub(crate) fn tenant_id(&self) -> &str {
        &self.context.tenant.id
    }

    pub(crate) fn overview(&self) -> AppResult<AppOverview> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            Ok(AppOverview {
                source_count: crate::backend::store::count_rows_sqlx(&pool, &tenant_id, "sources")
                    .await
                    .map_err(AppError::external)?,
                asset_count: crate::backend::store::count_rows_sqlx(&pool, &tenant_id, "assets")
                    .await
                    .map_err(AppError::external)?,
                profile_count: crate::backend::store::count_rows_sqlx(
                    &pool, &tenant_id, "profiles",
                )
                .await
                .map_err(AppError::external)?,
                last_scan_status: crate::backend::store::latest_scan_status_sqlx(&pool, &tenant_id)
                    .await
                    .map_err(AppError::external)?,
            })
        })
    }

    pub(crate) fn list_target_profile_descriptors(
        &self,
    ) -> AppResult<Vec<crate::backend::models::TargetProfileDescriptor>> {
        Ok(self.runtime.target_catalog().descriptors().to_vec())
    }

    pub(crate) fn refresh_target_profile_descriptors(
        &self,
    ) -> AppResult<Vec<crate::backend::models::TargetProfileDescriptor>> {
        Ok(self
            .runtime
            .refresh_target_catalog_from_disk()?
            .descriptors()
            .to_vec())
    }

    pub(crate) fn logs_get_snapshot(
        &self,
        file_name: Option<String>,
        line_limit: Option<usize>,
    ) -> AppResult<crate::backend::logs::LogSnapshot> {
        Ok(
            crate::backend::logs::logs_get_snapshot(file_name, line_limit)
                .map_err(AppError::external)?,
        )
    }

    pub(crate) fn logs_open_log_directory(&self) -> AppResult<()> {
        Ok(crate::backend::logs::logs_open_log_directory().map_err(AppError::external)?)
    }

    pub(crate) fn logs_write_operation(
        &self,
        level: String,
        operation: String,
        message: String,
        fields: Option<BTreeMap<String, String>>,
    ) -> AppResult<()> {
        Ok(
            crate::backend::logs::logs_write_operation(level, operation, message, fields)
                .map_err(AppError::external)?,
        )
    }

    pub(crate) fn get_app_settings(
        &self,
    ) -> AppResult<crate::backend::app_settings::AppSettingsFile> {
        Ok(crate::backend::app_settings::get_app_settings_for_database(
            &self.db,
        )?)
    }

    pub(crate) fn save_app_settings(
        &self,
        settings: Value,
    ) -> AppResult<crate::backend::app_settings::AppSettingsFile> {
        self.validate_agent_capability_assignments(&settings)?;
        Ok(crate::backend::app_settings::save_app_settings_for_database(&self.db, settings)?)
    }

    fn validate_agent_capability_assignments(&self, settings: &Value) -> AppResult<()> {
        let Some(assignments) = settings.get("agentAssignments").and_then(Value::as_object) else {
            return Ok(());
        };
        let previous =
            crate::backend::app_settings::read_app_settings_value_for_database(&self.db)?;
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
            let installation = self
                .db
                .block_on(repository.get(agent_id))
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
                "memory.extraction" | "memory.project" | "memory.global" | "memory.recall" => {
                    "memory"
                }
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

    pub(crate) fn run_doctor(&self) -> AppResult<Value> {
        let backup_root = capabilities::skill_backup_root_sqlx(&self.db, self.tenant_id())?;
        let runtime_statuses = self.list_conversation_adapter_runtime_statuses()?;
        let (runtime_status, runtime_message) =
            conversation_runtime_doctor_summary(&runtime_statuses);
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let source_count = self
            .db
            .block_on(async move {
                crate::backend::store::count_rows_sqlx(&pool, &tenant_id, "sources").await
            })
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

#[cfg(test)]
fn engine_db_path() -> AppResult<PathBuf> {
    if let Ok(path) = env::var("ASSETIWEAVE_DB_PATH") {
        if !path.trim().is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    Ok(crate::backend::path_utils::app_db_path()?)
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
mod tests {
    use super::*;
    use crate::backend::ai_execution::{
        executor::BackendFuture, AgentExecutionRuntime, AiExecutionRequest,
    };
    use crate::backend::conversations::{
        ConversationAdapterRuntimeKind, ConversationAdapterRuntimeStatus,
    };
    use std::sync::Arc;

    struct FakeAgentRuntime;

    impl AgentExecutionRuntime for FakeAgentRuntime {
        fn execute<'a>(&'a self, _request: AiExecutionRequest) -> BackendFuture<'a> {
            Box::pin(async { panic!("runtime execution is outside this constructor test") })
        }
    }

    #[test]
    fn app_service_accepts_an_injected_agent_runtime() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-runtime-injection-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let runtime: Arc<dyn AgentExecutionRuntime> = Arc::new(FakeAgentRuntime);

        let service = AppService::open_with_db_path_and_runtime(db_path.clone(), runtime.clone())
            .expect("open service with fake runtime");

        assert!(Arc::ptr_eq(&service.agent_runtime, &runtime));
        drop(service);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn default_app_services_use_independent_runtime_snapshots() {
        let first_path = std::env::temp_dir().join(format!(
            "assetiweave-runtime-shared-first-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let second_path = std::env::temp_dir().join(format!(
            "assetiweave-runtime-shared-second-{}.sqlite",
            uuid::Uuid::new_v4()
        ));

        let first = AppService::open_with_db_path(first_path.clone()).expect("first service");
        let second = AppService::open_with_db_path(second_path.clone()).expect("second service");

        let first_runtime = first.agent_runtime.clone();
        let second_runtime = second.agent_runtime.clone();
        assert!(!Arc::ptr_eq(&first_runtime, &second_runtime));
        drop(first);
        drop(second);
        let _ = std::fs::remove_file(first_path);
        let _ = std::fs::remove_file(second_path);
    }

    #[test]
    fn runtime_doctor_ignores_unavailable_unrequired_runtimes() {
        let statuses = vec![
            runtime_status(ConversationAdapterRuntimeKind::Node, false, None),
            runtime_status(ConversationAdapterRuntimeKind::Python, true, Some(">=3.10")),
            runtime_status(ConversationAdapterRuntimeKind::Bash, true, None),
        ];

        let (status, message) = conversation_runtime_doctor_summary(&statuses);

        assert_eq!(status, "pass");
        assert!(message.contains("all required conversation plugin runtimes available"));
        assert!(!message.contains("node runtime missing"));
    }

    #[test]
    fn runtime_doctor_warns_for_unavailable_required_runtimes() {
        let statuses = vec![
            runtime_status(ConversationAdapterRuntimeKind::Node, false, Some(">=20")),
            runtime_status(ConversationAdapterRuntimeKind::Python, true, None),
            runtime_status(ConversationAdapterRuntimeKind::Bash, true, None),
        ];

        let (status, message) = conversation_runtime_doctor_summary(&statuses);

        assert_eq!(status, "warn");
        assert!(message.contains("missing required conversation plugin runtimes"));
        assert!(message.contains("node >=20"));
    }

    fn runtime_status(
        kind: ConversationAdapterRuntimeKind,
        available: bool,
        required_version: Option<&str>,
    ) -> ConversationAdapterRuntimeStatus {
        ConversationAdapterRuntimeStatus {
            kind,
            program: "runtime".to_string(),
            available,
            version: None,
            required_version: required_version.map(str::to_string),
            error: None,
            hint: None,
        }
    }
}
