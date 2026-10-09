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
pub struct ConversationAdapterPackageInspection {
    pub origin: ConversationAdapterPackageOrigin,
    pub package: Option<ConversationAdapterPackage>,
    pub adapter: Option<ConversationAdapter>,
    pub affected_sources: Vec<ConversationSource>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConversationAdapterPackageChangePreflight {
    pub action: ConversationAdapterPackageChangeAction,
    pub origin: ConversationAdapterPackageOrigin,
    pub package_id: Option<String>,
    pub adapter_id: Option<String>,
    pub managed_paths: Vec<String>,
    pub affected_sources: Vec<ConversationSource>,
    pub task_conflicts: Vec<String>,
    pub preserves_conversation_records: bool,
    pub risk: ConversationAdapterPackageChangeRisk,
    pub confirmation_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConversationScriptCatalog {
    #[serde(alias = "schemaVersion")]
    pub schema_version: u32,
    #[serde(default, alias = "updatedAt")]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub items: Vec<ConversationScriptCatalogItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConversationScriptCatalogItem {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(alias = "recordKind")]
    pub record_kind: ConversationScriptRecordKind,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default, alias = "adapterId")]
    pub adapter_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, alias = "homepageUrl")]
    pub homepage_url: Option<String>,
    #[serde(default, alias = "repositoryUrl")]
    pub repository_url: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, alias = "manifestFile")]
    pub manifest_file: Option<String>,
    #[serde(default, alias = "packageManifestFile")]
    pub package_manifest_file: Option<String>,
    #[serde(default, alias = "expectedContentHash")]
    pub expected_content_hash: Option<String>,
    #[serde(default, alias = "expectedPackageHash")]
    pub expected_package_hash: Option<String>,
    #[serde(default, alias = "expectedArtifactHash")]
    pub expected_artifact_hash: Option<String>,
    #[serde(default, alias = "artifactSize")]
    pub artifact_size: Option<u64>,
    pub source: ConversationScriptCatalogSource,
}

impl ConversationScriptCatalogItem {
    pub fn to_install_spec(&self) -> ConversationAdapterPackageInstallSpec {
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

    pub fn package_id(&self) -> &str {
        self.id.as_str()
    }

    pub fn adapter_key(&self) -> &str {
        self.adapter_id.as_deref().unwrap_or(self.id.as_str())
    }

    pub fn manifest_file_name(&self) -> AppResult<String> {
        let value = self
            .manifest_file
            .as_deref()
            .unwrap_or("conversation-adapter.json");
        clean_relative_file_name(value)
    }

    pub fn package_manifest_file_name(&self) -> AppResult<String> {
        let value = self
            .package_manifest_file
            .as_deref()
            .unwrap_or("conversation-adapter-package.json");
        clean_relative_file_name(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConversationScriptCatalogSource {
    #[serde(rename = "type")]
    pub kind: ConversationScriptCatalogSourceKind,
    pub url: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationScriptCatalogSourceKind {
    Github,
    ArtifactZip,
    LocalDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationScriptRecordKind {
    Session,
    Web,
}

impl ConversationScriptRecordKind {
    pub fn as_package_record_kind(self) -> ConversationAdapterPackageRecordKind {
        match self {
            Self::Session => ConversationAdapterPackageRecordKind::Session,
            Self::Web => ConversationAdapterPackageRecordKind::Web,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConversationAdapterPackageCatalogEntry {
    pub item: ConversationScriptCatalogItem,
    pub installed: bool,
    pub update_available: bool,
    pub ahead_of_release: bool,
    pub runtime_ready: bool,
    pub status: String,
    pub installed_package: Option<ConversationAdapterPackage>,
    pub installed_adapter: Option<ConversationAdapter>,
    pub install_path: Option<String>,
    pub display_install_path: Option<String>,
    pub display_manifest_path: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConversationScriptCatalogEntry {
    pub item: ConversationScriptCatalogItem,
    pub installed: bool,
    pub update_available: bool,
    pub installed_adapter: Option<ConversationAdapter>,
    pub install_path: Option<String>,
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
pub struct GitHubCatalogLocation {
    pub repo_url: String,
    pub branch: Option<String>,
    pub path: Option<String>,
}
