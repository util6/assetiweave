use super::prelude::*;
use walkdir::WalkDir;

const PACKAGE_MANIFEST_FILE: &str = "conversation-adapter-package.json";
const SUPPORTED_CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageManifest {
    #[serde(alias = "schemaVersion")]
    pub(crate) schema_version: u32,
    #[serde(alias = "packageId")]
    pub(crate) package_id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    #[serde(alias = "minCoreVersion")]
    pub(crate) min_core_version: String,
    #[serde(alias = "recordKind")]
    pub(crate) record_kind: ConversationAdapterPackageRecordKind,
    #[serde(alias = "adapterManifest")]
    pub(crate) adapter_manifest: String,
    #[serde(default)]
    pub(crate) capabilities: Vec<String>,
    pub(crate) runtime: ConversationAdapterPackageRuntime,
    #[serde(default)]
    pub(crate) changelog: Vec<ConversationAdapterPackageChangelogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageRuntime {
    pub(crate) protocol: ConversationAdapterPackageRuntimeProtocol,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ConversationAdapterPackageRuntimeProtocol {
    StdioNdjsonV1,
    HttpJsonV1,
}

impl ConversationAdapterPackageRuntimeProtocol {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::StdioNdjsonV1 => "stdio-ndjson-v1",
            Self::HttpJsonV1 => "http-json-v1",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageChangelogEntry {
    pub(crate) version: String,
    #[serde(default)]
    pub(crate) date: Option<String>,
    #[serde(default)]
    pub(crate) notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConversationAdapterPackageValidationResult {
    pub(crate) manifest_path: String,
    pub(crate) adapter_manifest_path: String,
    pub(crate) content_hash: String,
    pub(crate) manifest: ConversationAdapterPackageManifest,
    pub(crate) adapter_validation: ExternalAdapterValidationResult,
}

pub(crate) fn validate_conversation_adapter_package_dir(
    package_root: &Path,
) -> AppResult<ConversationAdapterPackageValidationResult> {
    if !package_root.is_dir() {
        return Err(format!(
            "conversation adapter package root is not a directory: {}",
            package_root.display()
        ));
    }
    let manifest_path = package_root.join(PACKAGE_MANIFEST_FILE);
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?;
    let manifest: ConversationAdapterPackageManifest =
        serde_json::from_str(&manifest_text).map_err(|error| error.to_string())?;
    validate_package_manifest_shape(&manifest)?;

    let adapter_manifest_relative =
        safe_package_relative_path("adapter manifest", &manifest.adapter_manifest)?;
    let adapter_manifest_path = package_root.join(adapter_manifest_relative);
    if !adapter_manifest_path.is_file() {
        return Err(format!(
            "conversation adapter package adapter manifest not found: {}",
            adapter_manifest_path.display()
        ));
    }
    let adapter_validation = super::external::validate_external_adapter_manifest(
        &adapter_manifest_path.to_string_lossy(),
    )?;
    let content_hash = hash_conversation_adapter_package_dir(package_root)?;
    Ok(ConversationAdapterPackageValidationResult {
        manifest_path: manifest_path.to_string_lossy().to_string(),
        adapter_manifest_path: adapter_manifest_path.to_string_lossy().to_string(),
        content_hash,
        manifest,
        adapter_validation,
    })
}

pub(crate) fn hash_conversation_adapter_package_dir(package_root: &Path) -> AppResult<String> {
    if !package_root.is_dir() {
        return Err(format!(
            "conversation adapter package root is not a directory: {}",
            package_root.display()
        ));
    }
    let mut file_paths = Vec::new();
    for entry in WalkDir::new(package_root).follow_links(false) {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path == package_root {
            continue;
        }
        let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "conversation adapter package must not contain symlinks: {}",
                path.display()
            ));
        }
        if metadata.is_file() {
            file_paths.push(path.to_path_buf());
        }
    }
    file_paths.sort_by(|left, right| {
        relative_package_path_text(package_root, left)
            .cmp(&relative_package_path_text(package_root, right))
    });

    let mut hasher = Sha256::new();
    for path in file_paths {
        let relative = relative_package_path_text(package_root, &path);
        hasher.update(relative.as_bytes());
        hasher.update(b"\0");
        let bytes = fs::read(&path).map_err(|error| error.to_string())?;
        hasher.update(bytes);
        hasher.update(b"\0");
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_package_manifest_shape(manifest: &ConversationAdapterPackageManifest) -> AppResult<()> {
    if manifest.schema_version != 1 {
        return Err("conversation adapter package schema_version must be 1".to_string());
    }
    validate_safe_id("conversation adapter package id", &manifest.package_id)?;
    if manifest.name.trim().is_empty() {
        return Err("conversation adapter package name is required".to_string());
    }
    if manifest.version.trim().is_empty() {
        return Err("conversation adapter package version is required".to_string());
    }
    validate_min_core_version(&manifest.min_core_version)?;
    safe_package_relative_path("adapter manifest", &manifest.adapter_manifest)?;
    if manifest.capabilities.is_empty() {
        return Err("conversation adapter package capabilities are required".to_string());
    }
    Ok(())
}

fn validate_safe_id(field: &str, value: &str) -> AppResult<()> {
    let valid = !value.trim().is_empty()
        && value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_')
        });
    if !valid {
        return Err(format!("{field} must be a safe path segment: {value}"));
    }
    Ok(())
}

fn validate_min_core_version(value: &str) -> AppResult<()> {
    let minimum = value.trim().trim_start_matches('v');
    if minimum.is_empty() {
        return Err("conversation adapter package min_core_version is required".to_string());
    }
    if compare_core_versions(SUPPORTED_CORE_VERSION, minimum).is_lt() {
        return Err(format!(
            "conversation adapter package requires AssetIWeave core >= {minimum}"
        ));
    }
    Ok(())
}

fn compare_core_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    for index in 0..left_parts.len().max(right_parts.len()) {
        let left_part = *left_parts.get(index).unwrap_or(&0);
        let right_part = *right_parts.get(index).unwrap_or(&0);
        match left_part.cmp(&right_part) {
            std::cmp::Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    std::cmp::Ordering::Equal
}

fn version_parts(value: &str) -> Vec<u64> {
    value
        .trim()
        .trim_start_matches('v')
        .split(['.', '-', '+'])
        .map(|part| {
            part.chars()
                .take_while(|character| character.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
                .unwrap_or(0)
        })
        .collect()
}

fn safe_package_relative_path(field: &str, raw: &str) -> AppResult<PathBuf> {
    let trimmed = raw.trim();
    let path = Path::new(trimmed);
    if trimmed.is_empty()
        || path.is_absolute()
        || looks_like_windows_rooted_path(trimmed)
        || trimmed.contains('\0')
        || trimmed
            .split(['/', '\\'])
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(format!(
            "conversation adapter package {field} must be a relative path inside the package"
        ));
    }
    Ok(PathBuf::from(trimmed))
}

fn looks_like_windows_rooted_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    if path.starts_with("\\\\") || path.starts_with('\\') {
        return true;
    }
    bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

fn relative_package_path_text(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use uuid::Uuid;

    fn temp_package_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-conversation-package-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create package root");
        root
    }

    fn write_valid_package(root: &Path) {
        fs::write(
            root.join(PACKAGE_MANIFEST_FILE),
            r#"{
  "schema_version": 1,
  "package_id": "codex-session",
  "name": "Codex Session Parser",
  "version": "1.0.0",
  "min_core_version": "0.1.0",
  "record_kind": "session",
  "adapter_manifest": "conversation-adapter.json",
  "capabilities": ["read_session"],
  "runtime": { "protocol": "stdio-ndjson-v1" },
  "changelog": []
}"#,
        )
        .expect("write package manifest");
        fs::write(
            root.join("conversation-adapter.json"),
            r#"{
  "schema_version": 1,
  "id": "codex",
  "name": "Codex",
  "version": "1.0.0",
  "protocol_version": 1,
  "runtime": { "type": "node", "entry": "adapter.mjs", "version": ">=20" },
  "capabilities": ["probe", "read_session"],
  "input_kinds": ["directory"]
}"#,
        )
        .expect("write adapter manifest");
        fs::write(root.join("adapter.mjs"), "process.exit(0);\n").expect("write adapter");
    }

    #[test]
    fn package_manifest_rejects_unsafe_adapter_manifest_paths() {
        let root = temp_package_root();
        write_valid_package(&root);
        for unsafe_path in [
            "../conversation-adapter.json",
            "/tmp/conversation-adapter.json",
            "nested/../conversation-adapter.json",
            "C:\\tmp\\conversation-adapter.json",
        ] {
            let mut text =
                fs::read_to_string(root.join(PACKAGE_MANIFEST_FILE)).expect("read manifest");
            text = text.replace(
                "\"adapter_manifest\": \"conversation-adapter.json\"",
                &format!("\"adapter_manifest\": \"{unsafe_path}\""),
            );
            fs::write(root.join(PACKAGE_MANIFEST_FILE), text).expect("write manifest");
            assert!(
                validate_conversation_adapter_package_dir(&root).is_err(),
                "expected unsafe path to fail: {unsafe_path}"
            );
            write_valid_package(&root);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn package_manifest_rejects_missing_adapter_manifest_and_newer_core() {
        let root = temp_package_root();
        write_valid_package(&root);
        fs::remove_file(root.join("conversation-adapter.json")).expect("remove adapter manifest");
        assert!(validate_conversation_adapter_package_dir(&root).is_err());

        write_valid_package(&root);
        let text = fs::read_to_string(root.join(PACKAGE_MANIFEST_FILE))
            .expect("read package manifest")
            .replace(
                "\"min_core_version\": \"0.1.0\"",
                "\"min_core_version\": \"99.0.0\"",
            );
        fs::write(root.join(PACKAGE_MANIFEST_FILE), text).expect("write package manifest");
        assert!(validate_conversation_adapter_package_dir(&root).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn package_hash_changes_when_auxiliary_file_changes() {
        let root = temp_package_root();
        write_valid_package(&root);
        let first = hash_conversation_adapter_package_dir(&root).expect("hash package");

        let mut helper = fs::File::create(root.join("helper.txt")).expect("create helper");
        helper.write_all(b"first").expect("write helper");
        let second = hash_conversation_adapter_package_dir(&root).expect("hash changed package");
        assert_ne!(first, second);

        fs::write(root.join("helper.txt"), "second").expect("mutate helper");
        let third = hash_conversation_adapter_package_dir(&root).expect("hash mutated package");
        assert_ne!(second, third);
        let _ = fs::remove_dir_all(root);
    }
}
