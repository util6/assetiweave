use crate::view_models::{OverviewViewModel, VersionViewModel};
use anyhow::{Context, Result};
use asset_core::application::{
    mounting::SourceInput, AppService, AssetRefParams, ImportSkillParams, ListAssetsParams,
    SourceAddParams, SourceRemoveParams, SourceScanParams,
};
use asset_core::domain::{
    AssetGroupDetail, AssetKind, CatalogAsset, Source, SourceKind, TargetProfileDescriptor, Tenant,
};

pub mod agent;
pub mod conversation;
pub mod harvester;
pub mod memory;
pub mod system;

pub struct CliService {
    app_service: AppService,
}

impl CliService {
    pub async fn bootstrap() -> Result<Self> {
        match AppService::open_for_engine().await {
            Ok(app_service) => Ok(Self { app_service }),
            Err(err) => {
                let err_detail = format!("{:#}", err);
                if err_detail.contains("missing in the resolved migrations") {
                    anyhow::bail!(
                        "failed to bootstrap AppService in OneShot CLI mode: detected outdated aiwc binary against database migrations.\n\
                         Caused by:\n    {}\n\n\
                         Hint: If developing in assetiweave workspace, set ASSETIWEAVE_ENV=development to use target/debug/aiwc, or run:\n    \
                         cargo build -p cli --bin aiwc\n\
                         or update the global CLI with:\n    \
                         pnpm cli:install",
                        err_detail
                    );
                }
                Err(err).context("failed to bootstrap AppService in OneShot CLI mode")
            }
        }
    }

    pub fn get_version_static() -> VersionViewModel {
        VersionViewModel {
            cli_version: env!("CARGO_PKG_VERSION").to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            compatible: true,
            capabilities: vec![
                "skill_mounting".to_string(),
                "conversation_search".to_string(),
                "in_process_runtime".to_string(),
            ],
        }
    }

    pub async fn get_overview(&self) -> Result<OverviewViewModel> {
        let overview = self
            .app_service
            .overview()
            .await
            .context("failed to load catalog overview from AppService")?;

        Ok(OverviewViewModel {
            source_count: overview.source_count,
            asset_count: overview.asset_count,
            profile_count: overview.profile_count,
            last_scan_status: overview.last_scan_status,
        })
    }

    // --- Skills ---

    pub async fn list_skills(&self) -> Result<Vec<CatalogAsset>> {
        self.app_service
            .list_skills()
            .await
            .context("failed to list skills")
    }

