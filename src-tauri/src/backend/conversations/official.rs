use crate::backend::{
    app_settings::conversation_adapter_dir,
    compat::LegacyResult,
    models::{ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState},
};
use chrono::Utc;
use std::{fs, path::Path};

struct OfficialAdapterAsset {
    manifest: &'static str,
    manifest_text: &'static str,
    package_manifest_text: &'static str,
    script: &'static str,
    payload_policy_script: &'static str,
}

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
        payload_policy_script: include_str!(
            "../../../../builtin-assets/adapters/codex/payload-policy.mjs"
        ),
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
        payload_policy_script: include_str!(
            "../../../../builtin-assets/adapters/claude-code/payload-policy.mjs"
        ),
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
        payload_policy_script: include_str!(
            "../../../../builtin-assets/adapters/opencode/payload-policy.mjs"
        ),
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
        payload_policy_script: include_str!(
            "../../../../builtin-assets/adapters/antigravity/payload-policy.mjs"
        ),
    },
];

pub(crate) fn ensure_official_conversation_adapters() -> LegacyResult<Vec<ConversationAdapter>> {
    let root = conversation_adapter_dir()?;
    let mut adapters = Vec::new();
    for asset in OFFICIAL_ADAPTERS {
        let manifest_path = root.join(asset.manifest);
        let adapter_dir = manifest_path
            .parent()
            .ok_or_else(|| "official adapter manifest has no parent directory".to_string())?;
        fs::create_dir_all(adapter_dir).map_err(|error| error.to_string())?;
        write_if_missing(&manifest_path, asset.manifest_text.as_bytes())?;
        let package_manifest_path = adapter_dir.join("conversation-adapter-package.json");
        write_if_missing(
            &package_manifest_path,
            asset.package_manifest_text.as_bytes(),
        )?;
        let script_path = adapter_dir.join("adapter.mjs");
        write_if_missing(&script_path, asset.script.as_bytes())?;
        let payload_policy_path = adapter_dir.join("payload-policy.mjs");
        write_if_missing(&payload_policy_path, asset.payload_policy_script.as_bytes())?;
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

fn write_if_missing(path: &Path, bytes: &[u8]) -> LegacyResult<()> {
    if path.exists() {
        return Ok(());
    }
    fs::write(path, bytes).map_err(|error| error.to_string())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> LegacyResult<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> LegacyResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_preserves_existing_editable_workspace_files() {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-official-workspace-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create workspace");
        let path = root.join("adapter.mjs");
        fs::write(&path, "user revision\n").expect("write user revision");

        write_if_missing(&path, b"bundled revision\n").expect("seed file");

        assert_eq!(
            fs::read_to_string(&path).expect("read workspace file"),
            "user revision\n"
        );
        let _ = fs::remove_dir_all(root);
    }
}
