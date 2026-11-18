//! #198：受管目录候选包与数据库已受信任记录的更新判定。
//!
//! 只做“是否存在待晋升变更”的判定，不做晋升执行（晋升见 #200），
//! 也不改写任何受信任 hash —— 篡改已激活快照必须继续落到 `hash_mismatch`。

use super::*;
use crate::backend::application::prelude::*;

/// 本地目录候选是否代表一次待晋升的变更：候选 SemVer 更高，
/// 或版本相同但磁盘现算 hash 与已受信任 hash 不一致。
pub fn local_candidate_is_pending_update(
    candidate: &ConversationScriptCatalogItem,
    package: Option<&ConversationAdapterPackage>,
    adapter: Option<&ConversationAdapter>,
) -> bool {
    if candidate.source.kind != ConversationScriptCatalogSourceKind::LocalDirectory {
        return false;
    }
    let Some(installed_version) = package
        .map(|package| package.version.as_str())
        .or_else(|| adapter.map(|adapter| adapter.version.as_str()))
    else {
        return false;
    };
    if let (Ok(candidate_version), Ok(installed_version)) = (
        semver::Version::parse(candidate.version.trim()),
        semver::Version::parse(installed_version.trim()),
    ) {
        if candidate_version > installed_version {
            return true;
        }
        if candidate_version < installed_version {
            return false;
        }
    }
    let trusted_package_hash = package.and_then(|package| {
        package
            .trusted_package_hash
            .as_deref()
            .or(package.installed_content_hash.as_deref())
    });
    if hash_differs(
        candidate.expected_package_hash.as_deref(),
        trusted_package_hash,
    ) {
        return true;
    }
    let trusted_content_hash = adapter.and_then(|adapter| {
        adapter
            .trusted_hash
            .as_deref()
            .or(adapter.content_hash.as_deref())
    });
    hash_differs(
        candidate.expected_content_hash.as_deref(),
        trusted_content_hash,
    )
}

fn hash_differs(candidate: Option<&str>, trusted: Option<&str>) -> bool {
    matches!((candidate, trusted), (Some(candidate), Some(trusted)) if candidate != trusted)
}