    pub async fn mount_skill(
        &self,
        asset_ref: String,
        profile_id: String,
        dry_run: bool,
    ) -> Result<serde_json::Value> {
        let params = AssetRefParams {
            asset_ref,
            profile_id: Some(profile_id),
            dry_run,
            yes: false,
            unmount: false,
        };
        let result = self
            .app_service
            .mount_skill(params, true)
            .await
            .context("failed to mount skill")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn unmount_skill(
        &self,
        asset_ref: String,
        profile_id: String,
        dry_run: bool,
    ) -> Result<serde_json::Value> {
        let params = AssetRefParams {
            asset_ref,
            profile_id: Some(profile_id),
            dry_run,
            yes: false,
            unmount: false,
        };
        let result = self
            .app_service
            .mount_skill(params, false)
            .await
            .context("failed to unmount skill")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn backup_skill(&self, asset_id: String) -> Result<serde_json::Value> {
        let result = self
            .app_service
            .backup_skill(asset_id)
            .await
            .context("failed to backup skill")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn import_skill(
        &self,
        from: String,
        name: Option<String>,
        dry_run: bool,
    ) -> Result<serde_json::Value> {
        let params = ImportSkillParams {
            from,
            name,
            dry_run,
        };
        let result = self
            .app_service
            .import_skill(params)
            .await
            .context("failed to import skill")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn delete_skill(
        &self,
        asset_ref: String,
        profile_id: Option<String>,
        dry_run: bool,
    ) -> Result<serde_json::Value> {
        let params = AssetRefParams {
            asset_ref,
            profile_id,
            dry_run,
            yes: true,
            unmount: true,
        };
        let result = self
            .app_service
            .delete_skill(params)
            .await
            .context("failed to delete skill")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn list_skill_groups(&self) -> Result<Vec<AssetGroupDetail>> {
        self.app_service
            .list_skill_groups()
            .await
            .context("failed to list skill groups")
    }

    pub async fn get_skill_group(&self, group_id: String) -> Result<AssetGroupDetail> {
        self.app_service
            .get_skill_group(group_id)
            .await
            .context("failed to get skill group")
    }

    // --- Assets ---

    pub async fn list_assets(&self, kind: Option<String>) -> Result<Vec<CatalogAsset>> {
        let kind_enum = kind.and_then(|k| match k.to_lowercase().as_str() {
            "skill" => Some(AssetKind::Skill),
            "mcp" => Some(AssetKind::Mcp),
            "prompt" => Some(AssetKind::Prompt),
            "rule" => Some(AssetKind::Rule),
            _ => None,
        });

        self.app_service
            .list_assets(ListAssetsParams { kind: kind_enum })
            .await
            .context("failed to list assets")
    }

    // --- Sources ---

    pub async fn list_sources(&self) -> Result<Vec<Source>> {
        self.app_service
            .list_sources()
            .await
            .context("failed to list sources")
    }

    pub async fn add_source(
        &self,
        root_path: String,
        name: Option<String>,
        kind: Option<String>,
        dry_run: bool,
    ) -> Result<serde_json::Value> {
        let default_kind = kind.and_then(|k| match k.to_lowercase().as_str() {
            "skill" => Some(AssetKind::Skill),
            "mcp" => Some(AssetKind::Mcp),
            "prompt" => Some(AssetKind::Prompt),
            "rule" => Some(AssetKind::Rule),
            _ => None,
        });

        let source_name = name.unwrap_or_else(|| {
            std::path::Path::new(&root_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Custom Source")
                .to_string()
        });

        let source = SourceInput {
            id: None,
            name: source_name,
            kind: SourceKind::Local,
            root_path,
            scanner_kind: None,
            source_origin: None,
            repo_root: None,
            scan_root: None,
            origin_app_kind: None,
            origin_provider_id: None,
            include_globs: vec![],
            exclude_globs: vec![],
            default_kind,
            enabled: true,
            priority: 0,
        };

        let params = SourceAddParams { source, dry_run };

        let result = self
            .app_service
            .add_source_with_options(params)
            .await
            .context("failed to add source")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn remove_source(
        &self,
        id: String,
        yes: bool,
        dry_run: bool,
    ) -> Result<serde_json::Value> {
        let params = SourceRemoveParams { id, dry_run, yes };
        let result = self
            .app_service
            .remove_source(params)
            .await
            .context("failed to remove source")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn scan_sources(
        &self,
        kind: Option<String>,
        dry_run: bool,
    ) -> Result<serde_json::Value> {
        let kind_enum = kind.and_then(|k| match k.to_lowercase().as_str() {
            "skill" => Some(AssetKind::Skill),
            "mcp" => Some(AssetKind::Mcp),
            "prompt" => Some(AssetKind::Prompt),
            "rule" => Some(AssetKind::Rule),
            _ => None,
        });

        let params = SourceScanParams {
            kind: kind_enum,
            dry_run,
        };
        let result = self
            .app_service
            .scan_sources(params)
            .await
            .context("failed to scan sources")?;
        Ok(serde_json::to_value(result)?)
    }

    // --- Tenants ---

    pub async fn list_tenants(&self) -> Result<Vec<Tenant>> {
        self.app_service
            .list_tenants()
            .await
            .context("failed to list tenants")
    }

    pub async fn active_tenant(&self) -> Result<Tenant> {
        self.app_service
            .active_tenant()
            .await
            .context("failed to get active tenant")
    }

    pub async fn switch_tenant(&self, tenant_id: String) -> Result<Tenant> {
        self.app_service
            .switch_tenant(tenant_id)
            .await
            .context("failed to switch tenant")
    }

    // --- Profiles ---

    pub fn list_profiles(&self) -> Result<Vec<TargetProfileDescriptor>> {
        self.app_service
            .list_target_profile_descriptors()
            .context("failed to list target profile descriptors")
    }

    // --- Conversations ---

    pub async fn search_conversations(
        &self,
        query: String,
        kinds: Vec<String>,
        limit: usize,
        offset: usize,
        since: Option<String>,
        until: Option<String>,
        timeline: bool,
    ) -> Result<asset_core::application::ConversationSearchResult> {
        let params = asset_core::application::ConversationSearchParams {
            record_kind: None,
            adapter_id: None,
            source_id: None,
            project_path: None,
            query,
            content_types: vec![],
            card_kinds: kinds,
            semantic_roles: vec![],
            include_questions: None,
            include_cards: None,
            since,
            until,
            timeline,
            limit: Some(limit),
            offset: Some(offset),
            search_options: None,
        };
        self.app_service
            .search_conversation_records(params)
            .await
            .context("failed to search conversation records")
    }

    pub async fn list_conversation_sessions(
        &self,
        args: &crate::args::conversation::ConversationListArgs,
    ) -> Result<Vec<asset_core::domain::conversations::ConversationSessionListItem>> {
        let project_path = if args.current_project {
            std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().to_string())
        } else {
            args.project.clone()
        };
        let params = asset_core::application::ConversationSessionListParams {
            adapter_id: args.adapter.clone(),
            source_id: None,
            query: args.query.clone(),
            limit: Some(args.limit),
            offset: Some(args.offset),
            project_path,
            since: args.since.clone(),
            until: args.until.clone(),
            window: args.window.clone(),
            range: args.range.clone(),
        };
        self.app_service
            .list_conversation_sessions(params)
            .await
            .context("failed to list conversation sessions")
    }

    pub async fn sync_conversations(
        &self,
        adapter_id: Option<String>,
        source_id: Option<String>,
        force: bool,
    ) -> Result<serde_json::Value> {
        let mode = if force {
            asset_core::application::ConversationSyncMode::Full
        } else {
            asset_core::application::ConversationSyncMode::Incremental
        };
        let params = asset_core::application::ConversationSyncParams {
            source_id,
            adapter_id,
            record_kind: None,
            mode,
            dry_run: false,
        };
        let result = self
            .app_service
            .sync_conversations(params)
            .await
            .context("failed to synchronize conversations")?;
        Ok(serde_json::to_value(result)?)
    }

    pub fn list_conversation_adapters(
        &self,
    ) -> Result<Vec<asset_core::domain::conversations::ConversationAdapter>> {
        self.app_service
            .list_conversation_adapters()
            .context("failed to list conversation adapters")
    }

    // --- Sub-operations Accessors ---

    pub fn system(&self) -> system::SystemOperations<'_> {
        system::SystemOperations::new(&self.app_service)
    }

    pub fn memory(&self) -> memory::MemoryOperations<'_> {
        memory::MemoryOperations::new(&self.app_service)
    }

    pub fn agent(&self) -> agent::AgentOperations<'_> {
        agent::AgentOperations::new(&self.app_service)
    }

    pub fn conversation(&self) -> conversation::ConversationOperations<'_> {
        conversation::ConversationOperations::new(&self.app_service)
    }

    #[allow(dead_code)]
    pub fn harvester(&self) -> harvester::HarvesterOperations {
        harvester::HarvesterOperations
    }

    // --- Skills Acquisition & Remote ---

    pub async fn search_skills(
        &self,
        query: String,
        provider: String,
        limit: usize,
    ) -> Result<serde_json::Value> {
        let params = asset_core::application::SkillSearchParams {
            query,
            provider: Some(provider),
            limit: Some(limit),
        };
        let result = self
            .app_service
            .search_skills(params)
            .context("failed to search skills")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn acquire_skill(
        &self,
        params: asset_core::application::SkillAcquireParams,
    ) -> Result<serde_json::Value> {
        let result = self
            .app_service
            .acquire_skill(params)
            .await
            .context("failed to acquire skill")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn list_skill_remote_sources(&self) -> Result<serde_json::Value> {
        let result = self
            .app_service
            .list_skill_remote_sources()
            .await
            .context("failed to list skill remote sources")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn check_skill_remote_sources(
        &self,
        asset_id: Option<String>,
    ) -> Result<serde_json::Value> {
        let params = asset_core::application::SkillRemoteCheckParams { asset_id };
        let result = self
            .app_service
            .check_skill_remote_sources(params)
            .await
            .context("failed to check skill remote sources")?;
        Ok(serde_json::to_value(result)?)
    }
}
