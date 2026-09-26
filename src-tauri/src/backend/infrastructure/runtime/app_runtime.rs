use super::session_streams;
pub(crate) use super::shutdown::*;
use crate::backend::{
    infrastructure::error::{InfraError, InfraResult},
    infrastructure::tasks::{self, TaskRuntime},
};
use arc_swap::ArcSwap;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::backend::infrastructure::target_catalog::TargetCatalog;
use crate::backend::{
    domain::{ConversationAdapter, ConversationAdapterCatalog, RequestContext, Tenant},
    infrastructure::agent_execution::AgentExecutionRuntime,
    infrastructure::agent_market::AgentRuntimeManager,
    infrastructure::events::{
        EventDispatcher, EventDispatcherHandle, EventDispatcherShutdownReport,
    },
    infrastructure::extensions::RegistrySnapshot,
    store::{self, Database},
};

#[cfg(test)]
use crate::backend::domain::ConversationAdapterTrustState;

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

/// 进程级共享资源宿主。所有请求复用其中的数据库池与 tokio Runtime。
pub(crate) struct AppRuntime {
    pub(super) db_path: PathBuf,
    pub(super) db: Database,
    pub(super) context: ArcSwap<RequestContextSnapshot>,
    pub(super) task_runtime: TaskRuntime,
    pub(super) context_update_gate: tokio::sync::Mutex<()>,
    pub(super) shutdown: ShutdownState,
    pub(super) shutdown_gate: tokio::sync::Mutex<()>,
    pub(super) coordinator_timed_out: AtomicBool,
    pub(super) dispatcher: Mutex<Option<EventDispatcherHandle>>,
    pub(super) session_memory_coordinator: Mutex<Option<SessionMemoryCoordinatorHandle>>,
    pub(super) session_streams: session_streams::SessionStreamRegistry,
    pub(super) target_catalog_dir: PathBuf,
    pub(super) target_catalog: RegistrySnapshot<TargetCatalog>,
    pub(super) builtin_conversation_adapters: Arc<Vec<ConversationAdapter>>,
    pub(super) config: Arc<super::config::RuntimeConfig>,
    pub(super) settings: ArcSwap<serde_json::Value>,
}

static PROCESS_RUNTIME: OnceLock<Arc<AppRuntime>> = OnceLock::new();

