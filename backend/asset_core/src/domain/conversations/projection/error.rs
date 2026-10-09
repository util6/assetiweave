use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProjectionError {
    #[error("conversation content card schema_version must be {expected}")]
    UnsupportedSchemaVersion { expected: u64, actual: Option<u64> },

    #[error("conversation content card kind is required")]
    MissingCardKind,

    #[error("invalid conversation content card kind {kind:?}; {reason}")]
    InvalidCardKind { kind: String, reason: String },

    #[error("adapter {adapter_id} emitted undeclared conversation card kind {kind:?}")]
    UndeclaredCardKind { adapter_id: String, kind: String },

    #[error("unsupported conversation card renderer {renderer:?}")]
    UnsupportedRenderer { renderer: String },

    #[error("conversation card kind {kind:?} does not allow renderer {renderer:?}")]
    RendererNotAllowed { kind: String, renderer: String },

    #[error("adapter {adapter_id} must declare card_contract_version {expected} before emitting content_card")]
    MissingContractVersion { adapter_id: String, expected: u64 },

    #[error(
        "adapter {adapter_id} has ambiguous card kinds for legacy semantic role {semantic_role:?}"
    )]
    AmbiguousLegacySemanticRole {
        adapter_id: String,
        semantic_role: String,
    },

    #[error("structured content_card for kind {descriptor_kind:?} conflicts with legacy metadata content_card kind {legacy_kind:?}")]
    LegacyConflict {
        descriptor_kind: String,
        legacy_kind: String,
    },

    #[error("adapter card manifest validation failed: {0}")]
    ManifestValidation(String),

    #[error("invalid persisted conversation content card JSON: {0}")]
    InvalidPersistedCardJson(#[source] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

impl ProjectionError {
    pub fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }
}

impl PartialEq<&str> for ProjectionError {
    fn eq(&self, other: &&str) -> bool {
        self.to_string() == *other
    }
}

impl PartialEq<String> for ProjectionError {
    fn eq(&self, other: &String) -> bool {
        self.to_string() == *other
    }
}
