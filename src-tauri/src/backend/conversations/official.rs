use crate::backend::{
    app_settings::conversation_adapter_dir,
    dto::AppResult,
    models::{ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState},
};
use chrono::Utc;
use std::{fs, path::Path};

struct OfficialAdapterAsset {
    manifest: &'static str,
    manifest_text: &'static str,
    script: &'static str,
}

const OFFICIAL_ADAPTERS: &[OfficialAdapterAsset] = &[
    OfficialAdapterAsset {
        manifest: "codex/conversation-adapter.json",
        manifest_text: include_str!(
            "../../../bundled/conversation-adapters/codex/conversation-adapter.json"
        ),
        script: include_str!("../../../bundled/conversation-adapters/codex/adapter.mjs"),
    },
    OfficialAdapterAsset {
        manifest: "claude-code/conversation-adapter.json",
        manifest_text: include_str!(
            "../../../bundled/conversation-adapters/claude-code/conversation-adapter.json"
        ),
        script: include_str!("../../../bundled/conversation-adapters/claude-code/adapter.mjs"),
    },
    OfficialAdapterAsset {
        manifest: "opencode/conversation-adapter.json",
        manifest_text: include_str!(
            "../../../bundled/conversation-adapters/opencode/conversation-adapter.json"
        ),
        script: include_str!("../../../bundled/conversation-adapters/opencode/adapter.mjs"),
    },
];

pub(crate) fn ensure_official_conversation_adapters() -> AppResult<Vec<ConversationAdapter>> {
    let root = conversation_adapter_dir()?;
    let mut adapters = Vec::new();
    for asset in OFFICIAL_ADAPTERS {
        let manifest_path = root.join(asset.manifest);
        let adapter_dir = manifest_path
            .parent()
            .ok_or_else(|| "official adapter manifest has no parent directory".to_string())?;
        fs::create_dir_all(adapter_dir).map_err(|error| error.to_string())?;
        write_if_changed(&manifest_path, asset.manifest_text.as_bytes())?;
        let script_path = adapter_dir.join("adapter.mjs");
        write_if_changed(&script_path, asset.script.as_bytes())?;
        make_executable(&script_path)?;

        let validation =
            super::external::validate_external_adapter_manifest(&manifest_path.to_string_lossy())?;
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
            created_at: now.clone(),
            updated_at: now,
        });
    }
    Ok(adapters)
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if fs::read(path).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    fs::write(path, bytes).map_err(|error| error.to_string())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> AppResult<()> {
    Ok(())
}