impl AppRuntime {
    pub(crate) async fn bootstrap(
        db_path: PathBuf,
        pool: sqlx::SqlitePool,
        target_catalog_dir: PathBuf,
        target_catalog: TargetCatalog,
        builtin_conversation_adapters: Vec<ConversationAdapter>,
    ) -> InfraResult<Arc<Self>> {
        let context = store::load_local_request_context_sqlx(&pool).await?;
        let tenant_id = context.tenant.id.clone();
        let conversation_adapters =
            crate::backend::store::list_conversation_adapters_sqlx(&pool, &tenant_id).await?;
        let workspace_root = db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("agent-executions");
        let agent_runtime_manager =
            Arc::new(AgentRuntimeManager::new(pool.clone(), workspace_root));
        let initial_settings =
            crate::backend::infrastructure::app_settings::load_or_import_app_settings_sqlx(&pool)
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
            crate::backend::infrastructure::app_settings::load_or_import_app_settings_sqlx(
                db.pool(),
            )
            .await
            .unwrap_or_else(|_| {
                crate::backend::infrastructure::app_settings::canonicalize_settings(
                    serde_json::json!({}),
                )
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

    pub(crate) async fn start_resident_services(self: &Arc<Self>) {
        self.start_agent_health_refresh();
        let dispatcher = Arc::new(EventDispatcher::with_consumers(
            self.db.pool().clone(),
            self.db_path.clone(),
            Vec::new(),
        ));
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

    pub(crate) async fn start_event_dispatcher(
        &self,
        consumers: Vec<Arc<dyn crate::backend::infrastructure::events::DomainEventConsumer>>,
    ) -> Result<(), InfraError> {
        if let Ok(mut slot) = self.dispatcher.lock() {
            if let Some(mut existing) = slot.take() {
                existing.stop().await;
            }
        }
        let dispatcher = Arc::new(EventDispatcher::with_consumers(
            self.db.pool().clone(),
            self.db_path.clone(),
            consumers,
        ));
        dispatcher.initialize_all_tenants().await?;
        if let Some(runtime_handle) = self.task_runtime.runtime_handle() {
            let handle = dispatcher.start(&runtime_handle);
            if let Ok(mut slot) = self.dispatcher.lock() {
                *slot = Some(handle);
            }
        }
        Ok(())
    }

    fn start_agent_health_refresh(&self) {
        let snapshot = self.context();
        let runtime_manager = snapshot.agent_runtime_manager.clone();
        let mut spec = tasks::TaskSpec::global(
            tasks::TaskKind::Other,
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
                    return Err(InfraError::Cancelled(
                        "Agent startup health refresh was cancelled".to_string(),
                    ));
                }
                let summary = runtime_manager
                    .refresh_installed_agent_health()
                    .await
                    .map_err(InfraError::external)?;
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

    pub(crate) fn context_update_gate(&self) -> &tokio::sync::Mutex<()> {
        &self.context_update_gate
    }

    /// Commit an already prepared tenant context snapshot into runtime.
    /// Returns the active tenant.
    pub(crate) fn commit_tenant_snapshot(&self, snapshot: RequestContextSnapshot) -> Tenant {
        let tenant = snapshot.tenant.clone();
        self.context.store(Arc::new(snapshot));
        tenant
    }

    pub(crate) fn app_settings_value(&self) -> serde_json::Value {
        (**self.settings.load()).clone()
    }

    /// Refresh the settings snapshot after Application completes startup
    /// migrations which may update the persisted settings document.
    pub(crate) async fn refresh_app_settings(&self) {
        let settings =
            crate::backend::infrastructure::app_settings::load_or_import_app_settings_sqlx(
                self.db.pool(),
            )
            .await
            .unwrap_or_else(|_| serde_json::json!({}));
        self.settings.store(Arc::new(settings));
    }

    pub(crate) fn update_app_settings_value(&self, new_settings: serde_json::Value) {
        self.settings.store(Arc::new(new_settings));
    }

    pub(crate) fn backend_settings(
        &self,
    ) -> InfraResult<crate::backend::infrastructure::app_settings::BackendSettings> {
        crate::backend::infrastructure::app_settings::BackendSettings::from_value(
            &self.app_settings_value(),
        )
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

    pub(crate) fn target_catalog(&self) -> Arc<TargetCatalog> {
        self.target_catalog.load()
    }

    pub(crate) fn target_catalog_dir(&self) -> &Path {
        &self.target_catalog_dir
    }

    /// Publish an already validated and reconciled catalog snapshot.
    pub(crate) fn publish_target_catalog(&self, catalog: TargetCatalog) -> Arc<TargetCatalog> {
        self.target_catalog.replace(catalog);
        self.target_catalog.load()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn conversation_adapter_catalog(&self) -> Arc<ConversationAdapterCatalog> {
        self.context().conversation_adapter_catalog.clone()
    }

    pub(crate) fn builtin_conversation_adapters(&self) -> Arc<Vec<ConversationAdapter>> {
        self.builtin_conversation_adapters.clone()
    }

    pub(crate) async fn refresh_conversation_adapter_catalog(&self) -> InfraResult<()> {
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
}

pub(crate) fn install_process_runtime(runtime: Arc<AppRuntime>) -> InfraResult<()> {
    PROCESS_RUNTIME
        .set(runtime)
        .map_err(|_| InfraError::Conflict("进程运行时已经初始化".to_string()))
}

pub(crate) fn current_process_runtime() -> Option<Arc<AppRuntime>> {
    PROCESS_RUNTIME.get().cloned()
}
