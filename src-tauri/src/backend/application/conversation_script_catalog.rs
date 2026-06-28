use super::prelude::*;

const DEFAULT_CONVERSATION_SCRIPT_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/util6/assetiweave/main/parser-catalog/catalog.json";
const LOCAL_DEFAULT_CONVERSATION_SCRIPT_CATALOG: &str =
    include_str!("../../../../parser-catalog/catalog.json");
const CONVERSATION_SCRIPT_SECURITY_NOTICE: &str =
    "Review remote conversation script contents before installing; AssetIWeave registers the downloaded adapter as trusted for local execution.";

impl AppService {
    pub(crate) fn list_conversation_script_catalog(
        &self,
        params: ConversationScriptCatalogParams,
    ) -> AppResult<Vec<ConversationScriptCatalogEntry>> {
        let catalog = load_conversation_script_catalog(params.catalog_url.as_deref())?;
        let adapters = self.list_conversation_adapters()?;
        Ok(resolve_conversation_script_catalog_entries(
            catalog.items,
            &adapters,
        ))
    }

    pub(crate) fn install_conversation_script(
        &self,
        params: ConversationScriptInstallParams,
    ) -> AppResult<Value> {
        if !params.dry_run && !params.yes {
            return Err("conversation script install requires --yes".to_string());
        }

        let catalog = load_conversation_script_catalog(params.catalog_url.as_deref())?;
        let item_id = params.item_id.trim();
        let item = catalog
            .items
            .into_iter()
            .find(|item| item.id == item_id)
            .ok_or_else(|| format!("conversation script catalog item not found: {item_id}"))?;
        validate_conversation_script_catalog_item(&item)?;
        let install_dir = conversation_script_install_dir(&item)?;
        let manifest_file = item.manifest_file_name()?;
        let manifest_path = install_dir.join(&manifest_file);

        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "installed": false,
                "item": item,
                "install_path": install_dir,
                "manifest_path": manifest_path,
                "security_notice": CONVERSATION_SCRIPT_SECURITY_NOTICE,
            }));
        }

        install_conversation_script_files(&item, &install_dir)?;
        let validation = crate::backend::conversations::validate_external_adapter(
            crate::backend::conversations::ExternalAdapterValidateParams {
                manifest_path: manifest_path.to_string_lossy().to_string(),
            },
        )?;
        validate_installed_manifest_for_catalog_item(&item, &validation)?;

        let preview = crate::backend::conversations::register_external_adapter(
            crate::backend::conversations::ExternalAdapterRegisterParams {
                manifest_path: manifest_path.to_string_lossy().to_string(),
                dry_run: false,
                yes: true,
            },
        )?;
        let adapter =
            crate::backend::conversations::adapter_from_registration_preview(preview.clone())?;
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let adapter_to_save = adapter.clone();
        self.db.block_on(async move {
            crate::backend::store::upsert_conversation_adapter_sqlx(
                &pool,
                &tenant_id,
                &adapter_to_save,
            )
            .await
        })?;

        Ok(json!({
            "dry_run": false,
            "installed": true,
            "item": item,
            "install_path": install_dir,
            "manifest_path": manifest_path,
            "adapter": adapter,
            "validation": validation,
            "security_notice": CONVERSATION_SCRIPT_SECURITY_NOTICE,
        }))
    }
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
    #[serde(default, alias = "expectedContentHash")]
    pub(crate) expected_content_hash: Option<String>,
    pub(crate) source: ConversationScriptCatalogSource,
}

impl ConversationScriptCatalogItem {
    fn adapter_key(&self) -> &str {
        self.adapter_id.as_deref().unwrap_or(self.id.as_str())
    }

