use crate::backend::application::prelude::AppResult;
use crate::backend::domain::{
    ConversationAdapter, ConversationAdapterPackage, ConversationAdapterPackageVersion,
};
use sqlx::SqlitePool;

pub async fn list_adapters(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<ConversationAdapter>> {
    crate::backend::store::list_conversation_adapters_sqlx(pool, tenant_id)
        .await?
        .into_iter()
        .map(normalize_adapter)
        .collect()
}

pub async fn load_adapter(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: &str,
) -> AppResult<Option<ConversationAdapter>> {
    crate::backend::store::load_conversation_adapter_sqlx(pool, tenant_id, adapter_id)
        .await?
        .map(normalize_adapter)
        .transpose()
}

pub async fn save_adapter(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter: &ConversationAdapter,
) -> AppResult<()> {
    let adapter = normalize_adapter(adapter.clone())?;
    Ok(crate::backend::store::upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter).await?)
}

pub async fn list_packages(pool: &SqlitePool) -> AppResult<Vec<ConversationAdapterPackage>> {
    crate::backend::store::list_conversation_adapter_packages_sqlx(pool)
        .await?
        .into_iter()
        .map(normalize_package)
        .collect()
}

pub async fn load_package(
    pool: &SqlitePool,
    package_id: &str,
) -> AppResult<Option<ConversationAdapterPackage>> {
    crate::backend::store::load_conversation_adapter_package_sqlx(pool, package_id)
        .await?
        .map(normalize_package)
        .transpose()
}

pub async fn load_package_by_adapter(
    pool: &SqlitePool,
    adapter_id: &str,
) -> AppResult<Option<ConversationAdapterPackage>> {
    crate::backend::store::load_conversation_adapter_package_by_adapter_sqlx(pool, adapter_id)
        .await?
        .map(normalize_package)
        .transpose()
}

pub async fn save_package(
    pool: &SqlitePool,
    package: &ConversationAdapterPackage,
) -> AppResult<()> {
    let package = normalize_package(package.clone())?;
    Ok(crate::backend::store::upsert_conversation_adapter_package_sqlx(pool, &package).await?)
}

pub async fn list_package_versions(
    pool: &SqlitePool,
    package_id: &str,
) -> AppResult<Vec<ConversationAdapterPackageVersion>> {
    crate::backend::store::list_conversation_adapter_package_versions_sqlx(pool, package_id)
        .await?
        .into_iter()
        .map(normalize_version)
        .collect()
}

pub async fn activate_package(
    pool: &SqlitePool,
    adapter: &ConversationAdapter,
    package: &ConversationAdapterPackage,
    version: &ConversationAdapterPackageVersion,
) -> AppResult<()> {
    let adapter = normalize_adapter(adapter.clone())?;
    let package = normalize_package(package.clone())?;
    let version = normalize_version(version.clone())?;
    crate::backend::store::activate_conversation_adapter_package_sqlx(
        pool, &adapter, &package, &version,
    )
    .await?;
    seed_builtin_sources_for_all_tenants(pool).await;
    Ok(())
}

pub async fn activate_workspace(
    pool: &SqlitePool,
    adapter: &ConversationAdapter,
    package: &ConversationAdapterPackage,
) -> AppResult<()> {
    let adapter = normalize_adapter(adapter.clone())?;
    let package = normalize_package(package.clone())?;
    crate::backend::store::activate_conversation_adapter_workspace_sqlx(pool, &adapter, &package)
        .await?;
    seed_builtin_sources_for_all_tenants(pool).await;
    Ok(())
}

/// 给刚刚激活的适配器补齐对应的内置对话来源。
///
/// 激活会把适配器投影到所有租户，所以这里逐租户补齐。补齐只插入缺失行：
/// 已存在的来源（含被用户停用的、被改过配置的）不会被重写；适配器仍未
/// 存在的来源也不会落库。
///
/// 失败只记 warn：适配器已经装好了，缺一个默认来源不应该让激活整体失败。
async fn seed_builtin_sources_for_all_tenants(pool: &SqlitePool) {
    match crate::backend::store::load_all_tenant_ids_sqlx(pool).await {
        Ok(tenant_ids) => {
            for tenant_id in tenant_ids {
                seed_builtin_sources_for_tenant(pool, &tenant_id).await;
            }
        }
        Err(error) => tracing::warn!(
            target: "assetiweave.operation",
            operation = "app.conversation_adapter.builtin_source_seed",
            error = %error,
            "builtin conversation source backfill was skipped"
        ),
    }
}

/// 单租户版本的补齐入口，供只操作当前租户的注册路径复用。
pub(super) async fn seed_builtin_sources_for_tenant(pool: &SqlitePool, tenant_id: &str) {
    if let Err(error) =
        crate::backend::store::seed_builtin_conversation_sources_sqlx(pool, tenant_id).await
    {
        tracing::warn!(
            target: "assetiweave.operation",
            operation = "app.conversation_adapter.builtin_source_seed",
            tenant_id = %tenant_id,
            error = %error,
            "builtin conversation source backfill was skipped"
        );
    }
}

pub async fn deactivate_package(
    pool: &SqlitePool,
    package_id: &str,
    adapter_id: &str,
) -> AppResult<ConversationAdapterPackage> {
    normalize_package(
        crate::backend::store::deactivate_conversation_adapter_package_sqlx(
            pool, package_id, adapter_id,
        )
        .await?,
    )
}

fn normalize_adapter(mut adapter: ConversationAdapter) -> AppResult<ConversationAdapter> {
    crate::backend::infrastructure::path_utils::normalize_conversation_adapter_paths(&mut adapter)?;
    Ok(adapter)
}

fn normalize_package(
    mut package: ConversationAdapterPackage,
) -> AppResult<ConversationAdapterPackage> {
    crate::backend::infrastructure::path_utils::normalize_conversation_adapter_package_paths(
        &mut package,
    )?;
    Ok(package)
}

fn normalize_version(
    mut version: ConversationAdapterPackageVersion,
) -> AppResult<ConversationAdapterPackageVersion> {
    crate::backend::infrastructure::path_utils::normalize_conversation_adapter_version_paths(
        &mut version,
    )?;
    Ok(version)
}

#[cfg(test)]
#[path = "conversation_storage_tests.rs"]
mod tests;
