use crate::backend::{
    app_settings::conversation_adapter_dir,
    models::{ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState},
    runtime::{AppError, AppResult},
};
use chrono::Utc;
use std::{fs, path::Path, sync::Mutex};

struct OfficialAdapterAsset {
    manifest: &'static str,
    manifest_text: &'static str,
    package_manifest_text: &'static str,
    script: &'static str,
    payload_policy_script: Option<&'static str>,
}

const SHELL_PROJECTOR_SCRIPT: &str =
    include_str!("../../../../builtin-assets/adapters/common/shell-projector.cjs");
const SHELL_PROJECTOR_ADAPTER_SCRIPT: &str =
    include_str!("../../../../builtin-assets/adapters/common/projector-adapter.cjs");
const SHELL_PROJECTOR_MANIFEST: &str =
    include_str!("../../../../builtin-assets/adapters/common/projector-manifest.json");
const SHELL_PROJECTOR_RUNTIME_VERSION: &str = "shell-projector-v1";
static OFFICIAL_ADAPTER_MATERIALIZE_LOCK: Mutex<()> = Mutex::new(());
static SHELL_PROJECTOR_MATERIALIZE_LOCK: Mutex<()> = Mutex::new(());

const OFFICIAL_ADAPTERS: &[OfficialAdapterAsset] = &[
    OfficialAdapterAsset {
        manifest: "codex/conversation-adapter.json",
        manifest_text: include_str!(
            "../../../../builtin-assets/adapters/codex/conversation-adapter.json"
        ),
        package_manifest_text: include_str!(
            "../../../../builtin-assets/adapters/codex/conversation-adapter-package.json"
        ),
        script: include_str!("../../../../builtin-assets/adapters/codex/adapter.mjs"),
        payload_policy_script: Some(include_str!(
            "../../../../builtin-assets/adapters/codex/payload-policy.mjs"
        )),
    },
    OfficialAdapterAsset {
        manifest: "claude-code/conversation-adapter.json",
        manifest_text: include_str!(
            "../../../../builtin-assets/adapters/claude-code/conversation-adapter.json"
        ),
        package_manifest_text: include_str!(
            "../../../../builtin-assets/adapters/claude-code/conversation-adapter-package.json"
        ),
        script: include_str!("../../../../builtin-assets/adapters/claude-code/adapter.mjs"),
        payload_policy_script: Some(include_str!(
            "../../../../builtin-assets/adapters/claude-code/payload-policy.mjs"
        )),
    },
    OfficialAdapterAsset {
        manifest: "opencode/conversation-adapter.json",
        manifest_text: include_str!(
            "../../../../builtin-assets/adapters/opencode/conversation-adapter.json"
        ),
        package_manifest_text: include_str!(
            "../../../../builtin-assets/adapters/opencode/conversation-adapter-package.json"
        ),
        script: include_str!("../../../../builtin-assets/adapters/opencode/adapter.mjs"),
        payload_policy_script: Some(include_str!(
            "../../../../builtin-assets/adapters/opencode/payload-policy.mjs"
        )),
    },
    OfficialAdapterAsset {
        manifest: "antigravity/conversation-adapter.json",
        manifest_text: include_str!(
            "../../../../builtin-assets/adapters/antigravity/conversation-adapter.json"
        ),
        package_manifest_text: include_str!(
            "../../../../builtin-assets/adapters/antigravity/conversation-adapter-package.json"
        ),
        script: include_str!("../../../../builtin-assets/adapters/antigravity/adapter.mjs"),
        payload_policy_script: Some(include_str!(
            "../../../../builtin-assets/adapters/antigravity/payload-policy.mjs"
        )),
    },
    OfficialAdapterAsset {
        manifest: "zcode/conversation-adapter.json",
        manifest_text: include_str!(
            "../../../../builtin-assets/adapters/zcode/conversation-adapter.json"
        ),
        package_manifest_text: include_str!(
            "../../../../builtin-assets/adapters/zcode/conversation-adapter-package.json"
        ),
        script: include_str!("../../../../builtin-assets/adapters/zcode/adapter.mjs"),
        payload_policy_script: None,
    },
];