    fn manifest_file_name(&self) -> AppResult<String> {
        let value = self
            .manifest_file
            .as_deref()
            .unwrap_or("conversation-adapter.json");
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConversationScriptRecordKind {
    Session,
    Web,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConversationScriptCatalogEntry {
    pub(crate) item: ConversationScriptCatalogItem,
    pub(crate) installed: bool,
    pub(crate) update_available: bool,
    pub(crate) installed_adapter: Option<ConversationAdapter>,
    pub(crate) install_path: Option<String>,
}

#[derive(Debug)]
struct GitHubCatalogLocation {
    repo_url: String,
    branch: Option<String>,
    path: Option<String>,
}

impl GitHubCatalogLocation {
    fn source_dir(&self, staging_dir: &Path) -> PathBuf {
        self.path
            .as_deref()
            .map(|path| staging_dir.join(path))
            .unwrap_or_else(|| staging_dir.to_path_buf())
    }
}

fn load_conversation_script_catalog(
    catalog_url: Option<&str>,
) -> AppResult<ConversationScriptCatalog> {
    let catalog_url = catalog_url
        .and_then(clean_non_empty_string)
        .unwrap_or_else(|| DEFAULT_CONVERSATION_SCRIPT_CATALOG_URL.to_string());
    let text = if catalog_url.starts_with("https://") || catalog_url.starts_with("http://") {
        match fetch_catalog_text(&catalog_url) {
            Ok(text) => text,
            Err(error) if catalog_url == DEFAULT_CONVERSATION_SCRIPT_CATALOG_URL => {
                read_local_default_catalog().map_err(|fallback_error| {
                    format!("{error}; local default catalog fallback failed: {fallback_error}")
                })?
            }
            Err(error) => return Err(error),
        }
    } else {
        let path = crate::backend::path_utils::expand_path(&catalog_url)?;
        fs::read_to_string(&path)
            .map_err(|error| format!("read conversation script catalog failed: {error}"))?
    };
    let catalog: ConversationScriptCatalog = serde_json::from_str(&text)
        .map_err(|error| format!("conversation script catalog was not valid JSON: {error}"))?;
    validate_conversation_script_catalog(&catalog)?;
    Ok(catalog)
}

fn fetch_catalog_text(url: &str) -> AppResult<String> {
    let response = ureq::get(url)
        .set("User-Agent", "AssetIWeave/0.3 conversation-script-catalog")
        .call()
        .map_err(|error| format!("conversation script catalog request failed: {error}"))?;
    response
        .into_string()
        .map_err(|error| format!("conversation script catalog response was not text: {error}"))
}

fn read_local_default_catalog() -> AppResult<String> {
    Ok(LOCAL_DEFAULT_CONVERSATION_SCRIPT_CATALOG.to_string())
}

fn validate_conversation_script_catalog(catalog: &ConversationScriptCatalog) -> AppResult<()> {
    if catalog.schema_version != 1 {
        return Err("conversation script catalog schema_version must be 1".to_string());
    }
    let mut seen_ids = HashSet::new();
    for item in &catalog.items {
        validate_conversation_script_catalog_item(item)?;
        if !seen_ids.insert(item.id.clone()) {
            return Err(format!(
                "duplicate conversation script catalog item: {}",
                item.id
            ));
        }
    }
    Ok(())
}

fn validate_conversation_script_catalog_item(
    item: &ConversationScriptCatalogItem,
) -> AppResult<()> {
    if item.id.trim().is_empty() {
        return Err("conversation script catalog item id is required".to_string());
    }
    let slug = slug_path_segment(&item.id);
    if slug.is_empty() || slug != item.id {
        return Err(format!(
            "conversation script catalog item id must be a safe path segment: {}",
            item.id
        ));
    }
    if item.name.trim().is_empty() {
        return Err(format!(
            "conversation script catalog item name is required: {}",
            item.id
        ));
    }
    if item.version.trim().is_empty() {
        return Err(format!(
            "conversation script catalog item version is required: {}",
            item.id
        ));
    }
    if let Some(adapter_id) = item.adapter_id.as_deref() {
        if adapter_id.trim().is_empty() {
            return Err(format!(
                "conversation script catalog item adapter_id must not be empty: {}",
                item.id
            ));
        }
    }
    item.manifest_file_name()?;
    parse_github_catalog_location(&item.source)?;
    Ok(())
}

fn resolve_conversation_script_catalog_entries(
    items: Vec<ConversationScriptCatalogItem>,
    adapters: &[ConversationAdapter],
) -> Vec<ConversationScriptCatalogEntry> {
    items
        .into_iter()
        .map(|item| {
            let installed_adapter = adapters
                .iter()
                .find(|adapter| adapter.id == item.adapter_key())
                .cloned();
            let install_path = installed_adapter
                .as_ref()
                .and_then(|adapter| adapter.manifest_path.as_deref())
                .and_then(|path| Path::new(path).parent().map(|parent| parent.to_path_buf()))
                .map(|path| path.to_string_lossy().to_string());
            let update_available = installed_adapter
                .as_ref()
                .is_some_and(|adapter| adapter.version != item.version);
            ConversationScriptCatalogEntry {
                item,
                installed: installed_adapter.is_some(),
                update_available,
                installed_adapter,
                install_path,
            }
        })
        .collect()
}

fn install_conversation_script_files(
    item: &ConversationScriptCatalogItem,
    install_dir: &Path,
) -> AppResult<()> {
    let location = parse_github_catalog_location(&item.source)?;
    let staging_dir = conversation_script_staging_dir(item)?;
    clone_github_catalog_source(&location, &staging_dir)?;
    let source_dir = location.source_dir(&staging_dir);
    if !source_dir.is_dir() {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(format!(
            "conversation script source path is not a directory: {}",
            source_dir.display()
        ));
    }

    let manifest_file = item.manifest_file_name()?;
    if !source_dir.join(&manifest_file).is_file() {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(format!(
            "conversation script source does not contain {}: {}",
            manifest_file,
            source_dir.display()
        ));
    }

    let temp_dir = install_dir.with_file_name(format!(
        ".{}-{}.tmp",
        install_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("conversation-script"),
        short_uuid()
    ));
    if temp_dir.exists() {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(format!(
            "conversation script temporary install path already exists: {}",
            temp_dir.display()
        ));
    }
    capabilities::copy_dir(&source_dir, &temp_dir)?;
    let temp_manifest_path = temp_dir.join(&manifest_file);
    let validation = crate::backend::conversations::validate_external_adapter(
        crate::backend::conversations::ExternalAdapterValidateParams {
            manifest_path: temp_manifest_path.to_string_lossy().to_string(),
        },
    );
    if let Err(error) = validation
        .and_then(|validation| validate_installed_manifest_for_catalog_item(item, &validation))
    {
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(error);
    }

    if install_dir.exists() {
        fs::remove_dir_all(install_dir).map_err(|error| error.to_string())?;
    }
    if let Some(parent) = install_dir.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::rename(&temp_dir, install_dir).map_err(|error| error.to_string())?;
    let _ = fs::remove_dir_all(&staging_dir);
    Ok(())
}

fn validate_installed_manifest_for_catalog_item(
    item: &ConversationScriptCatalogItem,
    validation: &crate::backend::conversations::ExternalAdapterValidationResult,
) -> AppResult<()> {
    if validation.manifest.id != item.adapter_key() {
        return Err(format!(
            "installed adapter id {} does not match catalog adapter id {}",
            validation.manifest.id,
            item.adapter_key()
        ));
    }
    if !validation
        .manifest
        .capabilities
        .iter()
        .any(|capability| capability == "read_session")
    {
        return Err(format!(
            "conversation script {} must declare read_session",
            item.id
        ));
    }
    if item.record_kind == ConversationScriptRecordKind::Web
        && !validation
            .manifest
            .capabilities
            .iter()
            .any(|capability| capability == "web_records")
    {
        return Err(format!(
            "web conversation script {} must declare web_records",
            item.id
        ));
    }
    if let Some(expected) = item
        .expected_content_hash
        .as_deref()
        .and_then(clean_non_empty_string)
    {
        if validation.content_hash != expected {
            return Err(format!(
                "conversation script {} content hash mismatch",
                item.id
            ));
        }
    }
    Ok(())
}

fn conversation_script_install_dir(item: &ConversationScriptCatalogItem) -> AppResult<PathBuf> {
    Ok(crate::backend::app_settings::conversation_adapter_dir()?
        .join("market")
        .join(slug_path_segment(&item.id)))
}

fn conversation_script_staging_dir(item: &ConversationScriptCatalogItem) -> AppResult<PathBuf> {
    Ok(crate::backend::app_settings::conversation_adapter_dir()?
        .join("staging")
        .join(format!("{}-{}", slug_path_segment(&item.id), short_uuid())))
}

fn parse_github_catalog_location(
    source: &ConversationScriptCatalogSource,
) -> AppResult<GitHubCatalogLocation> {
    if source.kind != ConversationScriptCatalogSourceKind::Github {
        return Err("conversation script source must be github".to_string());
    }
    let trimmed = source
        .url
        .trim()
        .split('#')
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    let path = trimmed.strip_prefix("https://github.com/").ok_or_else(|| {
        "conversation script source only supports https://github.com URLs".to_string()
    })?;
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err("GitHub URL must include owner and repository".to_string());
    }

    let owner = parts[0];
    let repo = parts[1].trim_end_matches(".git");
    if repo.is_empty() {
        return Err("GitHub URL must include repository name".to_string());
    }

    let mut branch = source.branch.as_deref().and_then(clean_non_empty_string);
    let mut source_path = source.path.as_deref().and_then(clean_catalog_subpath);
    if source_path.is_none() && parts.len() >= 4 && matches!(parts[2], "tree" | "blob") {
        branch = branch.or_else(|| clean_non_empty_string(parts[3]));
        if parts.len() > 4 {
            source_path = clean_catalog_subpath(&parts[4..].join("/"));
        }
    }

    Ok(GitHubCatalogLocation {
        repo_url: format!("https://github.com/{owner}/{repo}.git"),
        branch,
        path: source_path,
    })
}

fn clone_github_catalog_source(location: &GitHubCatalogLocation, target: &Path) -> AppResult<()> {
    if target.exists() {
        return Err(format!(
            "conversation script staging path already exists: {}",
            target.display()
        ));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let mut command = Command::new("git");
    command.arg("clone").arg("--depth").arg("1");
    if let Some(branch) = &location.branch {
        command.arg("--branch").arg(branch);
    }
    let output = command
        .arg(&location.repo_url)
        .arg(target)
        .output()
        .map_err(|error| format!("failed to run git clone: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git clone failed: {stderr}"));
    }
    Ok(())
}

fn clean_non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn clean_catalog_subpath(value: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in value.trim().trim_matches('/').split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part == ".git" || part.contains('\\') || part.contains(':') {
            return None;
        }
        parts.push(part);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn clean_relative_file_name(value: &str) -> AppResult<String> {
    let trimmed = value.trim().trim_matches('/');
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains(':')
    {
        return Err(format!("manifest_file must be a file name: {value}"));
    }
    Ok(trimmed.to_string())
}

fn short_uuid() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog_item(id: &str, adapter_id: Option<&str>) -> ConversationScriptCatalogItem {
        ConversationScriptCatalogItem {
            id: id.to_string(),
            name: "Codex Session Parser".to_string(),
            version: "1.0.0".to_string(),
            record_kind: ConversationScriptRecordKind::Session,
            provider: Some("codex".to_string()),
            adapter_id: adapter_id.map(str::to_string),
            description: None,
            homepage_url: None,
            repository_url: None,
            tags: Vec::new(),
            manifest_file: None,
            expected_content_hash: None,
            source: ConversationScriptCatalogSource {
                kind: ConversationScriptCatalogSourceKind::Github,
                url: "https://github.com/util6/assetiweave/tree/main/src-tauri/bundled/conversation-adapters/codex".to_string(),
                branch: None,
                path: None,
            },
        }
    }

    fn adapter(id: &str, version: &str) -> ConversationAdapter {
        ConversationAdapter {
            id: id.to_string(),
            name: "Codex".to_string(),
            kind: crate::backend::models::ConversationAdapterKind::External,
            version: version.to_string(),
            enabled: true,
            manifest_path: Some("/tmp/codex/conversation-adapter.json".to_string()),
            executable_path: Some("/tmp/codex/adapter.mjs".to_string()),
            content_hash: Some("hash".to_string()),
            trusted_hash: Some("hash".to_string()),
            trust_state: crate::backend::models::ConversationAdapterTrustState::Trusted,
            protocol_version: Some(1),
            capabilities: vec!["read_session".to_string()],
            input_kinds: Vec::new(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn resolves_installed_state_from_declared_adapter_id() {
        let entries = resolve_conversation_script_catalog_entries(
            vec![catalog_item("codex-session", Some("codex"))],
            &[adapter("codex", "1.0.0")],
        );

        assert!(entries[0].installed);
        assert!(!entries[0].update_available);
        assert_eq!(entries[0].installed_adapter.as_ref().unwrap().id, "codex");
    }

    #[test]
    fn marks_installed_adapter_with_different_version_as_update_available() {
        let entries = resolve_conversation_script_catalog_entries(
            vec![catalog_item("codex-session", Some("codex"))],
            &[adapter("codex", "0.9.0")],
        );

        assert!(entries[0].installed);
        assert!(entries[0].update_available);
    }

    #[test]
    fn parses_github_tree_url_into_repo_branch_and_path() {
        let source = ConversationScriptCatalogSource {
            kind: ConversationScriptCatalogSourceKind::Github,
            url: "https://github.com/util6/assetiweave/tree/main/parser-catalog/adapters/codex"
                .to_string(),
            branch: None,
            path: None,
        };

        let location = parse_github_catalog_location(&source).expect("parse GitHub tree URL");

        assert_eq!(
            location.repo_url,
            "https://github.com/util6/assetiweave.git"
        );
        assert_eq!(location.branch.as_deref(), Some("main"));
        assert_eq!(
            location.path.as_deref(),
            Some("parser-catalog/adapters/codex"),
        );
    }

    #[test]
    fn rejects_unsafe_manifest_file_names() {
        let mut item = catalog_item("codex-session", Some("codex"));
        item.manifest_file = Some("../conversation-adapter.json".to_string());

        assert!(validate_conversation_script_catalog_item(&item).is_err());
    }
}
