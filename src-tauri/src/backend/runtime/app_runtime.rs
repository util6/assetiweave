use super::{tasks::TaskRuntime, AppError, AppResult, RuntimeLocks};
use arc_swap::ArcSwap;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use crate::backend::{
    agent_market::AgentRuntimeManager,
    ai_execution::AgentExecutionRuntime,
    conversations::ConversationAdapterCatalog,
    events::{EventDispatcher, EventDispatcherHandle},
    extension_kernel::RegistrySnapshot,
    models::{RequestContext, Tenant},
    path_utils::ensure_app_library_dirs,
    store::{self, Database},
    target_catalog::TargetCatalog,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeRole {
    ResidentHost,
    OneShot,
}

/// 绑定到一次请求的不可变上下文快照。
#[derive(Debug, Clone)]
pub(crate) struct RequestContextSnapshot {
    pub(crate) tenant: Tenant,
    pub(crate) generation: u64,
    pub(crate) request_context: RequestContext,
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

    pub(crate) fn accepting(&self) -> bool {
        self.accepting.load(Ordering::Acquire)
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
    generation: AtomicU64,
    agent_runtime_manager: Arc<AgentRuntimeManager>,
    agent_runtime: Arc<dyn AgentExecutionRuntime>,
    locks: RuntimeLocks,
    task_runtime: TaskRuntime,
    shutdown: ShutdownState,
    role: RuntimeRole,
    dispatcher: Mutex<Option<EventDispatcherHandle>>,
    target_catalog: RegistrySnapshot<TargetCatalog>,
    conversation_adapter_catalog: RegistrySnapshot<ConversationAdapterCatalog>,
}

static PROCESS_RUNTIME: OnceLock<Arc<AppRuntime>> = OnceLock::new();

impl AppRuntime {
    pub(crate) fn bootstrap(db_path: PathBuf, role: RuntimeRole) -> AppResult<Arc<Self>> {
        let runtime = crate::backend::store::build_runtime().map_err(AppError::Legacy)?;
        let pool = runtime
            .block_on(store::open_migrated_pool(&db_path))
            .map_err(AppError::Legacy)?;
        ensure_app_library_dirs().map_err(AppError::Legacy)?;

        // Bootstrap is the only production path that opens the migrated pool. The
        // old Database::open_initialized remains a test/migration compatibility API.
        runtime
            .block_on(store::seed_defaults_sqlx(&pool))
            .map_err(AppError::Legacy)?;
        let context = runtime
            .block_on(store::load_local_request_context_sqlx(&pool))
            .map_err(AppError::Legacy)?;
        let tenant_id = context.tenant.id.clone();
        runtime.block_on(
            crate::backend::bootstrap::materialize_and_seed_builtin_adapters(&pool, &tenant_id),
        )?;
        let conversation_adapters = runtime
            .block_on(crate::backend::store::list_conversation_adapters_sqlx(
                &pool, &tenant_id,
            ))
            .map_err(AppError::Legacy)?;
        let workspace_root = db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("agent-executions");
        let agent_runtime_manager =
            Arc::new(AgentRuntimeManager::new(pool.clone(), workspace_root));
        let runtime_root = crate::backend::agent_market::default_runtime_root()
            .map_err(|error| AppError::Legacy(error.to_string()))?;
        runtime
            .block_on(agent_runtime_manager.recover_startup(&tenant_id, &runtime_root))
            .map_err(AppError::Legacy)?;
        let migration_scope = db_path.to_string_lossy().to_string();
        if let Err(error) =
            runtime.block_on(crate::backend::agent_market::migrate_legacy_assignments(
                pool.clone(),
                agent_runtime_manager.clone(),
                &tenant_id,
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
            .block_on(agent_runtime_manager.reload(&tenant_id))
            .map_err(AppError::Legacy)?;

        let task_runtime = TaskRuntime::with_runtime_handle(runtime.handle().clone());
        let target_catalog = TargetCatalog::builtin().map_err(AppError::Legacy)?;
        let db = Database::from_parts(pool, runtime);
        let snapshot = RequestContextSnapshot {
            tenant: context.tenant.clone(),
            generation: 0,
            request_context: context,
        };
        let app_runtime = Arc::new(Self {
            db_path,
            db,
            context: ArcSwap::from_pointee(snapshot),
            generation: AtomicU64::new(0),
            agent_runtime: agent_runtime_manager.runtime(),
            agent_runtime_manager,
            locks: RuntimeLocks::default(),
            task_runtime,
            shutdown: ShutdownState::new(),
            role,
            dispatcher: Mutex::new(None),
            target_catalog: RegistrySnapshot::new(target_catalog),
            conversation_adapter_catalog: RegistrySnapshot::new(ConversationAdapterCatalog::new(
                conversation_adapters,
            )),
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
        let adapters = db
            .block_on(crate::backend::store::list_conversation_adapters_sqlx(
                db.pool(),
                &context.tenant.id,
            ))
            .unwrap_or_default();
        Arc::new(Self {
            db_path,
            db,
            context: ArcSwap::from_pointee(RequestContextSnapshot {
                tenant: context.tenant.clone(),
                generation: 0,
                request_context: context,
            }),
            generation: AtomicU64::new(0),
            agent_runtime_manager,
            agent_runtime,
            locks: RuntimeLocks::default(),
            task_runtime: TaskRuntime::new(),
            shutdown: ShutdownState::new(),
            role: RuntimeRole::OneShot,
            dispatcher: Mutex::new(None),
            target_catalog: RegistrySnapshot::new(
                TargetCatalog::builtin().expect("test target catalog must be valid"),
            ),
            conversation_adapter_catalog: RegistrySnapshot::new(ConversationAdapterCatalog::new(
                adapters,
            )),
        })
    }

    fn start_resident_services(&self) {
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

    pub(crate) fn db(&self) -> &Database {
        &self.db
    }
    pub(crate) fn pool(&self) -> &sqlx::SqlitePool {
        self.db.pool()
    }
    pub(crate) fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        self.db.block_on(future)
    }
    pub(crate) fn db_path(&self) -> &Path {
        &self.db_path
    }
    pub(crate) fn context(&self) -> Arc<RequestContextSnapshot> {
        self.context.load_full()
    }
    pub(crate) fn agent_runtime_manager(&self) -> Arc<AgentRuntimeManager> {
        self.agent_runtime_manager.clone()
    }
    pub(crate) fn agent_runtime(&self) -> Arc<dyn AgentExecutionRuntime> {
        self.agent_runtime.clone()
    }
    pub(crate) fn locks(&self) -> &RuntimeLocks {
        &self.locks
    }
    pub(crate) fn task_runtime(&self) -> &TaskRuntime {
        &self.task_runtime
    }
    pub(crate) fn role(&self) -> RuntimeRole {
        self.role
    }
    pub(crate) fn shutdown_state(&self) -> &ShutdownState {
        &self.shutdown
    }

    pub(crate) fn target_catalog(&self) -> Arc<TargetCatalog> {
        self.target_catalog.load()
    }

    pub(crate) fn conversation_adapter_catalog(&self) -> Arc<ConversationAdapterCatalog> {
        self.conversation_adapter_catalog.load()
    }

    pub(crate) fn refresh_conversation_adapter_catalog(&self) -> AppResult<()> {
        let tenant_id = self.context().tenant.id.clone();
        let pool = self.pool().clone();
        let adapters = self
            .block_on(crate::backend::store::list_conversation_adapters_sqlx(
                &pool, &tenant_id,
            ))
            .map_err(AppError::Legacy)?;
        self.conversation_adapter_catalog
            .replace(ConversationAdapterCatalog::new(adapters));
        Ok(())
    }

    pub(crate) fn notify_domain_events(&self) {
        if let Ok(slot) = self.dispatcher.lock() {
            if let Some(handle) = slot.as_ref() {
                handle.notify();
            }
        }
    }

    pub(crate) fn refresh_context(&self) -> AppResult<Arc<RequestContextSnapshot>> {
        let pool = self.pool().clone();
        let context = self
            .block_on(store::load_local_request_context_sqlx(&pool))
            .map_err(AppError::Legacy)?;
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let snapshot = Arc::new(RequestContextSnapshot {
            tenant: context.tenant.clone(),
            generation,
            request_context: context,
        });
        self.context.store(snapshot.clone());
        let adapters = self
            .block_on(crate::backend::store::list_conversation_adapters_sqlx(
                self.pool(),
                &snapshot.tenant.id,
            ))
            .map_err(AppError::Legacy)?;
        self.conversation_adapter_catalog
            .replace(ConversationAdapterCatalog::new(adapters));
        Ok(snapshot)
    }

    pub(crate) fn shutdown(&self) -> ShutdownReport {
        self.shutdown_with_grace(Duration::from_secs(5))
    }

    pub(crate) fn shutdown_with_grace(&self, grace: Duration) -> ShutdownReport {
        if !self.shutdown.begin() {
            return ShutdownReport::default();
        }
        let deadline = Instant::now() + grace;
        // Dispatcher drain is deliberately before task shutdown.
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
        let task_report = self
            .task_runtime
            .shutdown_with_grace(deadline.saturating_duration_since(Instant::now()));
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
