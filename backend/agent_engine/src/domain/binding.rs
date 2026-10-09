use std::fmt;

#[derive(Clone, Eq, PartialEq)]
pub struct PersistentExecutionBinding {
    pub tenant_id: String,
    pub execution_context_key: String,
    pub provider_session_id: String,
    pub agent_id: String,
    pub installation_id: Option<String>,
    pub model: Option<String>,
    pub workspace_path: String,
    pub binding_version: i64,
    pub provider_metadata_json: String,
}

impl fmt::Debug for PersistentExecutionBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistentExecutionBinding")
            .field("tenant_id", &self.tenant_id)
            .field("execution_context_key", &self.execution_context_key)
            .field("provider_session_id", &"<redacted>")
            .field("agent_id", &self.agent_id)
            .field("installation_id", &self.installation_id)
            .field("model", &self.model.as_ref().map(|_| "<redacted>"))
            .field("workspace_path", &"<redacted>")
            .field("binding_version", &self.binding_version)
            .field("provider_metadata_json", &"<redacted>")
            .finish()
    }
}

#[async_trait::async_trait]
pub trait PersistentBindingStorage: Send + Sync {
    async fn load(
        &self,
        tenant_id: &str,
        execution_context_key: &str,
    ) -> Result<Option<PersistentExecutionBinding>, String>;

    async fn save(&self, binding: &PersistentExecutionBinding) -> Result<(), String>;
}
