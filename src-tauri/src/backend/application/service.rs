use super::prelude::*;

pub(crate) struct AppService {
    /// 请求绑定的进程级运行时。生产与测试都必须通过同一运行时边界创建。
    pub(super) runtime: std::sync::Arc<crate::backend::runtime::AppRuntime>,
    pub(super) db: crate::backend::store::Database,
    pub(super) db_path: PathBuf,
    pub(super) context: RequestContext,
    pub(super) agent_runtime_manager:
        std::sync::Arc<crate::backend::agent_market::AgentRuntimeManager>,
    pub(super) agent_runtime:
        std::sync::Arc<dyn crate::backend::ai_execution::AgentExecutionRuntime>,
    pub(super) conversation_adapter_catalog:
        std::sync::Arc<crate::backend::conversations::ConversationAdapterCatalog>,
}

impl AppService {
    pub(crate) fn backend_settings(
        &self,
    ) -> AppResult<crate::backend::app_settings::BackendSettings> {
        crate::backend::app_settings::BackendSettings::from_value(
            &self.runtime.app_settings_value(),
        )
    }

    pub(crate) fn get_agent_session(
        &self,
        params: crate::backend::dto::AgentSessionGetParams,
    ) -> AppResult<crate::backend::dto::AgentSessionGetResult> {
        let session_ref = params.session_ref;
        if let Some(snapshot) = self
            .runtime
            .session_streams()
            .get_by_ref(&session_ref.value)
        {
            let requested_tenant = self.tenant_id();
            if let Some(meta_tenant) = &snapshot.metadata.tenant_id {
                if meta_tenant != requested_tenant {
                    return Ok(crate::backend::dto::AgentSessionGetResult::Unavailable(
                        crate::backend::dto::AgentSessionUnavailableView {
                            schema_version: 1,
                            session_ref,
                            state: "unavailable".to_string(),
                            reason: "notFoundOrExpired".to_string(),
                        },
                    ));
                }
            }
            Ok(crate::backend::dto::AgentSessionGetResult::Available(
                snapshot.to_view(),
            ))
        } else {
            Ok(crate::backend::dto::AgentSessionGetResult::Unavailable(
                crate::backend::dto::AgentSessionUnavailableView::new(session_ref),
            ))
        }
    }
}