fn should_refresh_official_adapter(manifest_path: &Path, bundled_manifest_text: &str) -> bool {
    let Ok(existing_text) = fs::read_to_string(manifest_path) else {
        return true;
    };
    let Ok(existing): Result<serde_json::Value, _> = serde_json::from_str(&existing_text) else {
        return true;
    };
    let Ok(bundled): Result<serde_json::Value, _> = serde_json::from_str(bundled_manifest_text)
    else {
        return true;
    };
    let existing_version = existing
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let bundled_version = bundled
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if existing_version != bundled_version {
        return true;
    }
    let existing_caps = existing.get("capabilities").and_then(|c| c.as_array());
    let bundled_caps = bundled.get("capabilities").and_then(|c| c.as_array());
    match (existing_caps, bundled_caps) {
        (Some(e), Some(b)) => b.iter().any(|req| !e.contains(req)),
        _ => false,
    }
}

pub(crate) fn is_official_adapter_id(adapter_id: &str) -> bool {
    OFFICIAL_ADAPTERS
        .iter()
        .any(|a| a.manifest.starts_with(&format!("{adapter_id}/")))
}

pub(crate) fn sync_official_adapter_to_package_dir(
    adapter_id: &str,
    target_dir: &Path,
) -> AppResult<bool> {
    let Some(asset) = OFFICIAL_ADAPTERS
        .iter()
        .find(|a| a.manifest.starts_with(&format!("{adapter_id}/")))
    else {
        return Ok(false);
    };
    let target_manifest = target_dir.join("conversation-adapter.json");
    if !should_refresh_official_adapter(&target_manifest, asset.manifest_text) {
        return Ok(false);
    }
    fs::create_dir_all(target_dir)?;
    write_managed_runtime_file(&target_manifest, asset.manifest_text.as_bytes())?;
    write_managed_runtime_file(
        &target_dir.join("conversation-adapter-package.json"),
        asset.package_manifest_text.as_bytes(),
    )?;
    let script_path = target_dir.join("adapter.mjs");
    write_managed_runtime_file(&script_path, asset.script.as_bytes())?;
    if let Some(policy_script) = asset.payload_policy_script {
        write_managed_runtime_file(
            &target_dir.join("payload-policy.mjs"),
            policy_script.as_bytes(),
        )?;
    }
    write_managed_runtime_file(
        &target_dir.join("shell-projector.cjs"),
        SHELL_PROJECTOR_SCRIPT.as_bytes(),
    )?;
    make_executable(&script_path)?;
    Ok(true)
}

pub(crate) fn ensure_official_conversation_adapters() -> AppResult<Vec<ConversationAdapter>> {
    let _guard = OFFICIAL_ADAPTER_MATERIALIZE_LOCK
        .lock()
        .map_err(|_| AppError::external("official adapter materialization lock poisoned"))?;
    let root = conversation_adapter_dir()?;
    let mut adapters = Vec::new();
    for asset in OFFICIAL_ADAPTERS {
        let manifest_path = root.join(asset.manifest);
        let adapter_dir = manifest_path.parent().ok_or_else(|| {
            AppError::Validation("official adapter manifest has no parent directory".to_string())
        })?;
        fs::create_dir_all(adapter_dir)?;
        let refresh = should_refresh_official_adapter(&manifest_path, asset.manifest_text);
        let write_fn = if refresh {
            write_managed_runtime_file
        } else {
            write_if_missing
        };
        write_fn(&manifest_path, asset.manifest_text.as_bytes())?;
        let package_manifest_path = adapter_dir.join("conversation-adapter-package.json");
        write_fn(
            &package_manifest_path,
            asset.package_manifest_text.as_bytes(),
        )?;
        let script_path = adapter_dir.join("adapter.mjs");
        write_fn(&script_path, asset.script.as_bytes())?;
        if let Some(payload_script) = asset.payload_policy_script {
            let payload_policy_path = adapter_dir.join("payload-policy.mjs");
            write_fn(&payload_policy_path, payload_script.as_bytes())?;
        }
        let shell_projector_path = adapter_dir.join("shell-projector.cjs");
        write_fn(&shell_projector_path, SHELL_PROJECTOR_SCRIPT.as_bytes())?;
        make_executable(&script_path)?;

        let Ok(validation) =
            super::external::validate_external_adapter_manifest(&manifest_path.to_string_lossy())
        else {
            continue;
        };
        let now = Utc::now().to_rfc3339();
        adapters.push(ConversationAdapter {
            id: validation.manifest.id.clone(),
            name: validation.manifest.name.clone(),
            kind: ConversationAdapterKind::External,
            version: validation.manifest.version.clone(),
            enabled: true,
            manifest_path: Some(validation.manifest_path.clone()),
            executable_path: Some(validation.executable_path.clone()),
            content_hash: Some(validation.content_hash.clone()),
            trusted_hash: Some(validation.content_hash.clone()),
            trust_state: ConversationAdapterTrustState::BuiltIn,
            protocol_version: Some(validation.manifest.protocol_version),
            capabilities: validation.manifest.capabilities.clone(),
            input_kinds: validation.manifest.input_kinds.clone(),
            card_contract_version: validation.manifest.card_contract_version,
            card_kinds: validation.manifest.card_kinds.clone(),
            created_at: now.clone(),
            updated_at: now,
        });
    }
    Ok(adapters)
}

