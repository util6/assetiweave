use super::prelude::*;

#[derive(Clone)]
pub struct AppService {
    /// 请求绑定的进程级运行时。生产与测试都必须通过同一运行时边界创建。
    pub(super) runtime: std::sync::Arc<crate::backend::infrastructure::runtime::AppRuntime>,
    pub(super) db: crate::backend::store::Database,
    pub(super) db_path: PathBuf,
    pub(super) context: RequestContext,
    pub(super) agent_runtime_manager:
        std::sync::Arc<crate::backend::infrastructure::agent_market::AgentRuntimeManager>,
    pub(super) agent_runtime:
        std::sync::Arc<dyn crate::backend::infrastructure::agent_execution::AgentExecutionRuntime>,
    pub(super) conversation_adapter_catalog:
        std::sync::Arc<crate::backend::domain::conversations::ConversationAdapterCatalog>,
}

impl AppService {
    pub fn backend_settings(
        &self,
    ) -> AppResult<crate::backend::infrastructure::app_settings::BackendSettings> {
        Ok(
            crate::backend::infrastructure::app_settings::BackendSettings::from_value(
                &self.runtime.app_settings_value(),
            )?,
        )
    }

    pub fn get_agent_session(
        &self,
        params: crate::backend::application::agents::AgentSessionGetParams,
    ) -> AppResult<crate::backend::application::agents::AgentSessionGetResult> {
        let session_ref = params.session_ref;
        if let Some(snapshot) = self
            .runtime
            .session_streams()
            .get_by_ref(&session_ref.value)
        {
            let requested_tenant = self.tenant_id();
            if let Some(meta_tenant) = &snapshot.metadata.tenant_id {
                if meta_tenant != requested_tenant {
                    return Ok(
                        crate::backend::application::agents::AgentSessionGetResult::Unavailable(
                            crate::backend::application::agents::AgentSessionUnavailableView {
                                schema_version: 1,
                                session_ref,
                                state: "unavailable".to_string(),
                                reason: "notFoundOrExpired".to_string(),
                            },
                        ),
                    );
                }
            }
            Ok(
                crate::backend::application::agents::AgentSessionGetResult::Available(
                    crate::backend::application::agents::AgentSessionView::from_entry_snapshot(
                        &snapshot,
                    ),
                ),
            )
        } else {
            Ok(
                crate::backend::application::agents::AgentSessionGetResult::Unavailable(
                    crate::backend::application::agents::AgentSessionUnavailableView::new(
                        session_ref,
                    ),
                ),
            )
        }
    }

    pub fn spawn_pipeline_task<T, F, Fut>(
        &self,
        spec: crate::backend::infrastructure::tasks::TaskSpec,
        task: F,
    ) -> AppResult<crate::backend::infrastructure::tasks::TaskHandle<T>>
    where
        T: serde::Serialize + Send + 'static,
        F: FnOnce(crate::backend::infrastructure::tasks::TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<
                Output = crate::backend::infrastructure::tasks::InfraResult<
                    crate::backend::infrastructure::tasks::TaskOutput<T>,
                >,
            > + Send
            + 'static,
    {
        Ok(self.runtime.task_runtime().run(spec, task)?)
    }

    /// 统一自适应任务执行门面 (ADR-0017 / Issue #79)
    ///
    /// 依据 `spec.admission` 的策略决定实际执行模式：
    /// - `Managed`：全量托管，分配后台流水线，广播 UI 事件，持久化 journal；
    /// - `Shadow`：影子观察，后台静默执行 (user_visible = false)，收集指标供策略决策；
    /// - `Direct`：受控穿透直跑，使用 TaskContext::direct() 原地执行，返回已完成的 Fast-path TaskHandle。
    pub async fn execute_task<T, F, Fut>(
        &self,
        mut spec: crate::backend::infrastructure::tasks::TaskSpec,
        task: F,
    ) -> AppResult<crate::backend::infrastructure::tasks::TaskHandle<T>>
    where
        T: serde::Serialize + Send + 'static,
        F: FnOnce(crate::backend::infrastructure::tasks::TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<
                Output = crate::backend::infrastructure::tasks::InfraResult<
                    crate::backend::infrastructure::tasks::TaskOutput<T>,
                >,
            > + Send
            + 'static,
    {
        use crate::backend::infrastructure::tasks::TaskAdmission;
        match spec.admission {
            TaskAdmission::Managed => self.spawn_pipeline_task(spec, task),
            TaskAdmission::Shadow => {
                spec.user_visible = Some(false);
                self.spawn_pipeline_task(spec, task)
            }
            TaskAdmission::Direct => {
                let task_id = spec
                    .task_id
                    .clone()
                    .unwrap_or_else(|| format!("direct:{}", uuid::Uuid::new_v4()));
                let context = crate::backend::infrastructure::tasks::TaskContext::direct();
                let output = task(context)
                    .await
                    .map_err(crate::backend::application::AppError::from)?;
                Ok(
                    crate::backend::infrastructure::tasks::TaskHandle::completed(
                        task_id,
                        output.data,
                    ),
                )
            }
        }
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
