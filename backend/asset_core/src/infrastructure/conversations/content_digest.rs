//! #204：适配器「综合内容 Hash」。
//!
//! 同步水位判定的身份令牌不能只看适配器包自身（清单 + 主脚本 + 包内脚本副本）：
//! 公共策略脚本（`common/**`）才是解析时真正生效的策略。因此综合 Hash 由三部分组成：
//!
//! 1. 适配器记录里的 `content_hash`（清单指纹 + 主脚本指纹 + 包内
//!    `payload-policy.mjs` 副本）；
//! 2. `$ASSETIWEAVE_HOME/conversation-adapters/common/` 下公共脚本的**实时内容**；
//! 3. 仅当 `common/` 缺失对应文件时，回落到包内随包副本。
//!
//! #195/#204：`common/` 是权威解析位置。此前「包内副本优先」会让晋升时冻结的
//! 副本遮蔽用户的实时修改：改 `common/payload-policy.mjs` 综合 Hash 不变，
//! 既不重解析也不生效。现在任何 `common/` 改动都会改变综合 Hash。
//!
//! 任一组成变化都会改变综合 Hash，从而由同步水位驱动存量会话重解析，
//! 不再需要修改编译期常量。

use super::io_utils::{hash_bytes, hash_file};
use super::official_runtime::{common_runtime_dir, COMMON_RUNTIME_FILES};
use crate::backend::domain::ConversationAdapter;
use crate::backend::infrastructure::error::InfraResult;
use std::path::{Path, PathBuf};

/// 计算适配器在本次同步中实际生效的综合内容 Hash。
///
/// 适配器没有可用的基座 `content_hash` 时返回 `None`，保持历史行为
/// （水位不参与 Hash 判定，只依赖会话指纹）。
pub fn adapter_runtime_content_hash(adapter: &ConversationAdapter) -> InfraResult<Option<String>> {
    let Some(base) = adapter
        .content_hash
        .as_deref()
        .map(str::trim)
        .filter(|hash| !hash.is_empty())
    else {
        return Ok(None);
    };
    let adapter_dir = adapter_script_dir(adapter);
    let common_dir = common_runtime_dir().ok();
    Ok(Some(compose_runtime_hash(
        base,
        adapter_dir.as_deref(),
        common_dir.as_deref(),
    )?))
}

/// 适配器包目录：优先清单所在目录，其次主脚本所在目录。
fn adapter_script_dir(adapter: &ConversationAdapter) -> Option<PathBuf> {
    for candidate in [&adapter.manifest_path, &adapter.executable_path] {
        let Some(path) = candidate.as_deref().map(Path::new) else {
            continue;
        };
        if let Some(parent) = path.parent() {
            return Some(parent.to_path_buf());
        }
    }
    None
}

fn compose_runtime_hash(
    base: &str,
    adapter_dir: Option<&Path>,
    common_dir: Option<&Path>,
) -> InfraResult<String> {
    let mut composite = format!("adapter:{base}");
    for file_name in COMMON_RUNTIME_FILES {
        let digest = effective_script_digest(adapter_dir, common_dir, file_name)?;
        composite.push('\n');
        composite.push_str(file_name);
        composite.push(':');
        composite.push_str(&digest);
    }
    Ok(hash_bytes(composite.as_bytes()))
}

/// Single public-script fingerprint: `common/` real-time content is authoritative.
///
/// #195/#204：`common/` 是运行时真正解析的位置，所以只要它存在同名文件就必须
/// 以它为准；包内副本只在 `common/` 缺失时兜底。否则晋升时冻结的副本会遮蔽
/// 用户对 `common/payload-policy.mjs` 的修改，使「改 common 即生效」失效。
fn effective_script_digest(
    adapter_dir: Option<&Path>,
    common_dir: Option<&Path>,
    file_name: &str,
) -> InfraResult<String> {
    for root in [common_dir, adapter_dir].into_iter().flatten() {
        let path = root.join(file_name);
        if path.is_file() {
            return hash_file(&path);
        }
    }
    Ok("-".to_string())
}
