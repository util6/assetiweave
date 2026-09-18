use super::prelude::*;
use crate::backend::{
    builtin_skills::system_skill_root,
    models::{AssetKind, MemorySkillBinding},
    runtime::{AppError, AppResult},
};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub const MEMORY_GENERATION_SKILL_ID: &str = "assetiweave.memory-generation";
pub const MAX_SKILL_FILE_SIZE_BYTES: u64 = 131_072; // 128 KB

impl AppService {
    pub(crate) async fn load_active_generation_skill_text(&self) -> AppResult<String> {
        let settings = self.app_settings_value();
        let custom_skill_id = settings
            .get("memory")
            .and_then(|memory| memory.get("generationSkillAssetId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if let Some(asset_id) = custom_skill_id {
            let asset = self.validate_generation_skill_asset(asset_id).await?;
            return fs::read_to_string(PathBuf::from(asset.absolute_path).join("SKILL.md"))
                .map_err(AppError::external);
        }

        let template_root = system_skill_root()?.join("assetiweave-memory-generation");
        let entry_path = template_root.join("SKILL.md");
        if entry_path.exists() {
            return fs::read_to_string(entry_path).map_err(AppError::external);
        }
        String::from_utf8(
            include_bytes!(
                "../../../../builtin-assets/skills/assetiweave-memory-generation/SKILL.md"
            )
            .to_vec(),
        )
        .map_err(AppError::external)
    }

    /// 校验普通 Skill 是否满足 Memory Generation Skill 准入约束 (M35-SKILL-05/06)
    pub(crate) async fn validate_generation_skill_asset(&self, asset_id: &str) -> AppResult<Asset> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let assets = crate::backend::store::load_assets_sqlx(pool, tenant_id, None).await?;
        let asset = assets
            .into_iter()
            .find(|a| a.id == asset_id)
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "MEMORY_SKILL_NOT_FOUND: asset not found: {asset_id}"
                ))
            })?;

        if asset.kind != AssetKind::Skill {
            return Err(AppError::Validation(format!(
                "MEMORY_SKILL_WRONG_TYPE: asset {asset_id} is not a skill"
            )));
        }

        let sources = crate::backend::store::load_sources_sqlx(pool, tenant_id).await?;
        let source = sources
            .into_iter()
            .find(|s| s.id == asset.source_id)
            .ok_or_else(|| {
                AppError::NotFound(format!("source not found for asset: {}", asset.source_id))
            })?;

        // 来源信任检查：拒绝未信任来源
        if !source.enabled {
            return Err(AppError::Validation(format!(
                "MEMORY_SKILL_UNTRUSTED: source {} is disabled or untrusted",
                source.id
            )));
        }

        let skill_dir = PathBuf::from(&asset.absolute_path);
        let entry_path = skill_dir.join("SKILL.md");

        if !entry_path.exists() || !entry_path.is_file() {
            return Err(AppError::Validation(format!(
                "MEMORY_SKILL_ENTRY_UNREADABLE: entry SKILL.md does not exist at {}",
                entry_path.display()
            )));
        }

        let metadata = fs::metadata(&entry_path).map_err(|e| {
            AppError::Validation(format!(
                "MEMORY_SKILL_ENTRY_UNREADABLE: cannot read metadata of SKILL.md: {e}"
            ))
        })?;

        if metadata.len() > MAX_SKILL_FILE_SIZE_BYTES {
            return Err(AppError::Validation(format!(
                "MEMORY_SKILL_TOO_LARGE: SKILL.md size {} exceeds budget {}",
                metadata.len(),
                MAX_SKILL_FILE_SIZE_BYTES
            )));
        }

        let content = fs::read_to_string(&entry_path).map_err(|e| {
            AppError::Validation(format!(
                "MEMORY_SKILL_ENTRY_UNREADABLE: cannot read SKILL.md content: {e}"
            ))
        })?;

        // 验证 frontmatter
        if !content.starts_with("---") {
            return Err(AppError::Validation(
                "MEMORY_SKILL_INVALID_FRONTMATTER: missing opening frontmatter delimiter"
                    .to_string(),
            ));
        }

        let rest = &content[3..];
        let Some(end_idx) = rest.find("\n---") else {
            return Err(AppError::Validation(
                "MEMORY_SKILL_INVALID_FRONTMATTER: missing closing frontmatter delimiter"
                    .to_string(),
            ));
        };

        let frontmatter = &rest[..end_idx];
        if !frontmatter.contains("name:") || !frontmatter.contains("description:") {
            return Err(AppError::Validation(
                "MEMORY_SKILL_INVALID_FRONTMATTER: frontmatter must contain name and description"
                    .to_string(),
            ));
        }

        Ok(asset)
    }

    /// 解析当前有效的 Generation Skill 绑定 (M35-SKILL-02/03/06)
    pub(crate) async fn get_active_generation_skill_binding(
        &self,
    ) -> AppResult<MemorySkillBinding> {
        let settings = self.app_settings_value();
        let custom_skill_id = settings
            .get("memory")
            .and_then(|m| m.get("generationSkillAssetId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());

        if let Some(asset_id) = custom_skill_id {
            // 用户配置了自定义 Skill，必须严格校验；失败时不静默回退
            let asset = self.validate_generation_skill_asset(asset_id).await?;
            let skill_dir = PathBuf::from(&asset.absolute_path);
            let entry_path = skill_dir.join("SKILL.md");

            let entry_bytes = fs::read(&entry_path).map_err(AppError::external)?;
            let entry_hash = format!("{:x}", Sha256::digest(&entry_bytes));

            // 计算目录所有文件散列
            let mut hasher = Sha256::new();
            hasher.update(&entry_bytes);
            if let Ok(manifest_bytes) = fs::read(skill_dir.join("assetiweave.skill.json")) {
                hasher.update(&manifest_bytes);
            }
            let content_hash = format!("{:x}", hasher.finalize());

            Ok(MemorySkillBinding {
                asset_id: asset.id,
                asset_revision: 1,
                content_hash,
                entry_hash,
            })
        } else {
            // 使用系统内置模板
            let template_root = system_skill_root()?.join("assetiweave-memory-generation");
            let entry_path = template_root.join("SKILL.md");

            let (entry_bytes, manifest_bytes) = if entry_path.exists() {
                let eb = fs::read(&entry_path).map_err(AppError::external)?;
                let mb = fs::read(template_root.join("assetiweave.skill.json")).unwrap_or_default();
                (eb, mb)
            } else {
                let eb = include_bytes!(
                    "../../../../builtin-assets/skills/assetiweave-memory-generation/SKILL.md"
                )
                .to_vec();
                let mb = include_bytes!(
                    "../../../../builtin-assets/skills/assetiweave-memory-generation/assetiweave.skill.json"
                )
                .to_vec();
                (eb, mb)
            };

            let entry_hash = format!("{:x}", Sha256::digest(&entry_bytes));
            let mut hasher = Sha256::new();
            hasher.update(&entry_bytes);
            hasher.update(&manifest_bytes);
            let content_hash = format!("{:x}", hasher.finalize());

            Ok(MemorySkillBinding {
                asset_id: MEMORY_GENERATION_SKILL_ID.to_string(),
                asset_revision: 1,
                content_hash,
                entry_hash,
            })
        }
    }

    /// 复制内置模板到用户 Skill Library，成为普通可编辑副本 (M35-SKILL-02)
    pub(crate) async fn duplicate_generation_skill_to_library(&self) -> AppResult<CatalogAsset> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let library_root = capabilities::skill_backup_root_sqlx(pool, tenant_id).await?;
        let unique_suffix = uuid::Uuid::new_v4().simple().to_string();
        let target_dir_name = format!("assetiweave-memory-generation-{}", &unique_suffix[..8]);
        let target_dir = library_root.join(&target_dir_name);

        fs::create_dir_all(&target_dir).map_err(AppError::external)?;

        let template_dir = system_skill_root()?.join("assetiweave-memory-generation");
        let skill_md_content = if template_dir.join("SKILL.md").exists() {
            fs::read_to_string(template_dir.join("SKILL.md")).map_err(AppError::external)?
        } else {
            String::from_utf8_lossy(include_bytes!(
                "../../../../builtin-assets/skills/assetiweave-memory-generation/SKILL.md"
            ))
            .to_string()
        };

        let skill_json_content = if template_dir.join("assetiweave.skill.json").exists() {
            fs::read_to_string(template_dir.join("assetiweave.skill.json"))
                .map_err(AppError::external)?
        } else {
            String::from_utf8_lossy(include_bytes!(
                "../../../../builtin-assets/skills/assetiweave-memory-generation/assetiweave.skill.json"
            ))
            .to_string()
        };

        // 写入目标目录
        fs::write(target_dir.join("SKILL.md"), &skill_md_content).map_err(AppError::external)?;

        let mut manifest_val: Value =
            serde_json::from_str(&skill_json_content).unwrap_or(serde_json::json!({}));
        let new_skill_id = format!("user.memory-generation.{}", &unique_suffix[..8]);
        manifest_val["id"] = Value::String(new_skill_id.clone());
        manifest_val["derived_from_asset_id"] =
            Value::String(MEMORY_GENERATION_SKILL_ID.to_string());
        fs::write(
            target_dir.join("assetiweave.skill.json"),
            serde_json::to_string_pretty(&manifest_val).map_err(AppError::external)?,
        )
        .map_err(AppError::external)?;

        // 重新扫描来源
        capabilities::refresh_all_sources(pool, tenant_id).await?;

        // 在资产目录中查找新入库的 Asset
        let assets = crate::backend::store::load_assets_sqlx(pool, tenant_id, None).await?;
        let new_asset = assets
            .into_iter()
            .find(|a| {
                a.kind == AssetKind::Skill
                    && a.absolute_path == target_dir.to_string_lossy().to_string()
            })
            .ok_or_else(|| {
                AppError::NotFound(
                    "duplicated skill asset was created but not found in catalog".to_string(),
                )
            })?;

        // 自动更新设置中的 generationSkillAssetId
        let mut current_settings = self.app_settings_value();
        if let Some(mem) = current_settings
            .get_mut("memory")
            .and_then(Value::as_object_mut)
        {
            mem.insert(
                "generationSkillAssetId".to_string(),
                Value::String(new_asset.id.clone()),
            );
        } else {
            let mut mem_obj = serde_json::Map::new();
            mem_obj.insert(
                "generationSkillAssetId".to_string(),
                Value::String(new_asset.id.clone()),
            );
            current_settings["memory"] = Value::Object(mem_obj);
        }
        self.save_app_settings(current_settings).await?;

        // 转换为 CatalogAsset
        let catalog_assets =
            capabilities::catalog_assets_sqlx(pool, tenant_id, Some(AssetKind::Skill)).await?;
        catalog_assets
            .into_iter()
            .find(|ca| ca.asset.id == new_asset.id)
            .ok_or_else(|| AppError::NotFound(format!("catalog asset not found: {}", new_asset.id)))
    }

    /// 恢复使用内置默认 Generation Skill (M35-SKILL-02)
    pub(crate) async fn reset_generation_skill_to_default(&self) -> AppResult<()> {
        let mut current_settings = self.app_settings_value();
        if let Some(mem) = current_settings
            .get_mut("memory")
            .and_then(Value::as_object_mut)
        {
            mem.remove("generationSkillAssetId");
        }
        self.save_app_settings(current_settings).await?;
        Ok(())
    }

    /// 验证 Memory 设置完整性，包括 schedule 与 generation skill (M35-SKILL-06, M35-UI-07)
    pub(crate) async fn validate_memory_settings(&self, settings_val: &Value) -> AppResult<()> {
        let Some(memory_val) = settings_val.get("memory") else {
            return Ok(());
        };

        if let Ok(memory_settings) = serde_json::from_value::<
            crate::backend::app_settings::MemorySettings,
        >(memory_val.clone())
        {
            memory_settings.validate_schedule()?;

            if let Some(ref asset_id) = memory_settings.generation_skill_asset_id {
                let trimmed = asset_id.trim();
                if !trimmed.is_empty() && trimmed != MEMORY_GENERATION_SKILL_ID {
                    self.validate_generation_skill_asset(trimmed).await?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "generation_skill_tests.rs"]
mod tests;
