//! #200：安装/更新在未指定版本时优先晋升受管目录里的本地工作区。
//!
//! 只负责“找候选 + 调晋升”，权威校验/快照/激活仍在
//! [`promote_conversation_adapter_workspace_package`] 里完成。
//! 命中本地候选时绝不发起任何远端请求；显式指定版本仍由调用方走发布安装。

use super::*;
use crate::backend::application::prelude::*;
use crate::backend::domain::ConversationAdapterPackageChangeAction;

/// 若受管用户目录里存在与该 `package_id` 匹配且构成待晋升变更的工作区，
/// 直接执行（或 dry-run 预览）本地晋升并返回结果；否则返回 `None`，
/// 由调用方继续走远端目录。
pub async fn promote_pending_local_workspace(
    service: &AppService,
    package_id: &str,
    action: ConversationAdapterPackageChangeAction,
    dry_run: bool,
) -> AppResult<Option<Value>> {
    let package_id = package_id.trim();
    if package_id.is_empty() {
        return Ok(None);
    }
    let managed_root = crate::backend::infrastructure::app_settings::conversation_adapter_dir()?;
    let Some(candidate) =
        find_pending_local_workspace(service, &managed_root, package_id, action).await?
    else {
        return Ok(None);
    };
    let source_dir = PathBuf::from(candidate.source.url.as_str());
    let mut result = promote_conversation_adapter_workspace_package(
        service,
        &source_dir,
        &managed_root,
        dry_run,
    )
    .await?;
    if let Some(object) = result.as_object_mut() {
        object.insert("source".to_string(), json!("local_workspace"));
        object.insert(
            "promoted_from".to_string(),
            json!(source_dir.to_string_lossy()),
        );
    }
    Ok(Some(result))
}

async fn find_pending_local_workspace(
    service: &AppService,
    managed_root: &Path,
    package_id: &str,
    action: ConversationAdapterPackageChangeAction,
) -> AppResult<Option<ConversationScriptCatalogItem>> {
    if !managed_root.is_dir() {
        return Ok(None);
    }
    let adapters = service.list_conversation_adapters()?;
    let package = service
        .load_conversation_adapter_package(package_id)
        .await?;
    for candidate in discover_local_conversation_adapter_packages(managed_root)? {
        if candidate.package_id() != package_id {
            continue;
        }
        let adapter_id = candidate.adapter_id.as_deref().unwrap_or_default();
        if adapter_id.is_empty() {
            continue;
        }
        // #196 之后不再有“官方适配器物化目录”：受管目录里的每个适配器包
        // 都是同等的可晋升工作区。已安装且内容未变的包由下方 hash 比较判为
        // 非待晋升，无需额外的官方身份短路。
        let adapter = adapters.iter().find(|adapter| adapter.id == adapter_id);
        let pending = match package.as_ref() {
            Some(package) => local_candidate_is_pending_update(&candidate, Some(package), adapter),
            // 首次安装：受管目录里的同名工作区就是用户要装的本地包。
            None => action == ConversationAdapterPackageChangeAction::Install,
        };
        if pending {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}
