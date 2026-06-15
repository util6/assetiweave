use crate::models::{
    Asset, AssetGroup, AssetGroupDetail, AssetKind, AssetMount, ConversationAdapter,
    ConversationSource, DeploymentPlan, DeploymentStrategy, Source, SourceOrigin,
    SourceScannerKind, TargetProfile,
};
use crate::{
    app_settings, commands, conversations, executor, logs, path_utils, planner, platform, scanner,
    store, targeting,
    types::{
        AppOverview, AppResult, AppShortcut, ApplyAssetGroupMountResult,
        ApplySkillGroupExclusiveMountResult, AssetGroupInput, AssetMountStatus,
        AssetMountUpdateResult, CatalogAsset, ExecutionResult, NavigationModel,
        PhysicalMountStateDto, SkillBackupSettings, SkillGroupExclusiveMountInput,
        SkillGroupExclusiveMountPreview, SkillRemoteSource, SourceInput, TargetProfileInput,
    },
};
use chrono::Utc;
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
use uuid::Uuid;

pub(crate) struct AppService {
    conn: Connection,
    db_path: PathBuf,
}

const SKILL_REMOTE_SECURITY_NOTICE: &str =
    "Review remote Skill contents before importing; AssetIWeave does not execute or trust remote code automatically.";

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ListAssetsParams {
    pub(crate) kind: Option<AssetKind>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AssetIdParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct RequiredAssetIdParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateAssetDescriptionParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: String,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct DeleteAssetParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: String,
    #[serde(default)]
    pub(crate) unmount: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ProfileIdParams {
    #[serde(alias = "profileId")]
    pub(crate) profile_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct IdParams {
    pub(crate) id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AssetProfileParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: String,
    #[serde(alias = "profileId")]
    pub(crate) profile_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SetAssetMountParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: String,
    #[serde(alias = "profileId")]
    pub(crate) profile_id: String,
    pub(crate) enabled: bool,
    pub(crate) strategy: Option<DeploymentStrategy>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ApplySkillGroupMountParams {
    #[serde(alias = "groupId")]
    pub(crate) group_id: String,
    #[serde(alias = "profileId")]
    pub(crate) profile_id: String,
    pub(crate) enabled: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateSourceParams {
    pub(crate) source: SourceInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateSourceParams {
    pub(crate) source: Source,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateProfileParams {
    pub(crate) input: TargetProfileInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateProfileParams {
    pub(crate) profile: TargetProfile,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateNavigationModelParams {
    pub(crate) model: NavigationModel,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateAppShortcutsParams {
    pub(crate) shortcuts: Vec<AppShortcut>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateSkillGroupParams {
    pub(crate) input: AssetGroupInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateSkillGroupParams {
    pub(crate) group: AssetGroup,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GroupIdParams {
    #[serde(alias = "groupId")]
    pub(crate) group_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SetSkillGroupManualMembersParams {
    #[serde(alias = "groupId")]
    pub(crate) group_id: String,
    #[serde(alias = "assetIds")]
    pub(crate) asset_ids: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SkillGroupExclusiveMountParams {
    pub(crate) input: SkillGroupExclusiveMountInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ExecutePlanParams {
    pub(crate) plan: DeploymentPlan,
    #[serde(alias = "actionIds")]
    pub(crate) action_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct LogsGetSnapshotParams {
    #[serde(alias = "fileName")]
    pub(crate) file_name: Option<String>,
    #[serde(alias = "lineLimit")]
    pub(crate) line_limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct LogsWriteOperationParams {
    pub(crate) level: String,
    pub(crate) operation: String,
    pub(crate) message: String,
    pub(crate) fields: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct RevealPathParams {
    pub(crate) path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SaveAppSettingsParams {
    pub(crate) settings: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SourceAddParams {
    #[serde(flatten)]
    pub(crate) source: SourceInput,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AssetRefParams {
    #[serde(alias = "assetRef")]
    pub(crate) asset_ref: String,
    #[serde(alias = "profileId")]
    pub(crate) profile_id: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
    #[serde(default)]
    pub(crate) unmount: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ImportSkillParams {
    pub(crate) from: String,
    pub(crate) name: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SkillSearchParams {
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) provider: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SkillAcquireParams {
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) branch: Option<String>,
    #[serde(default)]
    pub(crate) path: Option<String>,
    pub(crate) name: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SkillRemoteCheckParams {
    #[serde(default, alias = "assetId")]
    pub(crate) asset_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SkillSearchResult {
    pub(crate) query: String,
    pub(crate) provider: String,
    pub(crate) candidates: Vec<SkillSearchCandidate>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SkillSearchCandidate {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) match_reason: Option<String>,
    pub(crate) url: String,
    pub(crate) path: Option<String>,
    pub(crate) clone_url: Option<String>,
    pub(crate) default_branch: Option<String>,
    pub(crate) stars: Option<u64>,
    pub(crate) acquire_command: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateSkillBackupSettingsParams {
    #[serde(alias = "rootPath")]
    pub(crate) root_path: String,
    #[serde(default)]
    pub(crate) migrate: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SourceRemoveParams {
    pub(crate) id: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SourceScanParams {
    pub(crate) kind: Option<AssetKind>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SkillGroupMountParams {
    #[serde(alias = "groupId")]
    pub(crate) group_id: String,
    #[serde(alias = "profileId")]
    pub(crate) profile_id: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterUnregisterParams {
    #[serde(alias = "adapterId")]
    pub(crate) adapter_id: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationSourceUpsertParams {
    pub(crate) source: ConversationSource,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationSourceDisableParams {
    pub(crate) id: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationSyncParams {
    #[serde(alias = "sourceId")]
    pub(crate) source_id: Option<String>,
    #[serde(alias = "adapterId")]
    pub(crate) adapter_id: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationSessionListParams {
    #[serde(alias = "adapterId")]
    pub(crate) adapter_id: Option<String>,
    #[serde(alias = "sourceId")]
    pub(crate) source_id: Option<String>,
    pub(crate) query: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationSessionGetParams {
    #[serde(alias = "sessionId")]
    pub(crate) session_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationSessionExportParams {
    #[serde(alias = "sessionId")]
    pub(crate) session_id: String,
    #[serde(alias = "outputRoot")]
    pub(crate) output_root: String,
    #[serde(default, alias = "questionIds")]
    pub(crate) question_ids: Vec<String>,
    #[serde(default, alias = "contentFilter")]
    pub(crate) content_filter: store::ConversationExportContentFilter,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationQuestionListParams {
    #[serde(alias = "sessionId")]
    pub(crate) session_id: String,
    pub(crate) query: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationQuestionGetParams {
    #[serde(alias = "questionId")]
    pub(crate) question_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationQuestionMergeParams {
    #[serde(alias = "questionIds")]
    pub(crate) question_ids: Vec<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationQuestionSplitParams {
    #[serde(alias = "questionId")]
    pub(crate) question_id: String,
    #[serde(alias = "beforeTurnId")]
    pub(crate) before_turn_id: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

impl AppService {
    pub(crate) fn open_for_engine() -> AppResult<Self> {
        Self::open_with_db_path(engine_db_path()?)
    }

    pub(crate) fn open_with_db_path(db_path: PathBuf) -> AppResult<Self> {
        let conn = store::open_initialized(&db_path)?;
        Ok(Self { conn, db_path })
    }

    pub(crate) fn overview(&self) -> AppResult<AppOverview> {
        Ok(AppOverview {
            source_count: store::count_rows(&self.conn, "sources")?,
            asset_count: store::count_rows(&self.conn, "assets")?,
            profile_count: store::count_rows(&self.conn, "profiles")?,
            last_scan_status: store::latest_scan_status(&self.conn)?,
        })
    }

    pub(crate) fn list_sources(&self) -> AppResult<Vec<Source>> {
        store::load_sources(&self.conn)
    }

    pub(crate) fn list_skill_sources(&self) -> AppResult<Vec<Source>> {
        store::load_skill_sources(&self.conn)
    }

    pub(crate) fn add_source(&self, source: SourceInput) -> AppResult<Source> {
        let source = source_from_input(source);
        store::upsert_source(&self.conn, &source)?;
        Ok(source)
    }

    pub(crate) fn update_source(&self, source: Source) -> AppResult<Source> {
        let source = store::normalize_source(&source);
        if !store::load_sources(&self.conn)?
            .iter()
            .any(|candidate| candidate.id == source.id)
        {
            return Err(format!("source not found: {}", source.id));
        }
        store::upsert_source(&self.conn, &source)?;
        Ok(source)
    }

    pub(crate) fn delete_source(&self, id: String) -> AppResult<()> {
        self.remove_source(SourceRemoveParams {
            id,
            dry_run: false,
            yes: true,
        })
        .map(|_| ())
    }

    pub(crate) fn add_source_with_options(&self, params: SourceAddParams) -> AppResult<Value> {
        let source = source_from_input(params.source);
        if params.dry_run {
            return Ok(json!({ "dry_run": true, "source": source }));
        }
        store::upsert_source(&self.conn, &source)?;
        Ok(json!({ "dry_run": false, "source": source }))
    }

    pub(crate) fn remove_source(&self, params: SourceRemoveParams) -> AppResult<Value> {
        if !params.dry_run && !params.yes {
            return Err("source.remove requires --yes".to_string());
        }
        let sources = store::load_sources(&self.conn)?;
        let source = sources
            .into_iter()
            .find(|source| source.id == params.id)
            .ok_or_else(|| format!("source not found: {}", params.id))?;
        if is_protected_source(&source) {
            return Err(
                "default Skill source is managed by AssetIWeave and cannot be deleted".to_string(),
            );
        }
        if params.dry_run {
            return Ok(json!({ "removed": false, "dry_run": true, "source": source }));
        }
        store::delete_source(&self.conn, &source.id)?;
        commands::cleanup_orphan_asset_records(&self.conn)?;
        Ok(json!({ "removed": true, "source_id": source.id }))
    }

    pub(crate) fn scan_sources(&self, params: SourceScanParams) -> AppResult<Vec<CatalogAsset>> {
        if params.dry_run {
            return commands::catalog_assets(&self.conn, params.kind);
        }
        commands::refresh_all_sources(&self.conn)?;
        commands::catalog_assets(&self.conn, params.kind)
    }

    pub(crate) fn scan_skill_sources(&self) -> AppResult<Vec<CatalogAsset>> {
        let sources = store::load_skill_sources(&self.conn)?;
        commands::scan_selected_sources(&self.conn, sources, scanner::scan_skill_source)?;
        commands::catalog_assets(&self.conn, Some(AssetKind::Skill))
    }

    pub(crate) fn list_conversation_adapters(&self) -> AppResult<Vec<ConversationAdapter>> {
        store::list_conversation_adapters(&self.conn)
    }

    pub(crate) fn scaffold_conversation_adapter(
        &self,
        params: conversations::ExternalAdapterScaffoldParams,
    ) -> AppResult<conversations::ExternalAdapterScaffoldResult> {
        conversations::scaffold_external_adapter(params)
    }

    pub(crate) fn validate_conversation_adapter(
        &self,
        params: conversations::ExternalAdapterValidateParams,
    ) -> AppResult<conversations::ExternalAdapterValidationResult> {
        conversations::validate_external_adapter(params)
    }

    pub(crate) fn register_conversation_adapter(
        &self,
        params: conversations::ExternalAdapterRegisterParams,
    ) -> AppResult<Value> {
        let dry_run = params.dry_run;
        let preview = conversations::register_external_adapter(params)?;
        if !dry_run {
            let adapter = conversations::adapter_from_registration_preview(preview.clone())?;
            store::upsert_conversation_adapter(&self.conn, &adapter)?;
        }
        Ok(preview)
    }

    pub(crate) fn unregister_conversation_adapter(
        &self,
        params: ConversationAdapterUnregisterParams,
    ) -> AppResult<Value> {
        if !params.dry_run && !params.yes {
            return Err("conversation.adapter.unregister requires --yes".to_string());
        }
        let adapter = store::load_conversation_adapter(&self.conn, &params.adapter_id)?
            .ok_or_else(|| format!("conversation adapter not found: {}", params.adapter_id))?;
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "unregistered": false,
                "adapter": adapter
            }));
        }
        let adapter = store::delete_conversation_adapter(&self.conn, &params.adapter_id)?;
        Ok(json!({
            "dry_run": false,
            "unregistered": true,
            "adapter": adapter
        }))
    }

    pub(crate) fn try_run_conversation_adapter(
        &self,
        params: conversations::ExternalAdapterTryRunParams,
    ) -> AppResult<conversations::ExternalAdapterRunResult> {
        conversations::try_run_external_adapter(params)
    }

    pub(crate) fn list_conversation_sources(&self) -> AppResult<Vec<ConversationSource>> {
        store::list_conversation_sources(&self.conn)
    }

    pub(crate) fn upsert_conversation_source(
        &self,
        params: ConversationSourceUpsertParams,
    ) -> AppResult<Value> {
        if store::load_conversation_adapter(&self.conn, &params.source.adapter_id)?.is_none() {
            return Err(format!(
                "conversation adapter not found: {}",
                params.source.adapter_id
            ));
        }
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "source": params.source
            }));
        }
        store::upsert_conversation_source(&self.conn, &params.source)?;
        Ok(json!({
            "dry_run": false,
            "source": params.source
        }))
    }

    pub(crate) fn disable_conversation_source(
        &self,
        params: ConversationSourceDisableParams,
    ) -> AppResult<Value> {
        let source = store::load_conversation_source(&self.conn, &params.id)?
            .ok_or_else(|| format!("conversation source not found: {}", params.id))?;
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "disabled": false,
                "source": source
            }));
        }
        let source = store::disable_conversation_source(&self.conn, &params.id)?;
        Ok(json!({
            "dry_run": false,
            "disabled": true,
            "source": source
        }))
    }

    pub(crate) fn sync_conversations(&self, params: ConversationSyncParams) -> AppResult<Value> {
        let sources = store::list_conversation_sources(&self.conn)?
            .into_iter()
            .filter(|source| params.source_id.as_deref().is_none_or(|id| id == source.id))
            .filter(|source| {
                params
                    .adapter_id
                    .as_deref()
                    .is_none_or(|id| id == source.adapter_id)
            })
            .filter(|source| {
                source.enabled || params.source_id.as_deref() == Some(source.id.as_str())
            })
            .collect::<Vec<_>>();
        if sources.is_empty() {
            return Err("no matching conversation sources".to_string());
        }

        let mut results = Vec::new();
        let mut errors = Vec::new();
        for source in sources {
            match conversations::read_source_sessions(&source).and_then(|sessions| {
                store::import_conversation_sessions(&self.conn, &source, &sessions, params.dry_run)
            }) {
                Ok(result) => results.push(json!(result)),
                Err(error) if params.source_id.is_some() => return Err(error),
                Err(error) => errors.push(json!({
                    "source_id": source.id,
                    "adapter_id": source.adapter_id,
                    "message": error
                })),
            }
        }
        Ok(json!({
            "dry_run": params.dry_run,
            "results": results,
            "errors": errors
        }))
    }

    pub(crate) fn list_conversation_sessions(
        &self,
        params: ConversationSessionListParams,
    ) -> AppResult<Vec<store::ConversationSessionListItem>> {
        store::list_conversation_sessions(
            &self.conn,
            params.adapter_id.as_deref(),
            params.source_id.as_deref(),
            params.query.as_deref(),
            params.limit.unwrap_or(50).clamp(1, 500),
            params.offset.unwrap_or(0),
        )
    }

    pub(crate) fn get_conversation_session(
        &self,
        params: ConversationSessionGetParams,
    ) -> AppResult<store::ConversationSessionDetail> {
        store::load_conversation_session_detail(&self.conn, &params.session_id)
    }

    pub(crate) fn export_conversation_session(
        &self,
        params: ConversationSessionExportParams,
    ) -> AppResult<Value> {
        let detail = store::load_conversation_session_detail(&self.conn, &params.session_id)?;
        let output_root = path_utils::expand_path(&params.output_root)?;
        let project_segment = detail
            .session
            .project_path
            .as_deref()
            .and_then(|path| Path::new(path).file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("unknown-project");
        let short_id = detail
            .session
            .id
            .chars()
            .rev()
            .take(8)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        let question_count = params.question_ids.len();
        let file_stem = if question_count == 0 {
            sanitize_path_segment(&detail.session.title)
        } else {
            format!(
                "{}-selected-{question_count}",
                sanitize_path_segment(&detail.session.title)
            )
        };
        let target_path = output_root
            .join(sanitize_path_segment(&detail.session.adapter_id))
            .join(sanitize_path_segment(project_segment))
            .join(format!("{file_stem}-{short_id}.md"));
        let content = if params.question_ids.is_empty() {
            store::render_session_markdown_with_filter(
                &self.conn,
                &params.session_id,
                &params.content_filter,
            )?
        } else {
            store::render_session_markdown_for_questions_with_filter(
                &self.conn,
                &params.session_id,
                &params.question_ids,
                &params.content_filter,
            )?
        };
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "written": false,
                "path": target_path,
                "bytes": content.len(),
                "question_ids": params.question_ids,
                "question_count": question_count
            }));
        }
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&target_path, &content).map_err(|error| error.to_string())?;
        Ok(json!({
            "dry_run": false,
            "written": true,
            "path": target_path,
            "bytes": content.len(),
            "question_ids": params.question_ids,
            "question_count": question_count
        }))
    }

    pub(crate) fn list_conversation_questions(
        &self,
        params: ConversationQuestionListParams,
    ) -> AppResult<Vec<store::ConversationQuestionDetail>> {
        store::list_conversation_question_details(
            &self.conn,
            &params.session_id,
            params.query.as_deref(),
            params.limit.unwrap_or(100).clamp(1, 500),
            params.offset.unwrap_or(0),
        )
    }

    pub(crate) fn get_conversation_question(
        &self,
        params: ConversationQuestionGetParams,
    ) -> AppResult<store::ConversationQuestionDetail> {
        store::load_conversation_question_detail(&self.conn, &params.question_id)
    }

    pub(crate) fn merge_conversation_questions(
        &self,
        params: ConversationQuestionMergeParams,
    ) -> AppResult<store::ConversationMutationResult> {
        store::merge_conversation_questions(&self.conn, &params.question_ids, params.dry_run)
    }

    pub(crate) fn split_conversation_question(
        &self,
        params: ConversationQuestionSplitParams,
    ) -> AppResult<store::ConversationMutationResult> {
        store::split_conversation_question(
            &self.conn,
            &params.question_id,
            &params.before_turn_id,
            params.dry_run,
        )
    }

    pub(crate) fn list_profiles(&self) -> AppResult<Vec<TargetProfile>> {
        store::load_profiles(&self.conn)
    }

    pub(crate) fn create_profile(&self, input: TargetProfileInput) -> AppResult<TargetProfile> {
        let profile = commands::target_profile_from_input(input)?;
        if store::load_profiles(&self.conn)?
            .iter()
            .any(|candidate| candidate.id == profile.id)
        {
            return Err(format!("profile already exists: {}", profile.id));
        }
        store::upsert_profile(&self.conn, &profile)?;
        Ok(profile)
    }

    pub(crate) fn update_profile(&self, profile: TargetProfile) -> AppResult<TargetProfile> {
        commands::validate_target_profile(&profile)?;
        let existing_profile = store::load_profiles(&self.conn)?
            .into_iter()
            .find(|candidate| candidate.id == profile.id);
        let Some(existing_profile) = existing_profile else {
            return Err(format!("profile not found: {}", profile.id));
        };
        commands::ensure_default_profile_update_is_allowed(&existing_profile, &profile)?;
        store::upsert_profile(&self.conn, &profile)?;
        Ok(profile)
    }

    pub(crate) fn delete_profile(&self, id: String) -> AppResult<()> {
        if !store::load_profiles(&self.conn)?
            .iter()
            .any(|profile| profile.id == id)
        {
            return Err(format!("profile not found: {id}"));
        }
        commands::ensure_profile_can_be_deleted(&self.conn, &id)?;
        store::delete_profile(&self.conn, &id)
    }

    pub(crate) fn navigation_model(&self) -> AppResult<crate::types::NavigationModel> {
        store::load_navigation_model(&self.conn)
    }

    pub(crate) fn update_navigation_model(
        &self,
        model: NavigationModel,
    ) -> AppResult<NavigationModel> {
        store::save_navigation_model(&self.conn, &model)?;
        store::load_navigation_model(&self.conn)
    }

    pub(crate) fn list_app_shortcuts(&self) -> AppResult<Vec<crate::types::AppShortcut>> {
        store::load_app_shortcuts(&self.conn)
    }

    pub(crate) fn list_app_shortcut_settings(&self) -> AppResult<Vec<crate::types::AppShortcut>> {
        store::load_app_shortcut_settings(&self.conn)
    }

    pub(crate) fn update_app_shortcuts(
        &self,
        shortcuts: Vec<AppShortcut>,
    ) -> AppResult<Vec<AppShortcut>> {
        store::save_app_shortcuts(&self.conn, &shortcuts)?;
        store::load_app_shortcut_settings(&self.conn)
    }

    pub(crate) fn list_assets(&self, params: ListAssetsParams) -> AppResult<Vec<CatalogAsset>> {
        commands::catalog_assets(&self.conn, params.kind)
    }

    pub(crate) fn update_asset_description(
        &self,
        asset_id: String,
        description: Option<String>,
    ) -> AppResult<Asset> {
        let mut asset = store::load_assets(&self.conn)?
            .into_iter()
            .find(|asset| asset.id == asset_id)
            .ok_or_else(|| format!("asset not found: {asset_id}"))?;
        if !store::load_sources(&self.conn)?
            .iter()
            .any(|source| source.id == asset.source_id)
        {
            return Err(format!("source not found: {}", asset.source_id));
        }

        let source_path = path_utils::expand_path(&asset.absolute_path)?;
        if !source_path.exists() {
            return Err(format!(
                "asset source path does not exist: {}",
                source_path.display()
            ));
        }

        asset.description = description
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        asset.updated_at = Utc::now().to_rfc3339();
        store::update_asset_description(&self.conn, &asset)?;
        Ok(asset)
    }

    pub(crate) fn delete_asset(&self, asset_id: String, unmount: bool) -> AppResult<Asset> {
        let asset = store::load_assets(&self.conn)?
            .into_iter()
            .find(|asset| asset.id == asset_id)
            .ok_or_else(|| format!("asset not found: {asset_id}"))?;
        if asset.kind != AssetKind::Skill {
            return Err("only skill assets can be deleted from the catalog".to_string());
        }
        self.delete_skill(AssetRefParams {
            asset_ref: asset.id.clone(),
            profile_id: None,
            dry_run: false,
            yes: true,
            unmount,
        })?;
        Ok(asset)
    }

    pub(crate) fn list_asset_mounts(&self, asset_id: Option<&str>) -> AppResult<Vec<AssetMount>> {
        store::load_asset_mounts(&self.conn, asset_id)
    }

    pub(crate) fn list_asset_mount_statuses(
        &self,
        asset_id: Option<&str>,
    ) -> AppResult<Vec<AssetMountStatus>> {
        commands::scan_asset_mount_statuses(&self.conn, asset_id)
    }

    pub(crate) fn refresh_asset_mount_statuses(
        &self,
        asset_id: Option<&str>,
    ) -> AppResult<Vec<AssetMountStatus>> {
        commands::sync_asset_mount_observations(&self.conn, asset_id)
    }

    pub(crate) fn create_plan(&self, profile_id: Option<&str>) -> AppResult<DeploymentPlan> {
        let assets = store::load_assets(&self.conn)?;
        let profiles = store::load_profiles(&self.conn)?;
        let mounts = store::load_enabled_asset_mounts(&self.conn, profile_id)?;
        Ok(planner::build_plan(&assets, &profiles, &mounts, profile_id))
    }

    pub(crate) fn list_skills(&self) -> AppResult<Vec<CatalogAsset>> {
        commands::catalog_assets(&self.conn, Some(AssetKind::Skill))
    }

    pub(crate) fn get_skill_backup_settings(&self) -> AppResult<SkillBackupSettings> {
        commands::skill_backup_settings(&self.conn)
    }

    pub(crate) fn update_skill_backup_settings(
        &self,
        params: UpdateSkillBackupSettingsParams,
    ) -> AppResult<SkillBackupSettings> {
        let root_path = params.root_path.trim().to_string();
        if root_path.is_empty() {
            return Err("skill backup root path is required".to_string());
        }

        let current = commands::skill_backup_settings(&self.conn)?;
        let current_root = PathBuf::from(&current.expanded_root_path);
        let next_root = path_utils::expand_path(&root_path)?;
        if commands::same_path_or_text(&current_root, &next_root) {
            let source = commands::assetiweave_library_source_with_root(root_path);
            store::upsert_source(&self.conn, &source)?;
            return commands::skill_backup_settings(&self.conn);
        }

        if params.migrate {
            if !current.is_default_root && path_contains(&current_root, &next_root) {
                return Err(
                    "custom backup migration target cannot be inside the old backup directory"
                        .to_string(),
                );
            }
            fs::create_dir_all(&next_root).map_err(|error| error.to_string())?;
            commands::copy_dir_without_conflicts(&current_root, &next_root)?;
        } else {
            fs::create_dir_all(&next_root).map_err(|error| error.to_string())?;
        }

        let source = commands::assetiweave_library_source_with_root(root_path);
        store::upsert_source(&self.conn, &source)?;
        commands::refresh_all_sources(&self.conn)?;

        if params.migrate && !current.is_default_root && current_root.exists() {
            fs::remove_dir_all(&current_root).map_err(|error| error.to_string())?;
        }

        commands::skill_backup_settings(&self.conn)
    }

    pub(crate) fn backup_skill(&self, asset_id: String) -> AppResult<CatalogAsset> {
        let assets = store::load_assets(&self.conn)?;
        let asset = assets
            .iter()
            .find(|candidate| candidate.id == asset_id)
            .ok_or_else(|| format!("asset not found: {asset_id}"))?;
        if asset.kind != AssetKind::Skill {
            return Err("only skill assets can be backed up".to_string());
        }

        let source = store::load_sources(&self.conn)?
            .into_iter()
            .find(|candidate| candidate.id == asset.source_id)
            .ok_or_else(|| format!("source not found: {}", asset.source_id))?;
        if source.source_origin == SourceOrigin::AssetiweaveLibrary {
            return commands::catalog_asset_for_id(&self.conn, &asset.id);
        }

        let origin_bucket = source
            .origin_app_kind
            .map(|kind| format!("{kind:?}").to_ascii_lowercase())
            .unwrap_or_else(|| slug_path_segment(&source.id));
        let target_dir = commands::skill_backup_root(&self.conn)?
            .join("backed-up")
            .join(origin_bucket)
            .join(&asset.name);
        let source_path = Path::new(&asset.absolute_path);
        if target_dir.exists() {
            let source_hash = path_utils::hash_path(source_path)?;
            let target_hash = path_utils::hash_path(&target_dir)?;
            if source_hash != target_hash {
                return Err(format!(
                    "backup skill target already exists with different content: {}",
                    target_dir.display()
                ));
            }
        } else {
            commands::copy_dir(source_path, &target_dir)?;
        }

        let library_source = commands::assetiweave_library_source_with_root(
            commands::skill_backup_settings(&self.conn)?.root_path,
        );
        store::upsert_source(&self.conn, &library_source)?;
        commands::refresh_all_sources(&self.conn)?;

        commands::catalog_assets(&self.conn, Some(AssetKind::Skill))?
            .into_iter()
            .find(|candidate| {
                candidate.asset.id == asset.id
                    || candidate.asset.absolute_path == target_dir.to_string_lossy()
                    || (asset.content_hash.is_some()
                        && candidate.asset.content_hash.as_deref() == asset.content_hash.as_deref())
            })
            .ok_or_else(|| "backed up skill was copied but not found during rescan".to_string())
    }

    pub(crate) fn import_skill(&self, params: ImportSkillParams) -> AppResult<Value> {
        let source_dir = path_utils::expand_path(&params.from)?;
        if !source_dir.is_dir() {
            return Err(format!(
                "skill import source is not a directory: {}",
                source_dir.display()
            ));
        }
        let skill_file = source_dir.join("SKILL.md");
        if !skill_file.is_file() {
            return Err(format!(
                "skill import source must contain SKILL.md: {}",
                skill_file.display()
            ));
        }

        let name = params
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .or_else(|| {
                source_dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .ok_or_else(|| "skill import name could not be inferred".to_string())?;
        let target_dir = commands::skill_backup_root(&self.conn)?
            .join("downloaded")
            .join(&name);
        if target_dir.exists() {
            return Err(format!(
                "downloaded skill already exists: {}",
                target_dir.display()
            ));
        }

        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "source_path": source_dir,
                "target_path": target_dir,
                "name": name
            }));
        }

        commands::copy_dir(&source_dir, &target_dir)?;
        let library_source = commands::assetiweave_library_source_with_root(
            commands::skill_backup_settings(&self.conn)?.root_path,
        );
        store::upsert_source(&self.conn, &library_source)?;
        let library_assets = scanner::scan_skill_source(&library_source)?;
        store::replace_source_assets(&self.conn, &library_source.id, &library_assets)?;
        let asset = library_assets
            .into_iter()
            .find(|candidate| candidate.absolute_path == target_dir.to_string_lossy())
            .ok_or_else(|| "imported skill was copied but not found during rescan".to_string())?;
        Ok(json!({ "dry_run": false, "asset": asset }))
    }

    pub(crate) fn search_skills(&self, params: SkillSearchParams) -> AppResult<SkillSearchResult> {
        let query = params.query.trim();
        if query.is_empty() {
            return Err("skill search query is required".to_string());
        }
        let provider = normalize_skill_search_provider(params.provider.as_deref())?;
        let limit = params.limit.unwrap_or(10).clamp(1, 20);
        let (mut candidates, warnings) = match provider.as_str() {
            "github" => github_repository_skill_search(query, limit)?,
            "github-code" => github_code_skill_search(query, limit)?,
            _ => return Err(format!("unsupported skill search provider: {provider}")),
        };
        let query_terms = search_query_terms(query);
        candidates.sort_by(|left, right| {
            skill_candidate_score(right, &query_terms)
                .cmp(&skill_candidate_score(left, &query_terms))
                .then_with(|| {
                    right
                        .stars
                        .unwrap_or_default()
                        .cmp(&left.stars.unwrap_or_default())
                })
                .then_with(|| left.name.cmp(&right.name))
        });
        candidates.truncate(limit);
        Ok(SkillSearchResult {
            query: query.to_string(),
            provider,
            candidates,
            warnings,
        })
    }

    pub(crate) fn acquire_skill(&self, params: SkillAcquireParams) -> AppResult<Value> {
        if !params.dry_run && !params.yes {
            return Err("skill acquire requires --yes".to_string());
        }
        let location = parse_github_skill_location(
            &params.url,
            params.branch.as_deref(),
            params.path.as_deref(),
        )?;
        let raw_name = params
            .name
            .clone()
            .or_else(|| location.skill_name_hint())
            .unwrap_or_else(|| location.repo.clone());
        let name = slug_path_segment(&raw_name);
        let staging_dir = commands::skill_backup_root(&self.conn)?
            .join("staging")
            .join(format!("{}-{}", slug_path_segment(&name), short_uuid()));
        let skill_path_hint = location.skill_path_hint(&staging_dir);

        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "provider": "github",
                "url": params.url,
                "repo_url": location.repo_url,
                "branch": location.branch,
                "path": location.path,
                "name": name,
                "staging_path": staging_dir,
                "skill_path": skill_path_hint,
                "security_notice": SKILL_REMOTE_SECURITY_NOTICE,
            }));
        }

        clone_github_skill(&location, &staging_dir)?;
        let skill_dir = resolve_cloned_skill_dir(&staging_dir, location.path.as_deref())?;
        let acquired_tree_sha = git_skill_tree_sha(&staging_dir, location.path.as_deref());
        let acquired_branch = location
            .branch
            .clone()
            .or_else(|| git_current_branch(&staging_dir))
            .unwrap_or_else(|| "HEAD".to_string());
        let import_result = self.import_skill(ImportSkillParams {
            from: skill_dir.to_string_lossy().to_string(),
            name: Some(name.clone()),
            dry_run: false,
        })?;
        let imported_asset = import_result
            .get("asset")
            .cloned()
            .ok_or_else(|| "skill import result did not include asset".to_string())
            .and_then(|value| {
                serde_json::from_value::<Asset>(value)
                    .map_err(|error| format!("skill import result asset was invalid: {error}"))
            })?;
        let remote_source = SkillRemoteSource {
            asset_id: imported_asset.id.clone(),
            provider: "github".to_string(),
            source_url: params.url.clone(),
            repo_url: location.repo_url.clone(),
            branch: acquired_branch.clone(),
            path: location.path.clone(),
            acquired_at: Utc::now().to_rfc3339(),
            acquired_tree_sha,
            local_content_hash: imported_asset.content_hash.clone(),
            last_checked_at: None,
            latest_tree_sha: None,
            status: "unknown".to_string(),
            message: Some(
                "Remote source recorded; run skill remote check to detect drift".to_string(),
            ),
        };
        store::upsert_skill_remote_source(&self.conn, &remote_source)?;
        Ok(json!({
            "dry_run": false,
            "provider": "github",
            "url": params.url,
            "repo_url": location.repo_url,
            "branch": acquired_branch,
            "path": location.path,
            "name": name,
            "staging_path": staging_dir,
            "skill_path": skill_dir,
            "import": import_result,
            "remote_source": remote_source,
            "security_notice": SKILL_REMOTE_SECURITY_NOTICE,
        }))
    }

    pub(crate) fn list_skill_remote_sources(&self) -> AppResult<Vec<SkillRemoteSource>> {
        commands::cleanup_orphan_asset_records(&self.conn)?;
        store::list_skill_remote_sources(&self.conn)
    }

    pub(crate) fn check_skill_remote_sources(
        &self,
        params: SkillRemoteCheckParams,
    ) -> AppResult<Vec<SkillRemoteSource>> {
        commands::cleanup_orphan_asset_records(&self.conn)?;
        let sources = if let Some(asset_id) = params
            .asset_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            vec![store::load_skill_remote_source(&self.conn, asset_id)?
                .ok_or_else(|| format!("skill remote source not found: {asset_id}"))?]
        } else {
            store::list_skill_remote_sources(&self.conn)?
        };

        let mut checked = Vec::with_capacity(sources.len());
        for source in sources {
            let source = check_skill_remote_source(source);
            store::update_skill_remote_check_result(&self.conn, &source)?;
            checked.push(source);
        }
        Ok(checked)
    }

    pub(crate) fn delete_skill(&self, params: AssetRefParams) -> AppResult<Value> {
        if !params.dry_run && !params.yes {
            return Err("skill.delete requires --yes".to_string());
        }
        let asset = self.resolve_skill_asset(&params.asset_ref)?;
        let source = store::load_sources(&self.conn)?
            .into_iter()
            .find(|source| source.id == asset.source_id)
            .ok_or_else(|| format!("source not found: {}", asset.source_id))?;
        if source.source_origin != SourceOrigin::AssetiweaveLibrary {
            return Err(
                "only AssetIWeave backup library skills can be deleted; remove the source or unmount the skill instead"
                    .to_string(),
            );
        }

        let enabled_mounts = store::load_asset_mounts(&self.conn, Some(&asset.id))?
            .into_iter()
            .filter(|mount| mount.enabled)
            .collect::<Vec<_>>();
        if !enabled_mounts.is_empty() && !params.unmount {
            return Err(
                "skill has enabled mounts; pass --unmount to remove managed mounts first"
                    .to_string(),
            );
        }
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "deleted": false,
                "asset": asset,
                "enabled_mounts": enabled_mounts
            }));
        }

        for mount in enabled_mounts {
            commands::unmount_asset_mount_record(&self.conn, &asset.id, &mount.profile_id)?;
        }
        let asset_path = PathBuf::from(&asset.absolute_path);
        if asset_path.exists() {
            let metadata = fs::symlink_metadata(&asset_path).map_err(|error| error.to_string())?;
            if metadata.is_dir() {
                fs::remove_dir_all(&asset_path).map_err(|error| error.to_string())?;
            } else {
                fs::remove_file(&asset_path).map_err(|error| error.to_string())?;
            }
        }
        commands::refresh_recorded_assets(&self.conn)?;
        Ok(json!({ "deleted": true, "asset_id": asset.id }))
    }

    pub(crate) fn mount_skill(&self, params: AssetRefParams, enabled: bool) -> AppResult<Value> {
        let profile_id = params
            .profile_id
            .as_deref()
            .ok_or_else(|| "profile_id is required".to_string())?;
        let asset = self.resolve_skill_asset(&params.asset_ref)?;
        if params.dry_run {
            let profile = store::load_profiles(&self.conn)?
                .into_iter()
                .find(|profile| profile.id == profile_id)
                .ok_or_else(|| format!("profile not found: {profile_id}"))?;
            let inspection = targeting::inspect_mount(&profile, &asset)?;
            return Ok(json!({
                "dry_run": true,
                "asset": asset,
                "profile_id": profile_id,
                "enabled": enabled,
                "status": AssetMountStatus {
                    asset_id: asset.id,
                    profile_id: profile.id,
                    target_dir: inspection.target_dir,
                    target_path: inspection.target_path,
                    state: PhysicalMountStateDto::from(inspection.state),
                    linked_source: inspection.linked_source,
                }
            }));
        }

        let update = if enabled {
            self.mount_asset_by_id(&asset.id, profile_id)?
        } else {
            self.unmount_asset_by_id(&asset.id, profile_id)?
        };
        Ok(json!(update))
    }

    pub(crate) fn list_skill_groups(&self) -> AppResult<Vec<AssetGroupDetail>> {
        commands::cleanup_orphan_asset_records(&self.conn)?;
        let assets = store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))?;
        store::load_skill_group_details(&self.conn, &assets)
    }

    pub(crate) fn get_skill_group(&self, group_id: String) -> AppResult<AssetGroupDetail> {
        commands::cleanup_orphan_asset_records(&self.conn)?;
        let assets = store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))?;
        store::load_skill_group_detail(&self.conn, &group_id, &assets)
    }

    pub(crate) fn create_skill_group(&self, input: AssetGroupInput) -> AppResult<AssetGroupDetail> {
        let assets = store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))?;
        let now = Utc::now().to_rfc3339();
        let group = commands::asset_group_from_input(input, now.clone(), now);
        store::upsert_asset_group(&self.conn, &group)?;
        store::load_skill_group_detail(&self.conn, &group.id, &assets)
    }

    pub(crate) fn update_skill_group(&self, group: AssetGroup) -> AppResult<AssetGroupDetail> {
        let assets = store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))?;
        let mut group = group;
        group.updated_at = Utc::now().to_rfc3339();
        store::upsert_asset_group(&self.conn, &group)?;
        store::load_skill_group_detail(&self.conn, &group.id, &assets)
    }

    pub(crate) fn delete_skill_group(&self, group_id: String) -> AppResult<()> {
        let assets = store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))?;
        store::load_skill_group_detail(&self.conn, &group_id, &assets)?;
        store::delete_asset_group(&self.conn, &group_id)
    }

    pub(crate) fn set_skill_group_manual_members(
        &self,
        group_id: String,
        asset_ids: Vec<String>,
    ) -> AppResult<AssetGroupDetail> {
        let assets = store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))?;
        store::replace_asset_group_members(&self.conn, &group_id, &asset_ids, &assets)?;
        store::load_skill_group_detail(&self.conn, &group_id, &assets)
    }

    pub(crate) fn mount_skill_group(
        &self,
        params: SkillGroupMountParams,
        enabled: bool,
    ) -> AppResult<Value> {
        if !enabled && !params.dry_run && !params.yes {
            return Err("skill.group.unmount requires --yes".to_string());
        }
        if params.dry_run {
            let assets = store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))?;
            let detail = store::load_skill_group_detail(&self.conn, &params.group_id, &assets)?;
            return Ok(json!({
                "dry_run": true,
                "group_id": params.group_id,
                "profile_id": params.profile_id,
                "enabled": enabled,
                "requested_count": detail.members.len()
            }));
        }
        let result = self.apply_skill_group_mount(&params.group_id, &params.profile_id, enabled)?;
        Ok(json!(result))
    }

    pub(crate) fn apply_skill_group_mount(
        &self,
        group_id: &str,
        profile_id: &str,
        enabled: bool,
    ) -> AppResult<ApplyAssetGroupMountResult> {
        commands::apply_skill_group_mount_record(&self.conn, group_id, profile_id, enabled)
    }

    pub(crate) fn preview_skill_group_exclusive_mount(
        &self,
        input: SkillGroupExclusiveMountInput,
    ) -> AppResult<SkillGroupExclusiveMountPreview> {
        commands::build_skill_group_exclusive_mount_preview(&self.conn, &input)
    }

    pub(crate) fn apply_skill_group_exclusive_mount(
        &self,
        input: SkillGroupExclusiveMountInput,
    ) -> AppResult<ApplySkillGroupExclusiveMountResult> {
        commands::apply_skill_group_exclusive_mount_record(&self.conn, &input)
    }

    pub(crate) fn mount_asset_by_id(
        &self,
        asset_id: &str,
        profile_id: &str,
    ) -> AppResult<AssetMountUpdateResult> {
        commands::mount_asset_mount_record(&self.conn, asset_id, profile_id)
    }

    pub(crate) fn unmount_asset_by_id(
        &self,
        asset_id: &str,
        profile_id: &str,
    ) -> AppResult<AssetMountUpdateResult> {
        commands::unmount_asset_mount_record(&self.conn, asset_id, profile_id)
    }

    pub(crate) fn toggle_asset_mount(
        &self,
        asset_id: &str,
        profile_id: &str,
    ) -> AppResult<AssetMount> {
        let (asset, profile) = load_mount_asset_and_profile(&self.conn, asset_id, profile_id)?;
        let inspection = targeting::inspect_mount(&profile, &asset)?;
        commands::set_asset_mount_record(
            &self.conn,
            asset_id,
            profile_id,
            !matches!(inspection.state, targeting::PhysicalMountState::Mounted),
            None,
        )
    }

    pub(crate) fn set_asset_mount(
        &self,
        asset_id: &str,
        profile_id: &str,
        enabled: bool,
        strategy: Option<DeploymentStrategy>,
    ) -> AppResult<AssetMount> {
        commands::set_asset_mount_record(&self.conn, asset_id, profile_id, enabled, strategy)
    }

    pub(crate) fn execute_plan(
        &self,
        plan: DeploymentPlan,
        action_ids: Option<Vec<String>>,
    ) -> AppResult<ExecutionResult> {
        let profiles = store::load_profiles(&self.conn)?;
        let assets = store::load_assets(&self.conn)?;
        executor::execute_deployment_plan(
            &self.conn,
            &profiles,
            &assets,
            &plan,
            action_ids.as_deref(),
        )
    }

    pub(crate) fn logs_get_snapshot(
        &self,
        file_name: Option<String>,
        line_limit: Option<usize>,
    ) -> AppResult<logs::LogSnapshot> {
        logs::logs_get_snapshot(file_name, line_limit)
    }

    pub(crate) fn logs_open_log_directory(&self) -> AppResult<()> {
        logs::logs_open_log_directory()
    }

    pub(crate) fn logs_write_operation(
        &self,
        level: String,
        operation: String,
        message: String,
        fields: Option<BTreeMap<String, String>>,
    ) -> AppResult<()> {
        logs::logs_write_operation(level, operation, message, fields)
    }

    pub(crate) fn reveal_path(&self, path: String) -> AppResult<()> {
        platform::reveal_path(path)
    }

    pub(crate) fn get_app_settings(&self) -> AppResult<app_settings::AppSettingsFile> {
        app_settings::get_app_settings()
    }

    pub(crate) fn save_app_settings(
        &self,
        settings: Value,
    ) -> AppResult<app_settings::AppSettingsFile> {
        app_settings::save_app_settings(settings)
    }

    pub(crate) fn run_doctor(&self) -> AppResult<Value> {
        let backup_root = commands::skill_backup_root(&self.conn)?;
        Ok(json!({
            "checks": [
                { "name": "database", "status": "pass", "message": self.db_path.to_string_lossy() },
                {
                    "name": "skill_backup_root",
                    "status": if backup_root.exists() { "pass" } else { "fail" },
                    "message": backup_root.to_string_lossy()
                },
                {
                    "name": "sources",
                    "status": "pass",
                    "message": format!("{} sources", store::count_rows(&self.conn, "sources")?)
                }
            ]
        }))
    }

    fn resolve_skill_asset(&self, asset_ref: &str) -> AppResult<Asset> {
        let needle = asset_ref.trim();
        if needle.is_empty() {
            return Err("asset ref is required".to_string());
        }
        let matches = store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))?
            .into_iter()
            .filter(|asset| asset.id == needle || asset.name == needle)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [asset] => Ok(asset.clone()),
            [] => Err(format!("skill not found: {needle}")),
            many => Err(format!(
                "ambiguous skill ref {needle}: {}",
                many.iter()
                    .map(|asset| format!("{} ({})", asset.name, asset.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

fn is_protected_source(source: &Source) -> bool {
    source.id == "assetiweave-library-skills"
        || matches!(source.source_origin, SourceOrigin::AssetiweaveLibrary)
}

fn path_contains(parent: &Path, child: &Path) -> bool {
    let normalized_parent = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    let normalized_child = child.canonicalize().unwrap_or_else(|_| child.to_path_buf());
    normalized_child.starts_with(&normalized_parent)
}

#[derive(Debug)]
struct GitHubSkillLocation {
    repo: String,
    repo_url: String,
    branch: Option<String>,
    path: Option<String>,
}

impl GitHubSkillLocation {
    fn skill_name_hint(&self) -> Option<String> {
        self.path
            .as_deref()
            .and_then(|path| path.split('/').next_back())
            .filter(|name| !name.is_empty())
            .map(str::to_string)
    }

    fn skill_path_hint(&self, staging_dir: &Path) -> PathBuf {
        self.path
            .as_deref()
            .map(|path| staging_dir.join(path))
            .unwrap_or_else(|| staging_dir.to_path_buf())
    }
}

fn normalize_skill_search_provider(provider: Option<&str>) -> AppResult<String> {
    match provider
        .and_then(clean_non_empty_string)
        .unwrap_or_else(|| "github".to_string())
        .as_str()
    {
        "github" => Ok("github".to_string()),
        "github-code" | "github_code" | "code" => Ok("github-code".to_string()),
        other => Err(format!("unsupported skill search provider: {other}")),
    }
}

fn github_repository_skill_search(
    query: &str,
    limit: usize,
) -> AppResult<(Vec<SkillSearchCandidate>, Vec<String>)> {
    let repository_limit = limit.clamp(5, 10);
    let url = format!(
        "https://api.github.com/search/repositories?q={}&per_page={}",
        percent_encode_query(&format!("{query} skill")),
        repository_limit
    );
    let value = github_get_json(&url, "skill search")?;
    let mut candidates = Vec::new();
    let mut warnings = Vec::new();
    if let Some(items) = value.get("items").and_then(Value::as_array) {
        for item in items.iter().take(repository_limit) {
            if candidates.len() >= limit {
                break;
            }
            let Some(repo_candidate) = skill_search_candidate_from_github(item) else {
                continue;
            };
            let full_name = item.get("full_name").and_then(Value::as_str);
            let branch = repo_candidate
                .default_branch
                .as_deref()
                .unwrap_or("main")
                .to_string();
            let skill_candidates = match full_name {
                Some(full_name) => {
                    match github_skill_candidates_for_repo(full_name, &branch, &repo_candidate) {
                        Ok(candidates) => candidates,
                        Err(error) => {
                            warnings.push(format!(
                                "{full_name}: could not inspect GitHub tree on {branch}: {error}"
                            ));
                            Vec::new()
                        }
                    }
                }
                None => {
                    warnings.push(format!(
                        "{}: GitHub search result did not include full_name",
                        repo_candidate.name
                    ));
                    Vec::new()
                }
            };

            if skill_candidates.is_empty() {
                candidates.push(skill_search_repository_fallback_candidate(
                    repo_candidate,
                    &branch,
                ));
                continue;
            }
            candidates.extend(skill_candidates);
        }
    } else {
        warnings.push("GitHub search response did not include repository items".to_string());
    }
    Ok((candidates, warnings))
}

fn github_code_skill_search(
    query: &str,
    limit: usize,
) -> AppResult<(Vec<SkillSearchCandidate>, Vec<String>)> {
    let url = github_code_search_url(query, limit);
    let value = github_get_json(&url, "GitHub code skill search")?;
    let mut candidates = Vec::new();
    let mut warnings = Vec::new();
    if let Some(items) = value.get("items").and_then(Value::as_array) {
        for item in items.iter().take(limit) {
            match skill_search_candidate_from_github_code(item) {
                Some(candidate) => candidates.push(candidate),
                None => warnings
                    .push("GitHub code search returned an incomplete SKILL.md item".to_string()),
            }
        }
    } else {
        warnings.push("GitHub code search response did not include code items".to_string());
    }
    Ok((candidates, warnings))
}

fn github_code_search_url(query: &str, limit: usize) -> String {
    format!(
        "https://api.github.com/search/code?q={}&per_page={}",
        percent_encode_query(&format!("{query} filename:SKILL.md")),
        limit.clamp(1, 20)
    )
}

fn skill_search_candidate_from_github(item: &Value) -> Option<SkillSearchCandidate> {
    let url = item.get("html_url")?.as_str()?.to_string();
    let name = item
        .get("full_name")
        .and_then(Value::as_str)
        .or_else(|| item.get("name").and_then(Value::as_str))?
        .to_string();
    Some(SkillSearchCandidate {
        acquire_command: format!("assetiweave-cli skill acquire --url {url} --yes"),
        name,
        description: item
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        match_reason: None,
        url,
        path: None,
        clone_url: item
            .get("clone_url")
            .and_then(Value::as_str)
            .map(str::to_string),
        default_branch: item
            .get("default_branch")
            .and_then(Value::as_str)
            .map(str::to_string),
        stars: item.get("stargazers_count").and_then(Value::as_u64),
    })
}

fn skill_search_candidate_from_github_code(item: &Value) -> Option<SkillSearchCandidate> {
    let repository = item.get("repository")?;
    let full_name = repository
        .get("full_name")
        .and_then(Value::as_str)
        .or_else(|| repository.get("name").and_then(Value::as_str))?;
    let repo_url = repository.get("html_url")?.as_str()?;
    let skill_file_path = item.get("path")?.as_str()?.trim().trim_matches('/');
    if !skill_file_path.ends_with("SKILL.md") {
        return None;
    }
    let skill_path = clean_skill_subpath(skill_file_path);
    let branch = repository
        .get("default_branch")
        .and_then(Value::as_str)
        .unwrap_or("main");
    let url = github_skill_tree_url(repo_url, branch, skill_path.as_deref().unwrap_or_default());
    let name = skill_path
        .as_deref()
        .map(|path| format!("{full_name}/{path}"))
        .unwrap_or_else(|| full_name.to_string());
    Some(SkillSearchCandidate {
        acquire_command: format!("assetiweave-cli skill acquire --url {url} --yes"),
        name,
        description: repository
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        match_reason: Some(format!("GitHub code search matched {skill_file_path}")),
        url,
        path: skill_path,
        clone_url: repository
            .get("clone_url")
            .and_then(Value::as_str)
            .map(str::to_string),
        default_branch: Some(branch.to_string()),
        stars: repository.get("stargazers_count").and_then(Value::as_u64),
    })
}

fn skill_search_repository_fallback_candidate(
    mut candidate: SkillSearchCandidate,
    branch: &str,
) -> SkillSearchCandidate {
    candidate.match_reason = Some(format!(
        "Repository fallback: no concrete SKILL.md directory was resolved on branch {branch}"
    ));
    candidate
}

fn github_skill_candidates_for_repo(
    full_name: &str,
    branch: &str,
    repo_candidate: &SkillSearchCandidate,
) -> AppResult<Vec<SkillSearchCandidate>> {
    let url = format!(
        "https://api.github.com/repos/{}/git/trees/{}?recursive=1",
        full_name,
        percent_encode_path_segment(branch)
    );
    let value = github_get_json(&url, "GitHub skill tree")?;
    let mut candidates = github_skill_paths_from_tree_value(&value)
        .into_iter()
        .map(|path| {
            skill_search_candidate_from_github_skill_path(repo_candidate, full_name, branch, &path)
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(candidates)
}

fn github_skill_paths_from_tree_value(value: &Value) -> Vec<String> {
    let mut paths = BTreeSet::new();
    let Some(tree) = value.get("tree").and_then(Value::as_array) else {
        return Vec::new();
    };
    for entry in tree {
        if entry.get("type").and_then(Value::as_str) != Some("blob") {
            continue;
        }
        let Some(path) = entry.get("path").and_then(Value::as_str) else {
            continue;
        };
        let normalized_path = path.trim().trim_matches('/');
        if normalized_path == "SKILL.md" {
            paths.insert(String::new());
            continue;
        }
        let Some(skill_dir) = normalized_path.strip_suffix("/SKILL.md") else {
            continue;
        };
        if let Some(cleaned) = clean_skill_subpath(skill_dir) {
            paths.insert(cleaned);
        }
    }
    paths.into_iter().collect()
}

fn github_tree_sha_for_skill_path(value: &Value, path: Option<&str>) -> AppResult<String> {
    let Some(path) = path.and_then(clean_skill_subpath) else {
        return value
            .get("sha")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "GitHub tree response did not include root sha".to_string());
    };
    let Some(tree) = value.get("tree").and_then(Value::as_array) else {
        return Err("GitHub tree response did not include tree entries".to_string());
    };
    tree.iter()
        .find(|entry| {
            entry.get("type").and_then(Value::as_str) == Some("tree")
                && entry.get("path").and_then(Value::as_str) == Some(path.as_str())
        })
        .and_then(|entry| entry.get("sha").and_then(Value::as_str))
        .map(str::to_string)
        .ok_or_else(|| format!("GitHub tree response did not include Skill path: {path}"))
}

fn skill_search_candidate_from_github_skill_path(
    repo_candidate: &SkillSearchCandidate,
    full_name: &str,
    branch: &str,
    path: &str,
) -> SkillSearchCandidate {
    let url = github_skill_tree_url(&repo_candidate.url, branch, path);
    let path = clean_skill_subpath(path);
    let skill_file = path
        .as_deref()
        .map(|path| format!("{path}/SKILL.md"))
        .unwrap_or_else(|| "SKILL.md".to_string());
    let name = path
        .as_deref()
        .map(|path| format!("{full_name}/{path}"))
        .unwrap_or_else(|| full_name.to_string());
    SkillSearchCandidate {
        acquire_command: format!("assetiweave-cli skill acquire --url {url} --yes"),
        name,
        description: repo_candidate.description.clone(),
        match_reason: Some(format!(
            "Resolved concrete Skill directory from {skill_file}"
        )),
        url,
        path,
        clone_url: repo_candidate.clone_url.clone(),
        default_branch: Some(branch.to_string()),
        stars: repo_candidate.stars,
    }
}

fn github_skill_tree_url(repo_url: &str, branch: &str, path: &str) -> String {
    let base = repo_url.trim_end_matches('/');
    if path.trim().is_empty() {
        format!("{base}/tree/{branch}")
    } else {
        format!("{base}/tree/{branch}/{}", path.trim().trim_matches('/'))
    }
}

fn github_get_json(url: &str, context: &str) -> AppResult<Value> {
    let mut request = ureq::get(url)
        .set("User-Agent", "AssetIWeave/0.1 skill-search")
        .set("Accept", "application/vnd.github+json");
    let authorization = github_api_token().map(|token| format!("Bearer {token}"));
    if let Some(authorization) = authorization.as_deref() {
        request = request.set("Authorization", authorization);
    }
    let response = request
        .call()
        .map_err(|error| format!("{context} request failed: {error}"))?;
    response
        .into_json()
        .map_err(|error| format!("{context} response was not JSON: {error}"))
}

fn check_skill_remote_source(mut source: SkillRemoteSource) -> SkillRemoteSource {
    source.last_checked_at = Some(Utc::now().to_rfc3339());
    if source.provider != "github" {
        source.status = "error".to_string();
        source.message = Some(format!(
            "unsupported Skill remote provider: {}",
            source.provider
        ));
        return source;
    }

    let Some(full_name) = github_full_name_from_repo_url(&source.repo_url) else {
        source.status = "error".to_string();
        source.message = Some(format!(
            "unsupported GitHub repository URL: {}",
            source.repo_url
        ));
        return source;
    };
    let url = format!(
        "https://api.github.com/repos/{}/git/trees/{}?recursive=1",
        full_name,
        percent_encode_path_segment(&source.branch)
    );
    match github_get_json(&url, "GitHub skill drift check")
        .and_then(|value| github_tree_sha_for_skill_path(&value, source.path.as_deref()))
    {
        Ok(latest_tree_sha) => {
            source.latest_tree_sha = Some(latest_tree_sha.clone());
            match source.acquired_tree_sha.as_deref() {
                Some(acquired_tree_sha) if acquired_tree_sha == latest_tree_sha => {
                    source.status = "current".to_string();
                    source.message = Some("Remote Skill matches acquired tree".to_string());
                }
                Some(_) => {
                    source.status = "changed".to_string();
                    source.message = Some("Remote Skill changed since acquisition".to_string());
                }
                None => {
                    source.status = "unknown".to_string();
                    source.message =
                        Some("Remote Skill was acquired before tree SHA tracking".to_string());
                }
            }
        }
        Err(error) => {
            source.status = "error".to_string();
            source.message = Some(error);
        }
    }
    source
}

fn github_full_name_from_repo_url(repo_url: &str) -> Option<String> {
    let path = repo_url
        .trim()
        .trim_end_matches('/')
        .strip_prefix("https://github.com/")?
        .trim_end_matches(".git");
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        Some(format!("{}/{}", parts[0], parts[1]))
    } else {
        None
    }
}

fn github_api_token() -> Option<String> {
    env::var("GITHUB_TOKEN")
        .or_else(|_| env::var("GH_TOKEN"))
        .ok()
        .and_then(|token| clean_non_empty_string(&token))
}

fn search_query_terms(query: &str) -> Vec<String> {
    let terms = query
        .split(|character: char| !character.is_alphanumeric())
        .filter_map(clean_non_empty_string)
        .map(|term| term.to_lowercase())
        .collect::<Vec<_>>();
    if terms.is_empty() {
        let fallback = query.trim().to_lowercase();
        if fallback.is_empty() {
            Vec::new()
        } else {
            vec![fallback]
        }
    } else {
        terms
    }
}

fn skill_candidate_score(candidate: &SkillSearchCandidate, terms: &[String]) -> usize {
    let haystack = format!(
        "{} {} {} {}",
        candidate.name,
        candidate.path.as_deref().unwrap_or_default(),
        candidate.description.as_deref().unwrap_or_default(),
        candidate.url
    )
    .to_lowercase();
    let term_score = terms
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .count()
        * 100;
    let concrete_skill_score = usize::from(candidate.path.is_some()) * 10;
    term_score + concrete_skill_score
}

fn percent_encode_query(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char);
            }
            b' ' => encoded.push('+'),
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

fn percent_encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char);
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

fn parse_github_skill_location(
    url: &str,
    branch_override: Option<&str>,
    path_override: Option<&str>,
) -> AppResult<GitHubSkillLocation> {
    let trimmed = url
        .trim()
        .split('#')
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    let path = trimmed
        .strip_prefix("https://github.com/")
        .ok_or_else(|| "skill acquire only supports https://github.com URLs".to_string())?;
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err("GitHub URL must include owner and repository".to_string());
    }

    let owner = parts[0];
    let repo = parts[1].trim_end_matches(".git");
    if repo.is_empty() {
        return Err("GitHub URL must include repository name".to_string());
    }

    let mut branch = branch_override.and_then(clean_non_empty_string);
    let mut skill_path = path_override.and_then(clean_skill_subpath);
    if skill_path.is_none() && parts.len() >= 4 && matches!(parts[2], "tree" | "blob") {
        branch = branch.or_else(|| clean_non_empty_string(parts[3]));
        if parts.len() > 4 {
            skill_path = clean_skill_subpath(&parts[4..].join("/"));
        }
    }

    Ok(GitHubSkillLocation {
        repo: repo.to_string(),
        repo_url: format!("https://github.com/{owner}/{repo}.git"),
        branch,
        path: skill_path,
    })
}

fn clean_non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn clean_skill_subpath(value: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in value.trim().trim_matches('/').split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part == ".git" || part.contains('\\') || part.contains(':') {
            return None;
        }
        parts.push(part);
    }
    if matches!(parts.last().copied(), Some("SKILL.md")) {
        parts.pop();
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn clone_github_skill(location: &GitHubSkillLocation, target: &Path) -> AppResult<()> {
    if target.exists() {
        return Err(format!(
            "skill acquire staging path already exists: {}",
            target.display()
        ));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let mut command = Command::new("git");
    command.arg("clone").arg("--depth").arg("1");
    if let Some(branch) = &location.branch {
        command.arg("--branch").arg(branch);
    }
    let output = command
        .arg(&location.repo_url)
        .arg(target)
        .output()
        .map_err(|error| format!("failed to run git clone: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git clone failed: {stderr}"));
    }
    Ok(())
}

fn git_current_branch(repo: &Path) -> Option<String> {
    git_output(repo, &["rev-parse", "--abbrev-ref", "HEAD"]).filter(|branch| branch != "HEAD")
}

fn git_skill_tree_sha(repo: &Path, skill_path: Option<&str>) -> Option<String> {
    let revision = skill_path
        .and_then(clean_skill_subpath)
        .map(|path| format!("HEAD:{path}"))
        .unwrap_or_else(|| "HEAD^{tree}".to_string());
    git_output(repo, &["rev-parse", &revision])
}

fn git_output(repo: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn resolve_cloned_skill_dir(staging_dir: &Path, skill_path: Option<&str>) -> AppResult<PathBuf> {
    if let Some(skill_path) = skill_path {
        let candidate = staging_dir.join(skill_path);
        if candidate.join("SKILL.md").is_file() {
            return Ok(candidate);
        }
        return Err(format!(
            "cloned path does not contain SKILL.md: {}",
            candidate.display()
        ));
    }
    if staging_dir.join("SKILL.md").is_file() {
        return Ok(staging_dir.to_path_buf());
    }

    let mut candidates = Vec::new();
    for entry in walkdir::WalkDir::new(staging_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        if entry.file_name().to_str() == Some("SKILL.md") {
            if let Some(parent) = entry.path().parent() {
                candidates.push(parent.to_path_buf());
            }
        }
    }
    match candidates.as_slice() {
        [candidate] => Ok(candidate.clone()),
        [] => Err("cloned repository does not contain SKILL.md".to_string()),
        many => Err(format!(
            "cloned repository contains multiple skills; pass --path: {}",
            many.iter()
                .filter_map(|path| path.strip_prefix(staging_dir).ok())
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn short_uuid() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}

fn slug_path_segment(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;
    for character in value.trim().chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            last_was_separator = false;
        } else if !last_was_separator && !slug.is_empty() {
            slug.push('-');
            last_was_separator = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "source".to_string()
    } else {
        slug
    }
}

fn sanitize_path_segment(value: &str) -> String {
    let mut segment = String::new();
    let mut last_was_separator = false;
    for character in value.trim().chars() {
        if character.is_alphanumeric() || matches!(character, '_' | '.') {
            segment.push(character);
            last_was_separator = false;
        } else if !last_was_separator && !segment.is_empty() {
            segment.push('-');
            last_was_separator = true;
        }
        if segment.chars().count() >= 96 {
            break;
        }
    }
    let segment = segment.trim_matches(['-', '.']).to_string();
    if segment.is_empty() {
        "untitled".to_string()
    } else {
        segment
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn github_repo_item() -> Value {
        json!({
            "full_name": "util6/util6-agents",
            "html_url": "https://github.com/util6/util6-agents",
            "clone_url": "https://github.com/util6/util6-agents.git",
            "default_branch": "main",
            "description": "Codex skills and agent workflows",
            "stargazers_count": 7
        })
    }

    fn github_code_item() -> Value {
        json!({
            "path": "skills/browser/SKILL.md",
            "repository": {
                "full_name": "util6/util6-agents",
                "html_url": "https://github.com/util6/util6-agents",
                "clone_url": "https://github.com/util6/util6-agents.git",
                "default_branch": "main",
                "description": "Codex skills and agent workflows",
                "stargazers_count": 7
            }
        })
    }

    #[test]
    fn skill_search_provider_supports_github_code_aliases() {
        assert_eq!(
            normalize_skill_search_provider(None).as_deref(),
            Ok("github")
        );
        assert_eq!(
            normalize_skill_search_provider(Some("github_code")).as_deref(),
            Ok("github-code")
        );
        assert_eq!(
            normalize_skill_search_provider(Some("code")).as_deref(),
            Ok("github-code")
        );
        assert!(normalize_skill_search_provider(Some("unknown")).is_err());
    }

    #[test]
    fn github_code_search_url_targets_skill_markdown_files() {
        let url = github_code_search_url("browser automation", 25);

        assert_eq!(
            url,
            "https://api.github.com/search/code?q=browser+automation+filename%3ASKILL.md&per_page=20"
        );
    }

    #[test]
    fn github_tree_paths_extract_concrete_skill_dirs() {
        let value = json!({
            "tree": [
                { "path": "SKILL.md", "type": "blob" },
                { "path": "skills/browser/SKILL.md", "type": "blob" },
                { "path": "skills/browser/README.md", "type": "blob" },
                { "path": "skills/../escape/SKILL.md", "type": "blob" },
                { "path": "plugins/browser/SKILL.md", "type": "tree" }
            ]
        });

        let paths = github_skill_paths_from_tree_value(&value);

        assert_eq!(paths, vec!["".to_string(), "skills/browser".to_string()]);
    }

    #[test]
    fn github_tree_sha_for_skill_path_reads_root_and_nested_tree() {
        let value = json!({
            "sha": "root-tree",
            "tree": [
                { "path": "skills/browser", "type": "tree", "sha": "browser-tree" },
                { "path": "skills/browser/SKILL.md", "type": "blob", "sha": "skill-file" }
            ]
        });

        assert_eq!(
            github_tree_sha_for_skill_path(&value, None).as_deref(),
            Ok("root-tree")
        );
        assert_eq!(
            github_tree_sha_for_skill_path(&value, Some("skills/browser")).as_deref(),
            Ok("browser-tree")
        );
        assert!(github_tree_sha_for_skill_path(&value, Some("missing")).is_err());
    }

    #[test]
    fn github_skill_path_candidate_points_acquire_at_tree_url() {
        let repo_candidate = skill_search_candidate_from_github(&github_repo_item()).unwrap();

        let candidate = skill_search_candidate_from_github_skill_path(
            &repo_candidate,
            "util6/util6-agents",
            "main",
            "skills/browser",
        );

        assert_eq!(candidate.name, "util6/util6-agents/skills/browser");
        assert_eq!(candidate.path.as_deref(), Some("skills/browser"));
        assert_eq!(
            candidate.match_reason.as_deref(),
            Some("Resolved concrete Skill directory from skills/browser/SKILL.md")
        );
        assert_eq!(
            candidate.url,
            "https://github.com/util6/util6-agents/tree/main/skills/browser"
        );
        assert_eq!(
            candidate.acquire_command,
            "assetiweave-cli skill acquire --url https://github.com/util6/util6-agents/tree/main/skills/browser --yes"
        );
    }

    #[test]
    fn github_code_candidate_points_acquire_at_skill_directory() {
        let candidate = skill_search_candidate_from_github_code(&github_code_item())
            .expect("github code item should become candidate");

        assert_eq!(candidate.name, "util6/util6-agents/skills/browser");
        assert_eq!(candidate.path.as_deref(), Some("skills/browser"));
        assert_eq!(
            candidate.match_reason.as_deref(),
            Some("GitHub code search matched skills/browser/SKILL.md")
        );
        assert_eq!(
            candidate.url,
            "https://github.com/util6/util6-agents/tree/main/skills/browser"
        );
        assert_eq!(
            candidate.acquire_command,
            "assetiweave-cli skill acquire --url https://github.com/util6/util6-agents/tree/main/skills/browser --yes"
        );
    }

    #[test]
    fn root_skill_path_candidate_keeps_repo_name() {
        let repo_candidate = skill_search_candidate_from_github(&github_repo_item()).unwrap();

        let candidate = skill_search_candidate_from_github_skill_path(
            &repo_candidate,
            "util6/util6-agents",
            "main",
            "",
        );

        assert_eq!(candidate.name, "util6/util6-agents");
        assert_eq!(candidate.path, None);
        assert_eq!(
            candidate.match_reason.as_deref(),
            Some("Resolved concrete Skill directory from SKILL.md")
        );
        assert_eq!(
            candidate.url,
            "https://github.com/util6/util6-agents/tree/main"
        );
    }

    #[test]
    fn repository_fallback_candidate_explains_missing_skill_path() {
        let repo_candidate = skill_search_candidate_from_github(&github_repo_item()).unwrap();

        let candidate = skill_search_repository_fallback_candidate(repo_candidate, "main");

        assert_eq!(
            candidate.match_reason.as_deref(),
            Some("Repository fallback: no concrete SKILL.md directory was resolved on branch main")
        );
        assert_eq!(
            candidate.acquire_command,
            "assetiweave-cli skill acquire --url https://github.com/util6/util6-agents --yes"
        );
    }

    #[test]
    fn concrete_skill_candidate_scores_above_repo_fallback() {
        let repo_candidate = skill_search_candidate_from_github(&github_repo_item()).unwrap();
        let skill_candidate = skill_search_candidate_from_github_skill_path(
            &repo_candidate,
            "util6/util6-agents",
            "main",
            "skills/browser",
        );
        let terms = search_query_terms("browser skill");

        assert!(
            skill_candidate_score(&skill_candidate, &terms)
                > skill_candidate_score(&repo_candidate, &terms)
        );
    }
}

fn engine_db_path() -> AppResult<PathBuf> {
    if let Ok(path) = env::var("ASSETIWEAVE_DB_PATH") {
        if !path.trim().is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    path_utils::app_db_path()
}

fn load_mount_asset_and_profile(
    conn: &Connection,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<(Asset, TargetProfile)> {
    let asset = store::load_assets(conn)?
        .into_iter()
        .find(|asset| asset.id == asset_id)
        .ok_or_else(|| format!("asset not found: {asset_id}"))?;
    let profile = store::load_profiles(conn)?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("profile not found: {profile_id}"))?;

    Ok((asset, profile))
}

fn source_from_input(source: SourceInput) -> Source {
    let source = Source {
        id: source.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: source.name,
        kind: source.kind,
        root_path: source.root_path,
        scanner_kind: source.scanner_kind.unwrap_or(SourceScannerKind::Mixed),
        source_origin: source.source_origin.unwrap_or(SourceOrigin::LocalFolder),
        repo_root: source.repo_root,
        scan_root: source.scan_root.unwrap_or_default(),
        origin_app_kind: source.origin_app_kind,
        include_globs: source.include_globs,
        exclude_globs: source.exclude_globs,
        default_kind: source.default_kind,
        enabled: source.enabled,
        priority: source.priority,
        last_scanned_at: None,
        last_scan_status: Some("pending".to_string()),
    };
    store::normalize_source(&source)
}
