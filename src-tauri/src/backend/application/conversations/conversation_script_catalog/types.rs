use super::helpers::*;
use crate::backend::application::prelude::*;
use crate::backend::domain::{
    ConversationAdapterPackageChangeAction, ConversationAdapterPackageChangeRisk,
    ConversationAdapterPackageOrigin, ConversationAdapterPackageRecordKind,
    ConversationAdapterRuntimeGateStatus, ConversationPackageUpdatePolicy,
};
use crate::backend::infrastructure::conversations::{
    ConversationAdapterPackageInstallSourceKind, ConversationAdapterPackageInstallSpec,
};

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageInspection {
    pub(crate) origin: ConversationAdapterPackageOrigin,
    pub(crate) package: Option<ConversationAdapterPackage>,
    pub(crate) adapter: Option<ConversationAdapter>,
    pub(crate) affected_sources: Vec<ConversationSource>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageChangePreflight {
    pub(crate) action: ConversationAdapterPackageChangeAction,
    pub(crate) origin: ConversationAdapterPackageOrigin,
    pub(crate) package_id: Option<String>,
    pub(crate) adapter_id: Option<String>,
    pub(crate) managed_paths: Vec<String>,
    pub(crate) affected_sources: Vec<ConversationSource>,
    pub(crate) task_conflicts: Vec<String>,
    pub(crate) preserves_conversation_records: bool,
    pub(crate) risk: ConversationAdapterPackageChangeRisk,
    pub(crate) confirmation_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ConversationScriptCatalog {
    #[serde(alias = "schemaVersion")]
    pub(crate) schema_version: u32,
    #[serde(default, alias = "updatedAt")]
    pub(crate) updated_at: Option<String>,
    #[serde(default)]
    pub(crate) items: Vec<ConversationScriptCatalogItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ConversationScriptCatalogItem {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    #[serde(alias = "recordKind")]
    pub(crate) record_kind: ConversationScriptRecordKind,
    #[serde(default)]
    pub(crate) provider: Option<String>,
    #[serde(default, alias = "adapterId")]
    pub(crate) adapter_id: Option<String>,
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default, alias = "homepageUrl")]
    pub(crate) homepage_url: Option<String>,
    #[serde(default, alias = "repositoryUrl")]
    pub(crate) repository_url: Option<String>,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    #[serde(default, alias = "manifestFile")]
    pub(crate) manifest_file: Option<String>,
    #[serde(default, alias = "packageManifestFile")]
    pub(crate) package_manifest_file: Option<String>,
    #[serde(default, alias = "expectedContentHash")]
    pub(crate) expected_content_hash: Option<String>,
    #[serde(default, alias = "expectedPackageHash")]
    pub(crate) expected_package_hash: Option<String>,
    #[serde(default, alias = "expectedArtifactHash")]
    pub(crate) expected_artifact_hash: Option<String>,
    #[serde(default, alias = "artifactSize")]
    pub(crate) artifact_size: Option<u64>,
    pub(crate) source: ConversationScriptCatalogSource,
}

impl ConversationScriptCatalogItem {
    pub(crate) fn to_install_spec(&self) -> ConversationAdapterPackageInstallSpec {
        ConversationAdapterPackageInstallSpec {
            id: self.id.clone(),
            name: self.name.clone(),
            version: self.version.clone(),
            record_kind: self.record_kind.as_package_record_kind(),
            provider: self.provider.clone(),
            adapter_id: self.adapter_id.clone(),
            description: self.description.clone(),
            homepage_url: self.homepage_url.clone(),
            repository_url: self.repository_url.clone(),
            tags: self.tags.clone(),
            manifest_file: self.manifest_file.clone(),
            package_manifest_file: self.package_manifest_file.clone(),
            expected_content_hash: self.expected_content_hash.clone(),
            expected_package_hash: self.expected_package_hash.clone(),
            expected_artifact_hash: self.expected_artifact_hash.clone(),
            artifact_size: self.artifact_size,
            source: crate::backend::infrastructure::conversations::ConversationAdapterPackageInstallSource {
                kind: match self.source.kind {
                    ConversationScriptCatalogSourceKind::Github => {
                        ConversationAdapterPackageInstallSourceKind::Github
                    }
                    ConversationScriptCatalogSourceKind::ArtifactZip => {
                        ConversationAdapterPackageInstallSourceKind::ArtifactZip
                    }
                    ConversationScriptCatalogSourceKind::LocalDirectory => {
                        ConversationAdapterPackageInstallSourceKind::LocalDirectory
                    }
                },
                url: self.source.url.clone(),
                branch: self.source.branch.clone(),
                path: self.source.path.clone(),
            },
        }
    }

    pub(crate) fn package_id(&self) -> &str {
        self.id.as_str()
    }

    pub(crate) fn adapter_key(&self) -> &str {
        self.adapter_id.as_deref().unwrap_or(self.id.as_str())
    }

    pub(crate) fn manifest_file_name(&self) -> AppResult<String> {
        let value = self
            .manifest_file
            .as_deref()
            .unwrap_or("conversation-adapter.json");
        clean_relative_file_name(value)
    }

    pub(crate) fn package_manifest_file_name(&self) -> AppResult<String> {
        let value = self
            .package_manifest_file
            .as_deref()
            .unwrap_or("conversation-adapter-package.json");
        clean_relative_file_name(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ConversationScriptCatalogSource {
    #[serde(rename = "type")]
    pub(crate) kind: ConversationScriptCatalogSourceKind,
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) branch: Option<String>,
    #[serde(default)]
    pub(crate) path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConversationScriptCatalogSourceKind {
    Github,
    ArtifactZip,
    LocalDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConversationScriptRecordKind {
    Session,
    Web,
}

impl ConversationScriptRecordKind {
    pub(crate) fn as_package_record_kind(self) -> ConversationAdapterPackageRecordKind {
        match self {
            Self::Session => ConversationAdapterPackageRecordKind::Session,
            Self::Web => ConversationAdapterPackageRecordKind::Web,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageCatalogEntry {
    pub(crate) item: ConversationScriptCatalogItem,
    pub(crate) installed: bool,
    pub(crate) update_available: bool,
    pub(crate) ahead_of_release: bool,
    pub(crate) runtime_ready: bool,
    pub(crate) status: String,
    pub(crate) installed_package: Option<ConversationAdapterPackage>,
    pub(crate) installed_adapter: Option<ConversationAdapter>,
    pub(crate) install_path: Option<String>,
    pub(crate) display_install_path: Option<String>,
    pub(crate) display_manifest_path: Option<String>,
    pub(crate) error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConversationScriptCatalogEntry {
    pub(crate) item: ConversationScriptCatalogItem,
    pub(crate) installed: bool,
    pub(crate) update_available: bool,
    pub(crate) installed_adapter: Option<ConversationAdapter>,
    pub(crate) install_path: Option<String>,
}

impl From<ConversationAdapterPackageCatalogEntry> for ConversationScriptCatalogEntry {
    fn from(entry: ConversationAdapterPackageCatalogEntry) -> Self {
        Self {
            item: entry.item,
            installed: entry.installed,
            update_available: entry.update_available,
            installed_adapter: entry.installed_adapter,
            install_path: entry.install_path,
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug)]
pub(crate) struct GitHubCatalogLocation {
    pub(crate) repo_url: String,
    pub(crate) branch: Option<String>,
    pub(crate) path: Option<String>,
}
