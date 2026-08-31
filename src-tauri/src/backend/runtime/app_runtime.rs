use super::{tasks::TaskRuntime, AppError, AppResult};
use arc_swap::ArcSwap;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

use crate::backend::{
    agent_market::AgentRuntimeManager,
    ai_execution::AgentExecutionRuntime,
    application::AppService,
    conversations::ConversationAdapterCatalog,
    events::{EventDispatcher, EventDispatcherHandle},
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
}

impl ShutdownState {
    pub(crate) fn new() -> Self {
        Self {
            accepting: AtomicBool::new(true),
            shutdown_started: AtomicBool::new(false),
        }
    }

    pub(crate) fn begin(&self) -> bool {
        self.accepting.store(false, Ordering::Release);
        !self.shutdown_started.swap(true, Ordering::AcqRel)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ShutdownReport {
    pub(crate) unfinished_task_ids: Vec<String>,
    pub(crate) dispatcher_drained: bool,
    pub(crate) dispatcher_remaining_events: usize,
    pub(crate) dispatcher_timed_out: bool,
}

impl Default for ShutdownReport {
    fn default() -> Self {
        Self {
            unfinished_task_ids: Vec::new(),
            dispatcher_drained: true,
            dispatcher_remaining_events: 0,
            dispatcher_timed_out: false,
        }
    }
}

/// 进程级共享资源宿主。所有请求复用其中的数据库池与 tokio Runtime。
pub(crate) struct AppRuntime {
    db_path: PathBuf,
    db: Database,
    context: ArcSwap<RequestContextSnapshot>,
    task_runtime: TaskRuntime,
    context_update_gate: Mutex<()>,
    shutdown: ShutdownState,
    dispatcher: Mutex<Option<EventDispatcherHandle>>,
    session_memory_coordinator: Mutex<Option<SessionMemoryCoordinatorHandle>>,
    team_coordinator: Mutex<Option<TeamCoordinatorHandle>>,
    target_catalog_dir: PathBuf,
    target_catalog: RegistrySnapshot<TargetCatalog>,
    builtin_conversation_adapters: Arc<Vec<ConversationAdapter>>,
}

struct TeamCoordinatorHandle {
    cancellation: CancellationToken,
    join: Option<thread::JoinHandle<()>>,
}

struct SessionMemoryCoordinatorHandle {
    cancellation: CancellationToken,
    join: Option<thread::JoinHandle<()>>,
}

impl SessionMemoryCoordinatorHandle {
    fn stop(mut self) {
        self.cancellation.cancel();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl TeamCoordinatorHandle {
    fn stop(mut self) {
        self.cancellation.cancel();
        // The coordinator performs only the bounded durable reconciliation
        // query and task registration; join it before closing the shared pool
        // so no recovery pass can race database shutdown.
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

static PROCESS_RUNTIME: OnceLock<Arc<AppRuntime>> = OnceLock::new();

impl AppRuntime {
    pub(crate) fn bootstrap(db_path: PathBuf, role: RuntimeRole) -> AppResult<Arc<Self>> {
        let runtime = crate::backend::store::build_runtime()?;
        let pool = runtime.block_on(store::open_migrated_pool(&db_path))?;
        ensure_app_library_dirs()?;
        let target_catalog_dir = db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("target-providers");
        let target_catalog = TargetCatalog::load_with_overrides(&target_catalog_dir)?;

        // Bootstrap is the only production path that opens the migrated pool. The
        // old Database::open_initialized remains a test/migration compatibility API.
        runtime.block_on(store::seed_defaults_sqlx_with_catalog(
            &pool,
            &target_catalog,
        ))?;
        let context = runtime.block_on(store::load_local_request_context_sqlx(&pool))?;
        let tenant_id = context.tenant.id.clone();
        let builtin_conversation_adapters = runtime.block_on(
            crate::backend::bootstrap::materialize_and_seed_builtin_adapters(&pool, &tenant_id),
        )?;
        let conversation_adapters = runtime.block_on(
            crate::backend::store::list_conversation_adapters_sqlx(&pool, &tenant_id),
        )?;
        let workspace_root = db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("agent-executions");
        let agent_runtime_manager =
            Arc::new(AgentRuntimeManager::new(pool.clone(), workspace_root));
        let runtime_root = crate::backend::agent_market::default_runtime_root()
            .map_err(|error| AppError::External(error.to_string()))?;
        runtime
            .block_on(agent_runtime_manager.recover_startup(&runtime_root))
            .map_err(AppError::External)?;
        let migration_scope = db_path.to_string_lossy().to_string();
        if let Err(error) =
            runtime.block_on(crate::backend::agent_market::migrate_legacy_assignments(
                pool.clone(),
                agent_runtime_manager.clone(),
                &migration_scope,
            ))
        {
            crate::backend::operation_log::log_warn(
                "app.startup.agent_market_migration",
                "agent market legacy migration deferred",
                &[("error", error.to_string())],
            );
        }
        runtime
            .block_on(agent_runtime_manager.reload())
            .map_err(AppError::External)?;
        if role == RuntimeRole::ResidentHost {
            if let Err(error) =
                runtime.block_on(agent_runtime_manager.prepare_startup_health_refresh())
            {
                crate::backend::operation_log::log_warn(
                    "app.startup.agent_health_prepare",
                    "ACP startup health refresh could not be prepared",
                    &[("error", error)],
                );
            }
        }

        let task_runtime = TaskRuntime::with_runtime_handle(runtime.handle().clone());
        let db = Database::from_parts(pool, runtime);
        let snapshot = RequestContextSnapshot {
            tenant: context.tenant.clone(),
            request_context: context,
            agent_runtime: agent_runtime_manager.runtime(),
            agent_runtime_manager,
            conversation_adapter_catalog: Arc::new(ConversationAdapterCatalog::new(
                conversation_adapters,
            )),
        };
        let app_runtime = Arc::new(Self {
            db_path,
            db,
            context: ArcSwap::from_pointee(snapshot),
            task_runtime,
            context_update_gate: Mutex::new(()),
            shutdown: ShutdownState::new(),
            dispatcher: Mutex::new(None),
            session_memory_coordinator: Mutex::new(None),
            team_coordinator: Mutex::new(None),
            target_catalog_dir,
            target_catalog: RegistrySnapshot::new(target_catalog),
            builtin_conversation_adapters: Arc::new(builtin_conversation_adapters),
        });
        // The ResidentHost owns long-lived dispatchers. OneShot deliberately only
        // gets the in-process task runtime and never starts a dispatcher.
        if role == RuntimeRole::ResidentHost {
            app_runtime.start_resident_services();
        }
        Ok(app_runtime)
    }

    /// Test-only runtime builder. Tests still construct the same resident
    /// runtime boundary as production, but inject their temporary database and
    /// agent backend instead of reopening a second application service path.
    #[cfg(test)]
    pub(crate) fn for_test(
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
    }

    #[cfg(test)]
    pub(crate) fn for_test_with_target_catalog(
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
        let adapters = db
            .block_on(crate::backend::store::list_conversation_adapters_sqlx(
                db.pool(),
                &context.tenant.id,
            ))
            .unwrap_or_default();
        let builtin_conversation_adapters = adapters
            .iter()
            .filter(|adapter| adapter.trust_state == ConversationAdapterTrustState::BuiltIn)
            .cloned()
            .collect();
        Arc::new(Self {
            db_path,
            db,
            context: ArcSwap::from_pointee(RequestContextSnapshot {
                tenant: context.tenant.clone(),
                request_context: context,
                agent_runtime_manager,
                agent_runtime,
                conversation_adapter_catalog: Arc::new(ConversationAdapterCatalog::new(adapters)),
            }),
            task_runtime: TaskRuntime::new(),
            context_update_gate: Mutex::new(()),
            shutdown: ShutdownState::new(),
            dispatcher: Mutex::new(None),
            session_memory_coordinator: Mutex::new(None),
            team_coordinator: Mutex::new(None),
            target_catalog_dir,
            target_catalog: RegistrySnapshot::new(target_catalog),
            builtin_conversation_adapters: Arc::new(builtin_conversation_adapters),
        })
    }

    fn start_resident_services(self: &Arc<Self>) {
        self.start_agent_health_refresh();
        self.start_team_coordinator();
        self.start_session_memory_coordinator();
        let dispatcher = Arc::new(EventDispatcher::new(self.db.clone(), self.db_path.clone()));
        if let Err(error) = dispatcher.initialize_all_tenants() {
            crate::backend::operation_log::log_warn(
                "app.startup.event_dispatcher",
                "domain event dispatcher initialization deferred",
                &[("error", error.to_string())],
            );
            return;
        }
        let handle = dispatcher.start();
        if let Ok(mut slot) = self.dispatcher.lock() {
            *slot = Some(handle);
        }
    }

    /// Rehydrate durable Session Memory work independently of the Memory page.
    /// The loop only owns scheduling; SQLite owns leases, retries, watermarks,
    /// and terminal state, so an interrupted process can be rebuilt safely.
    fn start_session_memory_coordinator(self: &Arc<Self>) {
        let cancellation = CancellationToken::new();
        let thread_cancellation = cancellation.clone();
        let runtime = self.clone();
        let join = thread::Builder::new()
            .name("aiw-session-memory-coordinator".to_string())
            .spawn(move || {
                while !thread_cancellation.is_cancelled() {
                    let service = AppService::from_runtime(&runtime);
                    let principal_id = runtime.context().request_context.principal.id.clone();
                    let result = runtime.run_sync(async {
                        store::list_tenants_for_principal_sqlx(runtime.pool(), &principal_id).await
                    });
                    match result {
                        Ok(tenants) => {
                            for tenant in tenants {
                                if let Err(error) = service
                                    .reconcile_session_memory_jobs_for_tenant_at(
                                        &tenant.id,
                                        chrono::Utc::now(),
                                    )
                                {
                                    crate::backend::operation_log::log_warn(
                                        "session_memory.coordinator.recovery",
                                        "Session Memory durable coordinator reconciliation failed",
                                        &[("error", error.to_string())],
                                    );
                                }
                                if let Err(error) = service
                                    .reconcile_project_memory_jobs_for_tenant_at(
                                        &tenant.id,
                                        chrono::Utc::now(),
                                    )
                                {
                                    crate::backend::operation_log::log_warn(
                                        "project_memory.coordinator.recovery",
                                        "Project Memory durable coordinator reconciliation failed",
                                        &[("error", error.to_string())],
                                    );
                                }
                            }
                        }
                        Err(error) => {
                            crate::backend::operation_log::log_warn(
                                "session_memory.coordinator.tenants",
                                "Session Memory tenant enumeration failed",
                                &[("error", error.to_string())],
                            );
                        }
                    }
                    for _ in 0..10 {
                        if thread_cancellation.is_cancelled() {
                            return;
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            })
            .expect("Session Memory coordinator thread must start");
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
        let cancellation = CancellationToken::new();
        let thread_cancellation = cancellation.clone();
        let runtime = self.clone();
        let join = thread::Builder::new()
            .name("aiw-team-coordinator".to_string())
            .spawn(move || {
                while !thread_cancellation.is_cancelled() {
                    if let Err(error) = AppService::from_runtime(&runtime).recover_team_runs() {
                        crate::backend::operation_log::log_warn(
                            "team.coordinator.recovery",
                            "Team durable coordinator reconciliation failed",
                            &[("error", error.to_string())],
                        );
                    }
                    for _ in 0..10 {
                        if thread_cancellation.is_cancelled() {
                            return;
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            })
            .expect("Team coordinator thread must start");
        if let Ok(mut slot) = self.team_coordinator.lock() {
            *slot = Some(TeamCoordinatorHandle {
                cancellation,
                join: Some(join),
            });
        }
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
        let spawn = self.task_runtime.spawn(
            spec,
            Box::new(move |context| {
                if context.is_cancelled() {
                    return Err(AppError::Canceled(
                        "ACP startup health refresh was cancelled".to_string(),
                    ));
                }
                let summary = runtime_manager
                    .refresh_installed_acp_health_blocking()
                    .map_err(AppError::External)?;
                Ok(serde_json::json!({
                    "checked": summary.checked,
                    "available": summary.available,
                    "unavailable": summary.unavailable,
                }))
            }),
        );
        if let Err(error) = spawn {
            crate::backend::operation_log::log_warn(
                "app.startup.agent_health_refresh",
                "ACP startup health refresh could not be started",
                &[("error", error.to_string())],
            );
        }
    }

    pub(crate) fn db(&self) -> &Database {
        &self.db
    }
    pub(crate) fn pool(&self) -> &sqlx::SqlitePool {
        self.db.pool()
    }
    pub(crate) fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        self.db.block_on(future)
    }
    /// Named synchronous boundary for short application projections.
    /// Application modules should use this seam instead of embedding their
    /// own database/runtime synchronization calls.
    pub(crate) fn run_sync<F: std::future::Future>(&self, future: F) -> F::Output {
        self.db.block_on(future)
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
    pub(crate) fn activate_tenant(&self, tenant_id: &str) -> AppResult<Tenant> {
        let _update_guard = self
            .context_update_gate
            .lock()
            .map_err(|_| AppError::Conflict("租户上下文更新锁不可用".to_string()))?;
        let previous = self.context();
        let principal_id = previous.request_context.principal.id.clone();
        let previous_tenant_id = previous.tenant.id.clone();
        let tenant_id = tenant_id.to_string();
        let pool = self.pool().clone();

        let (tenant, next_snapshot) = self.block_on(async {
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
            match transition {
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
            }
        })?;

        self.context.store(Arc::new(next_snapshot));
        Ok(tenant)
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

    /// Stop accepting work and wait for resident tasks before close-time
    /// persistence runs. The final dispatcher/database shutdown remains in
    /// `shutdown_with_grace` so callers can persist through this same runtime.
    pub(crate) fn stop_tasks_with_grace(&self, grace: Duration) -> Vec<String> {
        self.stop_session_memory_coordinator();
        self.task_runtime.stop_accepting();
        self.task_runtime
            .shutdown_with_grace(grace)
            .unfinished_task_ids
    }

    fn stop_session_memory_coordinator(&self) {
        if let Ok(mut slot) = self.session_memory_coordinator.lock() {
            if let Some(handle) = slot.take() {
                handle.stop();
            }
        }
    }
    pub(crate) fn target_catalog(&self) -> Arc<TargetCatalog> {
        self.target_catalog.load()
    }

    /// Validate a complete provider set outside the snapshot and publish it as
    /// one replacement. Readers keep using the previous immutable catalog when
    /// validation fails.
    #[cfg(test)]
    pub(crate) fn refresh_target_catalog(
        &self,
        descriptors: Vec<TargetProfileDescriptor>,
    ) -> AppResult<Arc<TargetCatalog>> {
        let catalog = TargetCatalog::from_descriptors(descriptors)?;
        self.reconcile_tenants_with_target_catalog(&catalog)?;
        self.target_catalog.replace(catalog);
        Ok(self.target_catalog.load())
    }

    pub(crate) fn refresh_target_catalog_from_disk(&self) -> AppResult<Arc<TargetCatalog>> {
        let catalog = TargetCatalog::load_with_overrides(&self.target_catalog_dir)?;
        self.reconcile_tenants_with_target_catalog(&catalog)?;
        self.target_catalog.replace(catalog);
        Ok(self.target_catalog.load())
    }

    fn reconcile_tenants_with_target_catalog(&self, catalog: &TargetCatalog) -> AppResult<()> {
        let pool = self.db.pool().clone();
        let principal_id = self.context().request_context.principal.id.clone();
        let catalog_for_seed = catalog.clone();
        self.run_sync(async move {
            let tenants =
                crate::backend::store::list_tenants_for_principal_sqlx(&pool, &principal_id)
                    .await?;
            for tenant in tenants {
                crate::backend::store::seed_tenant_defaults_sqlx_with_catalog(
                    &pool,
                    &tenant.id,
                    &catalog_for_seed,
                )
                .await?;
            }
            Ok::<(), AppError>(())
        })?;
        Ok(())
    }

    pub(crate) fn conversation_adapter_catalog(&self) -> Arc<ConversationAdapterCatalog> {
        self.context().conversation_adapter_catalog.clone()
    }

    pub(crate) fn builtin_conversation_adapters(&self) -> Arc<Vec<ConversationAdapter>> {
        self.builtin_conversation_adapters.clone()
    }

    pub(crate) fn refresh_conversation_adapter_catalog(&self) -> AppResult<()> {
        let _update_guard = self
            .context_update_gate
            .lock()
            .map_err(|_| AppError::Conflict("租户上下文更新锁不可用".to_string()))?;
        let current = self.context();
        let tenant_id = current.tenant.id.clone();
        let pool = self.pool().clone();
        let adapters = self.block_on(crate::backend::store::list_conversation_adapters_sqlx(
            &pool, &tenant_id,
        ))?;
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

    pub(crate) fn shutdown_with_grace(&self, grace: Duration) -> ShutdownReport {
        if !self.shutdown.begin() {
            return ShutdownReport::default();
        }
        let deadline = Instant::now() + grace;
        // Stop new task registration before any component begins to close.
        // Workers must converge and publish their final state while the
        // resident dispatcher is still alive; only then can the dispatcher
        // drain and the database close.
        self.stop_session_memory_coordinator();
        if let Ok(mut slot) = self.team_coordinator.lock() {
            if let Some(handle) = slot.take() {
                handle.stop();
            }
        }
        self.task_runtime.stop_accepting();
        let task_report = self
            .task_runtime
            .shutdown_with_grace(deadline.saturating_duration_since(Instant::now()));
        let dispatcher_report = self
            .dispatcher
            .lock()
            .ok()
            .and_then(|mut slot| slot.take())
            .map(|handle| {
                let remaining = deadline.saturating_duration_since(Instant::now());
                handle.stop_with_timeout(remaining)
            })
            .unwrap_or_default();
        let _ = self.block_on(self.db.pool().close());
        ShutdownReport {
            unfinished_task_ids: task_report.unfinished_task_ids,
            dispatcher_drained: dispatcher_report.drained,
            dispatcher_remaining_events: dispatcher_report.remaining_events,
            dispatcher_timed_out: dispatcher_report.timed_out,
        }
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
