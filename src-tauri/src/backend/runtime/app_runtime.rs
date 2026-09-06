use super::{session_streams, tasks::TaskRuntime, AppError, AppResult};
use arc_swap::ArcSwap;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::backend::{
    agent_market::AgentRuntimeManager,
    ai_execution::AgentExecutionRuntime,
    application::AppService,
    conversations::ConversationAdapterCatalog,
    events::{EventDispatcher, EventDispatcherHandle, EventDispatcherShutdownReport},
    extension_kernel::RegistrySnapshot,
    models::{ConversationAdapter, RequestContext, Tenant},
    path_utils::ensure_app_library_dirs,
    store::{self, Database},
    target_catalog::TargetCatalog,
};

#[cfg(test)]
use crate::backend::models::{ConversationAdapterTrustState, TargetProfileDescriptor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeRole {
    ResidentHost,
    OneShot,
}

/// 绑定到一次请求的不可变上下文快照。
#[derive(Clone)]
pub(crate) struct RequestContextSnapshot {
    pub(crate) tenant: Tenant,
    pub(crate) request_context: RequestContext,
    pub(crate) agent_runtime_manager: Arc<AgentRuntimeManager>,
    pub(crate) agent_runtime: Arc<dyn AgentExecutionRuntime>,
    pub(crate) conversation_adapter_catalog: Arc<ConversationAdapterCatalog>,
}

#[derive(Debug, Default)]
pub(crate) struct ShutdownState {
    accepting: AtomicBool,
    shutdown_started: AtomicBool,
    finished_report: std::sync::Mutex<Option<ShutdownReport>>,
}

impl ShutdownState {
    pub(crate) fn new() -> Self {
        Self {
            accepting: AtomicBool::new(true),
            shutdown_started: AtomicBool::new(false),
            finished_report: std::sync::Mutex::new(None),
        }
    }

    pub(crate) fn begin(&self) -> bool {
        self.accepting.store(false, Ordering::Release);
        !self.shutdown_started.swap(true, Ordering::AcqRel)
    }

    pub(crate) fn get_finished_report(&self) -> Option<ShutdownReport> {
        self.finished_report
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    pub(crate) fn set_finished_report(&self, report: ShutdownReport) {
        if let Ok(mut guard) = self.finished_report.lock() {
            *guard = Some(report);
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ShutdownReport {
    pub(crate) unfinished_task_ids: Vec<String>,
    pub(crate) dispatcher_drained: bool,
    pub(crate) dispatcher_remaining_events: usize,
    pub(crate) dispatcher_timed_out: bool,
    pub(crate) unfinished_stages: Vec<String>,
}

impl Default for ShutdownReport {
    fn default() -> Self {
        Self {
            unfinished_task_ids: Vec::new(),
            dispatcher_drained: true,
            dispatcher_remaining_events: 0,
            dispatcher_timed_out: false,
            unfinished_stages: Vec::new(),
        }
    }
}

impl ShutdownReport {
    pub(crate) fn is_clean(&self) -> bool {
        self.unfinished_task_ids.is_empty()
            && self.dispatcher_drained
            && self.dispatcher_remaining_events == 0
            && !self.dispatcher_timed_out
            && self.unfinished_stages.is_empty()
    }
}

/// 进程级共享资源宿主。所有请求复用其中的数据库池与 tokio Runtime。
pub(crate) struct AppRuntime {
    db_path: PathBuf,
    db: Database,
    context: ArcSwap<RequestContextSnapshot>,
    task_runtime: TaskRuntime,
    context_update_gate: tokio::sync::Mutex<()>,
    shutdown: ShutdownState,
    shutdown_gate: tokio::sync::Mutex<()>,
    coordinator_timed_out: AtomicBool,
    dispatcher: Mutex<Option<EventDispatcherHandle>>,
    session_memory_coordinator: Mutex<Option<SessionMemoryCoordinatorHandle>>,
    session_streams: session_streams::SessionStreamRegistry,
    target_catalog_dir: PathBuf,
    target_catalog: RegistrySnapshot<TargetCatalog>,
    builtin_conversation_adapters: Arc<Vec<ConversationAdapter>>,
    config: Arc<super::config::RuntimeConfig>,
    settings: ArcSwap<serde_json::Value>,
}

struct SessionMemoryCoordinatorHandle {
    cancellation: CancellationToken,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl SessionMemoryCoordinatorHandle {
    async fn stop_until(&mut self, deadline: Instant) -> bool {
        self.cancellation.cancel();
        if let Some(join) = self.join.as_mut() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, &mut *join).await {
                Ok(res) => {
                    let _ = res;
                    true
                }
                Err(_) => {
                    join.abort();
                    false
                }
            }
        } else {
            true
        }
    }
}

static PROCESS_RUNTIME: OnceLock<Arc<AppRuntime>> = OnceLock::new();

impl AppRuntime {
    pub(crate) async fn bootstrap(db_path: PathBuf, role: RuntimeRole) -> AppResult<Arc<Self>> {
        let pool = store::open_migrated_pool(&db_path).await?;
        ensure_app_library_dirs()?;
        if let Err(error) = super::archive_legacy_memory_once(&db_path) {
            tracing::warn!(
                action = "app.startup.memory_legacy_archive",
                error = %error,
                "legacy Memory archive was not created"
            );
        }
        let target_catalog_dir = db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("target-providers");
        let target_catalog = TargetCatalog::load_with_overrides(&target_catalog_dir)?;

        // Bootstrap is the only production path that opens the migrated pool. The
        // old Database::open_initialized remains a test/migration compatibility API.
        store::seed_defaults_sqlx_with_catalog(&pool, &target_catalog).await?;
        let context = store::load_local_request_context_sqlx(&pool).await?;
        let tenant_id = context.tenant.id.clone();
        let builtin_conversation_adapters =
            crate::backend::bootstrap::materialize_and_seed_builtin_adapters(&pool, &tenant_id)
                .await?;
        let conversation_adapters =
            crate::backend::store::list_conversation_adapters_sqlx(&pool, &tenant_id).await?;
        let workspace_root = db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("agent-executions");
        let agent_runtime_manager =
            Arc::new(AgentRuntimeManager::new(pool.clone(), workspace_root));
        let runtime_root = crate::backend::agent_market::default_runtime_root()
            .map_err(|error| AppError::External(error.to_string()))?;
        agent_runtime_manager
            .recover_startup(&runtime_root)
            .await
            .map_err(AppError::External)?;
        let migration_scope = db_path.to_string_lossy().to_string();
        if let Err(error) = crate::backend::agent_market::migrate_legacy_assignments(
            pool.clone(),
            agent_runtime_manager.clone(),
            &migration_scope,
        )
        .await
        {
            tracing::warn!(
                action = "app.startup.agent_market_migration",
                error = %error,
                "agent market legacy migration deferred"
            );
        }
        agent_runtime_manager
            .reload()
            .await
            .map_err(AppError::External)?;
        if role == RuntimeRole::ResidentHost {
            if let Err(error) = agent_runtime_manager.prepare_startup_health_refresh().await {
                tracing::warn!(
                    action = "app.startup.agent_health_prepare",
                    error = %error,
                    "Agent startup health refresh could not be prepared"
                );
            }
        }

        let initial_settings =
            crate::backend::app_settings::load_or_import_app_settings_sqlx(&pool)
                .await
                .unwrap_or_else(|_| serde_json::json!({}));
        let task_runtime = TaskRuntime::with_runtime_handle(tokio::runtime::Handle::current());
        let db = Database::from_pool(pool);
        let snapshot = RequestContextSnapshot {
            tenant: context.tenant.clone(),
            request_context: context,
            agent_runtime: agent_runtime_manager.runtime(),
            agent_runtime_manager,
            conversation_adapter_catalog: Arc::new(ConversationAdapterCatalog::new(
                conversation_adapters,
            )),
        };
        let mut config = super::config::RuntimeConfig::from_environment()?;
        config.db_path = db_path.clone();
        let config = Arc::new(config);

        let app_runtime = Arc::new(Self {
            db_path,
            db,
            context: ArcSwap::from_pointee(snapshot),
            task_runtime,
            context_update_gate: tokio::sync::Mutex::new(()),
            shutdown: ShutdownState::new(),
            shutdown_gate: tokio::sync::Mutex::new(()),
            coordinator_timed_out: AtomicBool::new(false),
            dispatcher: Mutex::new(None),
            session_memory_coordinator: Mutex::new(None),
            session_streams: session_streams::SessionStreamRegistry::default(),
            target_catalog_dir,
            target_catalog: RegistrySnapshot::new(target_catalog),
            builtin_conversation_adapters: Arc::new(builtin_conversation_adapters),
            config,
            settings: ArcSwap::from_pointee(initial_settings),
        });

        // The ResidentHost owns long-lived dispatchers. OneShot deliberately only
        // gets the in-process task runtime and never starts a dispatcher.
        if role == RuntimeRole::ResidentHost {
            app_runtime.start_resident_services().await;
        }
        Ok(app_runtime)
    }

    /// Test-only runtime builder. Tests still construct the same resident
    /// runtime boundary as production, but inject their temporary database and
    /// agent backend instead of reopening a second application service path.

    #[cfg(test)]
    pub(crate) async fn for_test(
        db_path: PathBuf,
        db: Database,
        context: RequestContext,
        agent_runtime_manager: Arc<AgentRuntimeManager>,
        agent_runtime: Arc<dyn AgentExecutionRuntime>,
    ) -> Arc<Self> {
        Self::for_test_with_target_catalog(
            db_path,
            db,
            context,
            agent_runtime_manager,
            agent_runtime,
            TargetCatalog::builtin().expect("test target catalog must be valid"),
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn for_test_with_target_catalog(
        db_path: PathBuf,
        db: Database,
        context: RequestContext,
        agent_runtime_manager: Arc<AgentRuntimeManager>,
        agent_runtime: Arc<dyn AgentExecutionRuntime>,
        target_catalog: TargetCatalog,
    ) -> Arc<Self> {
        let target_catalog_dir = db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("target-providers");
        let adapters =
            crate::backend::store::list_conversation_adapters_sqlx(db.pool(), &context.tenant.id)
                .await
                .unwrap_or_default();
        let builtin_conversation_adapters = adapters
            .iter()
            .filter(|adapter| adapter.trust_state == ConversationAdapterTrustState::BuiltIn)
            .cloned()
            .collect();
        let initial_settings =
            crate::backend::app_settings::load_or_import_app_settings_sqlx(db.pool())
                .await
                .unwrap_or_else(|_| {
                    crate::backend::app_settings::canonicalize_settings(serde_json::json!({}))
                        .unwrap_or_else(|_| serde_json::json!({}))
                });
        Arc::new(Self {
            db_path: db_path.clone(),
            db,
            context: ArcSwap::from_pointee(RequestContextSnapshot {
                tenant: context.tenant.clone(),
                request_context: context,
                agent_runtime_manager,
                agent_runtime,
                conversation_adapter_catalog: Arc::new(ConversationAdapterCatalog::new(adapters)),
            }),
            task_runtime: TaskRuntime::new(),
            context_update_gate: tokio::sync::Mutex::new(()),
            shutdown: ShutdownState::new(),
            shutdown_gate: tokio::sync::Mutex::new(()),
            coordinator_timed_out: AtomicBool::new(false),
            dispatcher: Mutex::new(None),
            session_memory_coordinator: Mutex::new(None),
            session_streams: session_streams::SessionStreamRegistry::default(),
            target_catalog_dir,
            target_catalog: RegistrySnapshot::new(target_catalog),
            builtin_conversation_adapters: Arc::new(builtin_conversation_adapters),
            config: {
                let mut config =
                    super::config::RuntimeConfig::from_environment().unwrap_or_else(|_| {
                        let defaults = super::config::RuntimeConfigDefaults {
                            home_dir: PathBuf::from("/fixture/home"),
                            data_dir: PathBuf::from("/fixture/data"),
                        };
                        super::config::RuntimeConfig::from_env_map(&Default::default(), &defaults)
                            .unwrap()
                    });
                config.db_path = db_path;
                Arc::new(config)
            },
            settings: ArcSwap::from_pointee(initial_settings),
        })
    }

    pub(crate) fn config(&self) -> Arc<super::config::RuntimeConfig> {
        Arc::clone(&self.config)
    }

    async fn start_resident_services(self: &Arc<Self>) {
        self.start_agent_health_refresh();
        self.start_team_coordinator();
        self.start_session_memory_coordinator();
        let dispatcher = Arc::new(EventDispatcher::new(self.db.clone(), self.db_path.clone()));
        if let Err(error) = dispatcher.initialize_all_tenants().await {
            tracing::warn!(
                action = "app.startup.event_dispatcher",
                error = %error,
                "domain event dispatcher initialization deferred"
            );
            return;
        }
        if let Some(runtime_handle) = self.task_runtime.runtime_handle() {
            let handle = dispatcher.start(&runtime_handle);
            if let Ok(mut slot) = self.dispatcher.lock() {
                *slot = Some(handle);
            }
        }
    }

    /// Rehydrate durable Session Memory work independently of the Memory page.
    /// The loop only owns scheduling; SQLite owns leases, retries, watermarks,
    /// and terminal state, so an interrupted process can be rebuilt safely.
    fn start_session_memory_coordinator(self: &Arc<Self>) {
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let runtime = self.clone();
        let join = tokio::spawn(async move {
            while !task_cancellation.is_cancelled() {
                let service = AppService::from_runtime(&runtime);
                let principal_id = runtime.context().request_context.principal.id.clone();
                let result: AppResult<()> = async {
                    let tenants =
                        store::list_tenants_for_principal_sqlx(runtime.pool(), &principal_id)
                            .await?;
                    for tenant in tenants {
                        if let Err(error) = service
                            .reconcile_session_memory_jobs_for_tenant_at(
                                &tenant.id,
                                chrono::Utc::now(),
                            )
                            .await
                        {
                            tracing::warn!(
                                action = "session_memory.coordinator.recovery",
                                tenant_id = %tenant.id,
                                error = %error,
                                "Session Memory durable coordinator reconciliation failed"
                            );
                        }
                        if let Err(error) = service
                            .reconcile_project_memory_jobs_for_tenant_at(
                                &tenant.id,
                                chrono::Utc::now(),
                            )
                            .await
                        {
                            tracing::warn!(
                                action = "project_memory.coordinator.recovery",
                                tenant_id = %tenant.id,
                                error = %error,
                                "Project Memory durable coordinator reconciliation failed"
                            );
                        }
                        if let Err(error) = service
                            .reconcile_global_memory_jobs_for_tenant_at(
                                &tenant.id,
                                chrono::Utc::now(),
                            )
                            .await
                        {
                            tracing::warn!(
                                action = "global_memory.coordinator.recovery",
                                tenant_id = %tenant.id,
                                error = %error,
                                "Global Memory durable coordinator reconciliation failed"
                            );
                        }
                        if let Err(error) = service
                            .recover_memory_recall_turns_for_tenant(&tenant.id)
                            .await
                        {
                            tracing::warn!(
                                action = "memory_recall.coordinator.recovery",
                                tenant_id = %tenant.id,
                                error = %error,
                                "Recall durable workflow reconciliation failed"
                            );
                        }
                    }
                    Ok(())
                }
                .await;
                if let Err(error) = result {
                    tracing::warn!(
                        action = "session_memory.coordinator.tenants",
                        error = %error,
                        "Session Memory tenant enumeration failed"
                    );
                }
                for _ in 0..10 {
                    if task_cancellation.is_cancelled() {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        });
        if let Ok(mut slot) = self.session_memory_coordinator.lock() {
            *slot = Some(SessionMemoryCoordinatorHandle {
                cancellation,
                join: Some(join),
            });
        }
    }

    /// Reconcile durable Team facts independently of the UI and provider
    /// process. Confirm writes the run and wake-up event first; this resident
    /// loop then makes startup, duplicate delivery, and mid-run interruption
    /// converge through the same AppService scheduling path.
    fn start_team_coordinator(self: &Arc<Self>) {
        let runtime = self.clone();
        let mut spec = super::tasks::TaskSpec::global(
            super::tasks::TaskKind::Other,
            Some("team-coordinator".to_string()),
        );
        spec.detail = serde_json::json!({
            "domain": "team",
            "operation": "coordinator_reconciliation",
        });
        let _ = self
            .task_runtime
            .spawn_async(spec, move |context| async move {
                while !context.is_cancelled() {
                    if let Err(error) = AppService::from_runtime(&runtime).recover_team_runs().await
                    {
                        tracing::warn!(
                            action = "team.coordinator.recovery",
                            error = %error,
                            "Team durable coordinator reconciliation failed"
                        );
                    }
                    for _ in 0..10 {
                        if context.is_cancelled() {
                            return Ok(serde_json::json!({ "status": "stopped" }));
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
                Ok(serde_json::json!({ "status": "stopped" }))
            });
    }

    fn start_agent_health_refresh(&self) {
        let snapshot = self.context();
        let runtime_manager = snapshot.agent_runtime_manager.clone();
        let mut spec = super::tasks::TaskSpec::global(
            super::tasks::TaskKind::Other,
            Some("agent-health-startup".to_string()),
        );
        spec.detail = serde_json::json!({
            "domain": "agent_market",
            "operation": "startup_health_refresh",
        });
        let spawn = self
            .task_runtime
            .spawn_async(spec, move |context| async move {
                if context.is_cancelled() {
                    return Err(AppError::Cancelled(
                        "Agent startup health refresh was cancelled".to_string(),
                    ));
                }
                let summary = runtime_manager
                    .refresh_installed_agent_health()
                    .await
                    .map_err(AppError::External)?;
                Ok(serde_json::json!({
                    "checked": summary.checked,
                    "available": summary.available,
                    "unavailable": summary.unavailable,
                }))
            });
        if let Err(error) = spawn {
            tracing::warn!(
                action = "app.startup.agent_health_refresh",
                error = %error,
                "Agent startup health refresh could not be started"
            );
        }
    }

    pub(crate) fn db(&self) -> &Database {
        &self.db
    }
    pub(crate) fn pool(&self) -> &sqlx::SqlitePool {
        self.db.pool()
    }
    pub(crate) fn db_path(&self) -> &Path {
        &self.db_path
    }
    pub(crate) fn context(&self) -> Arc<RequestContextSnapshot> {
        self.context.load_full()
    }

    /// Persist and publish a tenant change as one runtime transition.
    ///
    /// The database update is validated first, then the new request context is
    /// loaded before the ArcSwap publication. If context construction fails,
    /// the active tenant is compensated back to the previously published
    /// snapshot so callers never keep a successful database change with a
    /// stale runtime context.
    pub(crate) async fn activate_tenant(&self, tenant_id: &str) -> AppResult<Tenant> {
        let _update_guard = self.context_update_gate.lock().await;
        let previous = self.context();
        let principal_id = previous.request_context.principal.id.clone();
        let previous_tenant_id = previous.tenant.id.clone();
        let tenant_id = tenant_id.to_string();
        let pool = self.pool().clone();

        let transition = async {
            let tenant =
                crate::backend::store::set_active_tenant_sqlx(&pool, &principal_id, &tenant_id)
                    .await?;
            let next_context =
                crate::backend::store::load_local_request_context_sqlx(&pool).await?;
            let next_snapshot = self.build_tenant_snapshot(next_context).await?;
            AppResult::Ok((tenant, next_snapshot))
        }
        .await;

        let (tenant, next_snapshot) = match transition {
            Ok(transition) => Ok(transition),
            Err(error) => {
                crate::backend::store::set_active_tenant_sqlx(
                    &pool,
                    &principal_id,
                    &previous_tenant_id,
                )
                .await
                .map_err(|rollback_error| {
                    AppError::Conflict(format!(
                        "租户上下文构造失败且回滚 active tenant 失败: {error}; {rollback_error}"
                    ))
                })?;
                Err(error)
            }
        }?;

        self.context.store(Arc::new(next_snapshot));
        Ok(tenant)
    }

    pub(crate) fn app_settings_value(&self) -> serde_json::Value {
        (**self.settings.load()).clone()
    }

    pub(crate) fn update_app_settings_value(&self, new_settings: serde_json::Value) {
        self.settings.store(Arc::new(new_settings));
    }

    pub(crate) fn backend_settings(
        &self,
    ) -> AppResult<crate::backend::app_settings::BackendSettings> {
        crate::backend::app_settings::BackendSettings::from_value(&self.app_settings_value())
    }

    async fn build_tenant_snapshot(
        &self,
        request_context: RequestContext,
    ) -> AppResult<RequestContextSnapshot> {
        let tenant_id = request_context.tenant.id.clone();
        let current = self.context();
        let manager = current.agent_runtime_manager.clone();
        let pool = self.pool().clone();
        crate::backend::bootstrap::reconcile_app_conversation_adapters(&pool, &tenant_id).await?;
        let adapters =
            crate::backend::store::list_conversation_adapters_sqlx(&pool, &tenant_id).await?;
        Ok(RequestContextSnapshot {
            tenant: request_context.tenant.clone(),
            request_context,
            agent_runtime: current.agent_runtime.clone(),
            agent_runtime_manager: manager,
            conversation_adapter_catalog: Arc::new(ConversationAdapterCatalog::new(adapters)),
        })
    }

    pub(crate) fn agent_runtime(&self) -> Arc<dyn AgentExecutionRuntime> {
        self.context().agent_runtime.clone()
    }
    pub(crate) fn task_runtime(&self) -> &TaskRuntime {
        &self.task_runtime
    }

    pub(crate) fn session_streams(&self) -> &session_streams::SessionStreamRegistry {
        &self.session_streams
    }

    /// Stop accepting work and wait for resident tasks before close-time
    /// persistence runs. The final dispatcher/database shutdown remains in
    /// `shutdown_with_grace` so callers can persist through this same runtime.
    #[allow(dead_code)]
    pub(crate) async fn stop_tasks_with_grace(&self, grace: Duration) -> Vec<String> {
        self.stop_tasks_until(Instant::now() + grace).await
    }

    pub(crate) async fn stop_tasks_until(&self, deadline: Instant) -> Vec<String> {
        let clean = self.stop_session_memory_coordinator_until(deadline).await;
        if !clean {
            self.coordinator_timed_out.store(true, Ordering::Release);
        }
        self.task_runtime.stop_accepting();
        self.task_runtime
            .shutdown_until(deadline)
            .await
            .unfinished_task_ids
    }

    async fn stop_session_memory_coordinator_until(&self, deadline: Instant) -> bool {
        let handle = self
            .session_memory_coordinator
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());
        if let Some(mut handle) = handle {
            handle.stop_until(deadline).await
        } else {
            true
        }
    }

    #[cfg(test)]
    pub(crate) fn set_test_session_memory_coordinator(
        &self,
        cancellation: CancellationToken,
        join: tokio::task::JoinHandle<()>,
    ) {
        if let Ok(mut slot) = self.session_memory_coordinator.lock() {
            *slot = Some(SessionMemoryCoordinatorHandle {
                cancellation,
                join: Some(join),
            });
        }
    }

    pub(crate) fn target_catalog(&self) -> Arc<TargetCatalog> {
        self.target_catalog.load()
    }

    /// Validate a complete provider set outside the snapshot and publish it as
    /// one replacement. Readers keep using the previous immutable catalog when
    /// validation fails.
    #[cfg(test)]
    pub(crate) async fn refresh_target_catalog(
        &self,
        descriptors: Vec<TargetProfileDescriptor>,
    ) -> AppResult<Arc<TargetCatalog>> {
        let catalog = TargetCatalog::from_descriptors(descriptors)?;
        self.reconcile_tenants_with_target_catalog(&catalog).await?;
        self.target_catalog.replace(catalog);
        Ok(self.target_catalog.load())
    }

    pub(crate) async fn refresh_target_catalog_from_disk(&self) -> AppResult<Arc<TargetCatalog>> {
        let catalog = TargetCatalog::load_with_overrides(&self.target_catalog_dir)?;
        self.reconcile_tenants_with_target_catalog(&catalog).await?;
        self.target_catalog.replace(catalog);
        Ok(self.target_catalog.load())
    }

    async fn reconcile_tenants_with_target_catalog(
        &self,
        catalog: &TargetCatalog,
    ) -> AppResult<()> {
        let pool = self.db.pool();
        let principal_id = self.context().request_context.principal.id.clone();
        let tenants =
            crate::backend::store::list_tenants_for_principal_sqlx(pool, &principal_id).await?;
        for tenant in tenants {
            crate::backend::store::seed_tenant_defaults_sqlx_with_catalog(
                pool, &tenant.id, catalog,
            )
            .await?;
        }
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn conversation_adapter_catalog(&self) -> Arc<ConversationAdapterCatalog> {
        self.context().conversation_adapter_catalog.clone()
    }

    pub(crate) fn builtin_conversation_adapters(&self) -> Arc<Vec<ConversationAdapter>> {
        self.builtin_conversation_adapters.clone()
    }

    pub(crate) async fn refresh_conversation_adapter_catalog(&self) -> AppResult<()> {
        let _update_guard = self.context_update_gate.lock().await;
        let current = self.context();
        let tenant_id = current.tenant.id.clone();
        let pool = self.pool().clone();
        let adapters =
            crate::backend::store::list_conversation_adapters_sqlx(&pool, &tenant_id).await?;
        let mut next = (*current).clone();
        next.conversation_adapter_catalog = Arc::new(ConversationAdapterCatalog::new(adapters));
        self.context.store(Arc::new(next));
        Ok(())
    }

    pub(crate) fn notify_domain_events(&self) {
        if let Ok(slot) = self.dispatcher.lock() {
            if let Some(handle) = slot.as_ref() {
                handle.notify();
            }
        }
    }

    pub(crate) async fn shutdown_until(&self, deadline: Instant) -> ShutdownReport {
        let _gate = self.shutdown_gate.lock().await;
        if let Some(report) = self.shutdown.get_finished_report() {
            return report;
        }

        self.shutdown.begin();
        self.task_runtime.stop_accepting();

        let mut unfinished_stages = Vec::new();

        // 1. Session memory coordinator
        let coordinator_clean = self.stop_session_memory_coordinator_until(deadline).await;
        if !coordinator_clean || self.coordinator_timed_out.load(Ordering::Acquire) {
            unfinished_stages.push("session_memory_coordinator".to_string());
        }

        // 2. Task runtime: cancels active tasks and awaits tracked tasks until deadline
        let task_report = self.task_runtime.shutdown_until(deadline).await;
        if !task_report.unfinished_task_ids.is_empty() {
            unfinished_stages.push("tasks".to_string());
        }

        // 3. Dispatcher: drain domain events until deadline
        let mut dispatcher_handle = self.dispatcher.lock().ok().and_then(|mut slot| slot.take());
        let dispatcher_report = match dispatcher_handle.as_mut() {
            Some(handle) => handle.stop_until(deadline).await,
            None => EventDispatcherShutdownReport::default(),
        };
        if dispatcher_report.timed_out || !dispatcher_report.drained {
            unfinished_stages.push("dispatcher".to_string());
        }

        // 4. Session streams
        self.session_streams.clear();

        // 5. Database pool: close bounded by remaining deadline
        let pool = self.db.pool().clone();
        let remaining = deadline.saturating_duration_since(Instant::now());
        if tokio::time::timeout(remaining, pool.close()).await.is_err() {
            unfinished_stages.push("database_pool".to_string());
        }

        let report = ShutdownReport {
            unfinished_task_ids: task_report.unfinished_task_ids,
            dispatcher_drained: dispatcher_report.drained,
            dispatcher_remaining_events: dispatcher_report.remaining_events,
            dispatcher_timed_out: dispatcher_report.timed_out,
            unfinished_stages,
        };

        self.shutdown.set_finished_report(report.clone());
        report
    }

    pub(crate) async fn shutdown_with_grace(&self, grace: Duration) -> ShutdownReport {
        self.shutdown_until(Instant::now() + grace).await
    }
}

pub(crate) fn install_process_runtime(runtime: Arc<AppRuntime>) -> AppResult<()> {
    PROCESS_RUNTIME
        .set(runtime)
        .map_err(|_| AppError::Conflict("进程运行时已经初始化".to_string()))
}

pub(crate) fn current_process_runtime() -> Option<Arc<AppRuntime>> {
    PROCESS_RUNTIME.get().cloned()
}