pub(crate) fn ensure_shell_command_projector() -> AppResult<ConversationAdapter> {
    let _guard = SHELL_PROJECTOR_MATERIALIZE_LOCK
        .lock()
        .map_err(|_| AppError::external("shell command projector materialization lock poisoned"))?;
    let root = conversation_adapter_dir()?
        .join("runtime")
        .join(SHELL_PROJECTOR_RUNTIME_VERSION);
    materialize_shell_command_projector(&root)
}

fn materialize_shell_command_projector(root: &Path) -> AppResult<ConversationAdapter> {
    fs::create_dir_all(root)?;
    let manifest_path = root.join("conversation-adapter.json");
    let adapter_path = root.join("projector-adapter.cjs");
    let projector_path = root.join("shell-projector.cjs");
    write_managed_runtime_file(&manifest_path, SHELL_PROJECTOR_MANIFEST.as_bytes())?;
    write_managed_runtime_file(&adapter_path, SHELL_PROJECTOR_ADAPTER_SCRIPT.as_bytes())?;
    write_managed_runtime_file(&projector_path, SHELL_PROJECTOR_SCRIPT.as_bytes())?;
    make_executable(&adapter_path)?;

    let validation =
        super::external::validate_external_adapter_manifest(&manifest_path.to_string_lossy())?;
    let now = Utc::now().to_rfc3339();
    Ok(ConversationAdapter {
        id: validation.manifest.id.clone(),
        name: validation.manifest.name.clone(),
        kind: ConversationAdapterKind::External,
        version: validation.manifest.version.clone(),
        enabled: true,
        manifest_path: Some(validation.manifest_path.clone()),
        executable_path: Some(validation.executable_path.clone()),
        content_hash: Some(validation.content_hash.clone()),
        trusted_hash: Some(validation.content_hash.clone()),
        trust_state: ConversationAdapterTrustState::BuiltIn,
        protocol_version: Some(validation.manifest.protocol_version),
        capabilities: validation.manifest.capabilities.clone(),
        input_kinds: validation.manifest.input_kinds.clone(),
        card_contract_version: validation.manifest.card_contract_version,
        card_kinds: validation.manifest.card_kinds.clone(),
        created_at: now.clone(),
        updated_at: now,
    })
}

fn write_managed_runtime_file(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    fs::write(path, bytes).map_err(AppError::from)
}

fn write_if_missing(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if path.exists() {
        return Ok(());
    }
    fs::write(path, bytes).map_err(AppError::from)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(AppError::from)
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> AppResult<()> {
    Ok(())
}

#[cfg(test)]
#[path = "official_tests.rs"]
mod tests;
