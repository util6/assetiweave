use std::fmt;

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct PersistentExecutionBinding {
    pub(crate) tenant_id: String,
    pub(crate) execution_context_key: String,
    pub(crate) provider_session_id: String,
    pub(crate) agent_id: String,
    pub(crate) installation_id: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) workspace_path: String,
    pub(crate) binding_version: i64,
    pub(crate) provider_metadata_json: String,
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
