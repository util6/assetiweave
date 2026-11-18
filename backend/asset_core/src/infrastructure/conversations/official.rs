//! Read-only projection of conversation adapters that already exist in the
//! managed user directory.
//!
//! The host never ships adapter script copies inside the binary: an adapter is
//! only known to the application once a real package exists on disk. Bootstrap
//! therefore enumerates the managed directory, validates each manifest and
//! projects the result into `conversation_adapters`. Files are only ever read,
//! so user edits to `adapter.mjs` / `payload-policy.mjs` / manifests survive
//! every bootstrap, reconcile and sync pass byte for byte.

use crate::backend::infrastructure::app_settings::conversation_adapter_dir;
use crate::backend::{
    domain::{ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState},
    infrastructure::error::InfraResult,
};
use chrono::Utc;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Managed directory children that are infrastructure, not adapter packages.
const RESERVED_MANAGED_ENTRIES: &[&str] = &["packages", "runtime", "staging", "common"];

/// Manifest file that identifies an adapter package directory.
const ADAPTER_MANIFEST_FILE_NAME: &str = "conversation-adapter.json";

/// Enumerate the managed user directory and project every valid adapter
/// package that already exists there.
pub fn discover_managed_conversation_adapters() -> InfraResult<Vec<ConversationAdapter>> {
    discover_managed_conversation_adapters_at(&conversation_adapter_dir()?)
}

/// Read-only discovery over an explicit managed root.
///
/// A missing root is a legal state (fresh installation without any adapter),
/// and discovery must not create it.
pub fn discover_managed_conversation_adapters_at(
    root: &Path,
) -> InfraResult<Vec<ConversationAdapter>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut candidates: Vec<PathBuf> = fs::read_dir(root)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                return false;
            };
            !name.starts_with('.') && !RESERVED_MANAGED_ENTRIES.contains(&name)
        })
        .filter(|path| path.join(ADAPTER_MANIFEST_FILE_NAME).is_file())
        .collect();
    candidates.sort();

    let mut adapters = Vec::new();
    for adapter_dir in candidates {
        let manifest_path = adapter_dir.join(ADAPTER_MANIFEST_FILE_NAME);
        match super::external::validate_external_adapter_manifest(&manifest_path.to_string_lossy())
        {
            Ok(validation) => adapters.push(project_validated_adapter(validation)),
            Err(error) => tracing::warn!(
                target: "assetiweave.operation",
                operation = "app.bootstrap.conversation_adapter_discovery",
                manifest_path = %manifest_path.display(),
                error = %error,
                "managed conversation adapter was skipped"
            ),
        }
    }
    Ok(adapters)
}

/// Project a validated on-disk adapter as an *untrusted observation*.
///
/// #195：受管目录是用户可写的位置，磁盘上出现一份包不构成任何信任授权。
/// 这里只记录「当前磁盘内容是什么」（`content_hash`），`trusted_hash` 必须留空，
/// 信任只能由显式动作（工作区晋升 / 市场安装 / 本地注册）建立。
fn project_validated_adapter(
    validation: super::types::ExternalAdapterValidationResult,
) -> ConversationAdapter {
    let now = Utc::now().to_rfc3339();
    ConversationAdapter {
        id: validation.manifest.id.clone(),
        name: validation.manifest.name.clone(),
        kind: ConversationAdapterKind::External,
        version: validation.manifest.version.clone(),
        enabled: true,
        manifest_path: Some(validation.manifest_path.clone()),
        executable_path: Some(validation.executable_path.clone()),
        content_hash: Some(validation.content_hash.clone()),
        trusted_hash: None,
        trust_state: ConversationAdapterTrustState::Untrusted,
        protocol_version: Some(validation.manifest.protocol_version),
        capabilities: validation.manifest.capabilities.clone(),
        input_kinds: validation.manifest.input_kinds.clone(),
        card_contract_version: validation.manifest.card_contract_version,
        card_kinds: validation.manifest.card_kinds.clone(),
        projection_version: validation.manifest.projection_version,
        created_at: now.clone(),
        updated_at: now,
    }
}

#[cfg(test)]
#[path = "official_tests.rs"]
mod tests;
