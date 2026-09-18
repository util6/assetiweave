use super::*;
use crate::backend::ai_execution::{executor::BackendFuture, AiExecutionResult};
use crate::backend::card_translation::{
    ConversationTranslationCli, ConversationTranslationProvider,
};
use crate::backend::dto::{PhysicalMountStateDto, SkillBackupState};
use crate::backend::models::{
    AppKind, AssetFormat, AssetGroup, AssetGroupRules, AssetKind, DeploymentStrategy,
    ProfileSafety, RuleSet, SourceKind, SourceOrigin, SourceScannerKind,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Default)]
struct RecordingAiTaskEmitter {
    snapshots: Mutex<Vec<AiExecutionTaskSnapshot>>,
}

impl AiExecutionTaskEmitter for RecordingAiTaskEmitter {
    fn emit(&self, snapshot: &AiExecutionTaskSnapshot) {
        self.snapshots.lock().unwrap().push(snapshot.clone());
    }
}

struct AdapterFakeRuntime {
    cleaned: Arc<AtomicBool>,
}

impl AgentExecutionRuntime for AdapterFakeRuntime {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
        Box::pin(async move {
            request.report_phase(AiExecutionPhase::Resolving);
            request.report_phase(AiExecutionPhase::Spawning);
            request.report_phase(AiExecutionPhase::Prompting);
            self.cleaned.store(true, Ordering::SeqCst);
            Ok(AiExecutionResult {
                text: "adapter result".to_string(),
                agent_id: request.agent_id,
                protocol: crate::backend::agents::types::AgentProtocol::Acp,
                requested_model: request.model,
                elapsed_ms: 1,
                persistent_binding: None,
                replay_text: None,
                session_cleanup: crate::backend::ai_execution::SessionCleanupStatus::Deleted,
            })
        })
    }
}

struct FailingAdapterRuntime;

impl AgentExecutionRuntime for FailingAdapterRuntime {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
        Box::pin(async move {
            request.report_phase(AiExecutionPhase::Prompting);
            request.report_phase(AiExecutionPhase::Closing);
            request.report_phase(AiExecutionPhase::CleaningUp);
            request.report_cleanup(AiExecutionCleanupReport {
                process_reaped: false,
                workspace_removed: true,
                failure_count: 1,
                session_closed: Some(false),
                session_deleted: Some(false),
                session_delete_method: None,
            });
            Err(AiExecutionError::Protocol {
                operation: "adapter_failure",
            })
        })
    }
}

#[tokio::test(flavor = "current_thread")]
async fn tauri_01_02_start_preparation_is_fast_and_has_no_global_lock_dependency() {
    let tasks = Arc::new(BackgroundTaskRegistry::default());
    let emitter = Arc::new(RecordingAiTaskEmitter::default());
    let started = Instant::now();

    let (snapshot, request) = prepare_ai_execution_task(
        tasks.clone(),
        opencode_translation_request(),
        emitter.clone(),
    )
    .unwrap();

    assert!(started.elapsed() < Duration::from_millis(100));
    assert_eq!(
        snapshot.state,
        crate::adapters::tauri::background_tasks::AiExecutionTaskState::Queued
    );
    assert_eq!(request.prompt, "translate this");
    assert_eq!(emitter.snapshots.lock().unwrap().as_slice(), [snapshot]);
}

#[tokio::test(flavor = "current_thread")]
async fn tauri_03_04_phase_and_terminal_events_are_full_snapshots_after_runtime_cleanup() {
    let tasks = Arc::new(BackgroundTaskRegistry::default());
    let emitter = Arc::new(RecordingAiTaskEmitter::default());
    let cleaned = Arc::new(AtomicBool::new(false));
    let runtime: Arc<dyn AgentExecutionRuntime> = Arc::new(AdapterFakeRuntime {
        cleaned: cleaned.clone(),
    });
    let (queued, request) = prepare_ai_execution_task(
        tasks.clone(),
        opencode_translation_request(),
        emitter.clone(),
    )
    .unwrap();

    run_ai_execution_task(
        tasks.clone(),
        runtime,
        queued.id.clone(),
        request,
        emitter.clone(),
    )
    .await;

    assert!(cleaned.load(Ordering::SeqCst));
    let events = emitter.snapshots.lock().unwrap();
    assert_eq!(events.first().unwrap().phase, AiExecutionPhase::Queued);
    assert!(events
        .iter()
        .any(|snapshot| snapshot.phase == AiExecutionPhase::Resolving));
    assert!(events
        .iter()
        .any(|snapshot| snapshot.phase == AiExecutionPhase::Prompting));
    let terminal = events.last().unwrap();
    assert_eq!(
        terminal.state,
        crate::adapters::tauri::background_tasks::AiExecutionTaskState::Succeeded
    );
    assert_eq!(terminal.result.as_ref().unwrap().text, "adapter result");
    let serialized = serde_json::to_value(terminal).unwrap();
    for field in [
        "id",
        "purpose",
        "agent_id",
        "state",
        "phase",
        "created_at",
        "updated_at",
        "finished_at",
        "result",
        "error",
    ] {
        assert!(serialized.get(field).is_some(), "missing field {field}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn tauri_03_04_failure_keeps_execution_phase_separate_from_cleanup_phase() {
    let tasks = Arc::new(BackgroundTaskRegistry::default());
    let emitter = Arc::new(RecordingAiTaskEmitter::default());
    let runtime: Arc<dyn AgentExecutionRuntime> = Arc::new(FailingAdapterRuntime);
    let (queued, request) = prepare_ai_execution_task(
        tasks.clone(),
        opencode_translation_request(),
        emitter.clone(),
    )
    .unwrap();

    run_ai_execution_task(tasks.clone(), runtime, queued.id.clone(), request, emitter).await;

    let failed = tasks.ai_execution_snapshot(&queued.id).unwrap().unwrap();
    assert_eq!(failed.phase, AiExecutionPhase::CleaningUp);
    assert_eq!(
        failed.error.as_ref().and_then(|error| error.phase),
        Some(AiExecutionPhase::Prompting)
    );
    assert_eq!(
        failed.cleanup,
        Some(AiExecutionCleanupReport {
            process_reaped: false,
            workspace_removed: true,
            failure_count: 1,
            session_closed: Some(false),
            session_deleted: Some(false),
            session_delete_method: None,
        })
    );
}

#[test]
fn tauri_05_06_get_list_and_cancel_use_the_central_registry_token() {
    let tasks = Arc::new(BackgroundTaskRegistry::default());
    let emitter = Arc::new(RecordingAiTaskEmitter::default());
    let (queued, request) =
        prepare_ai_execution_task(tasks.clone(), opencode_translation_request(), emitter).unwrap();
    let cancellation = request.cancellation.clone();

    let cancelling = tasks.cancel_ai_execution(&queued.id).unwrap();

    assert!(cancellation.is_cancelled());
    assert_eq!(cancelling.phase, AiExecutionPhase::Cancelling);
    assert_eq!(
        tasks.ai_execution_snapshot(&queued.id).unwrap(),
        Some(cancelling.clone())
    );
    assert_eq!(tasks.ai_execution_snapshots().unwrap(), [cancelling]);
}

fn opencode_translation_request() -> ConversationTranslationRequest {
    ConversationTranslationRequest {
        agent_id: None,
        provider: ConversationTranslationProvider::Cli,
        cli: ConversationTranslationCli::Opencode,
        model: "model/a".to_string(),
        prompt: "translate this".to_string(),
    }
}

async fn open_test_database_async(db_path: &Path) -> crate::backend::store::Database {
    crate::backend::store::Database::open_initialized_async(db_path)
        .await
        .expect("open initialized db")
}

async fn upsert_test_source_async(db: &crate::backend::store::Database, source: &Source) {
    crate::backend::store::upsert_source_sqlx(db.pool(), "default", source)
        .await
        .expect("insert source");
}

async fn load_test_sources_async(db: &crate::backend::store::Database) -> Vec<Source> {
    crate::backend::store::load_sources_sqlx(db.pool(), "default")
        .await
        .expect("load sources")
}

async fn upsert_test_profile_async(db: &crate::backend::store::Database, profile: &TargetProfile) {
    crate::backend::store::upsert_profile_sqlx(db.pool(), "default", profile)
        .await
        .expect("insert profile");
}

async fn delete_test_profile_async(db: &crate::backend::store::Database, profile_id: &str) {
    crate::backend::store::delete_profile_sqlx(db.pool(), "default", profile_id)
        .await
        .expect("delete profile");
}

async fn load_test_profiles_async(db: &crate::backend::store::Database) -> Vec<TargetProfile> {
    crate::backend::store::load_profiles_sqlx(db.pool(), "default")
        .await
        .expect("load profiles")
}

async fn replace_test_source_assets_async(
    db: &crate::backend::store::Database,
    source_id: &str,
    assets: &[Asset],
) {
    crate::backend::store::replace_source_assets_sqlx(db.pool(), "default", source_id, assets)
        .await
        .expect("insert assets");
}

async fn set_test_asset_mount_async(
    db: &crate::backend::store::Database,
    asset_id: &str,
    profile_id: &str,
    enabled: bool,
    strategy: DeploymentStrategy,
) -> AssetMount {
    crate::backend::store::set_asset_mount_sqlx(
        db.pool(),
        "default",
        asset_id,
        profile_id,
        enabled,
        strategy,
    )
    .await
    .expect("insert mount")
}

async fn load_test_assets_async(db: &crate::backend::store::Database) -> Vec<Asset> {
    crate::backend::store::load_assets_sqlx(db.pool(), "default", None)
        .await
        .expect("load assets")
}

async fn load_test_mounts_async(
    db: &crate::backend::store::Database,
    asset_id: Option<&str>,
) -> Vec<AssetMount> {
    crate::backend::store::load_asset_mounts_sqlx(db.pool(), "default", asset_id)
        .await
        .expect("load mounts")
}

async fn upsert_test_group_async(db: &crate::backend::store::Database, group: &AssetGroup) {
    crate::backend::store::upsert_asset_group_sqlx(db.pool(), "default", group)
        .await
        .expect("insert group");
}

async fn replace_test_group_members_async(
    db: &crate::backend::store::Database,
    group_id: &str,
    asset_ids: &[String],
    assets: &[Asset],
) {
    crate::backend::store::replace_asset_group_members_sqlx(
        db.pool(),
        "default",
        group_id,
        asset_ids,
        assets,
    )
    .await
    .expect("insert group members");
}

async fn load_test_mount_observations_async(
    db: &crate::backend::store::Database,
) -> Vec<crate::backend::dto::AssetMountObservation> {
    crate::backend::store::load_asset_mount_observations_sqlx(db.pool(), "default")
        .await
        .expect("load observations")
}

async fn is_test_managed_deployment_async(
    db: &crate::backend::store::Database,
    profile_id: &str,
    asset_id: &str,
    target_path: &str,
) -> bool {
    crate::backend::store::is_managed_deployment_sqlx(
        db.pool(),
        "default",
        profile_id,
        asset_id,
        target_path,
    )
    .await
    .expect("deployment state")
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_recorded_assets_prunes_missing_sources() {
    let db_path = unique_temp_path("assetiweave-refresh-recorded");
    let database = open_test_database_async(&db_path).await;
    let source = test_missing_source("missing-recorded-source");
    upsert_test_source_async(&database, &source).await;

    refresh_recorded_assets(database.pool(), "default")
        .await
        .expect("refresh recorded assets");

    assert!(!load_test_sources_async(&database)
        .await
        .iter()
        .any(|candidate| candidate.id == source.id));
    std::fs::remove_file(db_path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn source_scan_prunes_missing_sources_without_error_row() {
    let db_path = unique_temp_path("assetiweave-scan-missing-source");
    let database = open_test_database_async(&db_path).await;
    let source = test_missing_source("missing-scan-source");
    upsert_test_source_async(&database, &source).await;

    scan_selected_sources(
        database.pool(),
        "default",
        vec![source.clone()],
        crate::backend::capabilities::scan_source,
    )
    .await
    .expect("scan selected sources");

    assert!(!load_test_sources_async(&database)
        .await
        .iter()
        .any(|candidate| candidate.id == source.id));
    std::fs::remove_file(db_path).ok();
}

#[test]
fn target_profile_input_uses_skill_mount_defaults() {
    let profile = target_profile_from_input(TargetProfileInput {
        id: None,
        name: "  Team App  ".to_string(),
        app_kind: None,
        target_provider_id: None,
        target_paths: Some(vec!["  ~/team-app/skills  ".to_string()]),
        supported_kinds: None,
        deployment_strategy: None,
        enabled: None,
        include: None,
        exclude: None,
        safety: None,
    })
    .expect("build profile");

    assert_eq!(profile.id, "team-app");
    assert_eq!(profile.name, "Team App");
    assert_eq!(profile.app_kind, Some(AppKind::Custom));
    assert_eq!(profile.target_paths, vec!["~/team-app/skills"]);
    assert_eq!(profile.supported_kinds, vec![AssetKind::Skill]);
    assert_eq!(profile.include.kinds, vec![AssetKind::Skill]);
    assert_eq!(profile.exclude.kinds, vec![AssetKind::Unclassified]);
    assert!(!profile.safety.allow_remove);
    assert!(!profile.safety.allow_overwrite);
}

#[tokio::test(flavor = "multi_thread")]
async fn target_profile_can_be_persisted_updated_and_deleted() {
    let db_path = unique_temp_path("assetiweave-profile-crud-db");
    let database = open_test_database_async(&db_path).await;
    let mut profile = target_profile_from_input(TargetProfileInput {
        id: Some("team-app".to_string()),
        name: "Team App".to_string(),
        app_kind: Some(AppKind::Custom),
        target_provider_id: None,
        target_paths: Some(vec!["~/team-app/skills".to_string()]),
        supported_kinds: None,
        deployment_strategy: None,
        enabled: Some(true),
        include: None,
        exclude: None,
        safety: None,
    })
    .expect("build profile");

    upsert_test_profile_async(&database, &profile).await;
    profile.name = "Team App Edited".to_string();
    upsert_test_profile_async(&database, &profile).await;

    assert!(load_test_profiles_async(&database)
        .await
        .iter()
        .any(|candidate| candidate.id == profile.id && candidate.name == "Team App Edited"));

    ensure_profile_can_be_deleted_sqlx(database.pool(), "default", &profile.id)
        .await
        .expect("profile delete guard");
    delete_test_profile_async(&database, &profile.id).await;
    assert!(!load_test_profiles_async(&database)
        .await
        .iter()
        .any(|candidate| candidate.id == profile.id));
    std::fs::remove_file(db_path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn default_app_profile_delete_is_blocked() {
    let db_path = unique_temp_path("assetiweave-default-profile-delete-db");
    let database = open_test_database_async(&db_path).await;

    let error = ensure_profile_can_be_deleted_sqlx(database.pool(), "default", "codex")
        .await
        .expect_err("delete blocked");

    assert!(error.to_string().contains("default app cannot be deleted"));
    std::fs::remove_file(db_path).ok();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn target_profile_delete_is_blocked_when_mount_exists() {
    let db_path = unique_temp_path("assetiweave-profile-delete-block-db");
    let source_root = unique_temp_path("assetiweave-profile-delete-block-source");
    let target_root = unique_temp_path("assetiweave-profile-delete-block-target");
    let asset_path = source_root.join("skill-a");
    std::fs::create_dir_all(&asset_path).expect("create asset dir");
    std::fs::create_dir_all(&target_root).expect("create target dir");

    let database = open_test_database_async(&db_path).await;
    let source = test_source("profile-delete-source", source_root.clone());
    let profile = test_profile("team-app", target_root.clone());
    let asset = test_asset(&source, "skill-a", asset_path);
    upsert_test_source_async(&database, &source).await;
    replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
    upsert_test_profile_async(&database, &profile).await;
    mount_asset_mount_record(database.pool(), "default", &asset.id, &profile.id)
        .await
        .expect("mount asset");

    let error = ensure_profile_can_be_deleted_sqlx(database.pool(), "default", &profile.id)
        .await
        .expect_err("delete blocked");

    assert!(
        error.to_string().contains("managed deployments")
            || error.to_string().contains("mounted assets")
    );
    std::fs::remove_dir_all(source_root).ok();
    std::fs::remove_dir_all(target_root).ok();
    std::fs::remove_file(db_path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_recorded_assets_removes_mounts_for_deleted_assets() {
    let db_path = unique_temp_path("assetiweave-refresh-deleted-mount");
    let source_root = unique_temp_path("assetiweave-existing-source");
    std::fs::create_dir_all(&source_root).expect("create source root");
    let database = open_test_database_async(&db_path).await;
    let source = test_source("source-with-deleted-asset", source_root.clone());
    let asset = test_asset(&source, "deleted-asset", source_root.join("deleted-asset"));
    upsert_test_source_async(&database, &source).await;
    replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
    set_test_asset_mount_async(
        &database,
        &asset.id,
        "codex",
        true,
        DeploymentStrategy::SymlinkToSource,
    )
    .await;

    refresh_recorded_assets(database.pool(), "default")
        .await
        .expect("refresh recorded assets");

    assert!(load_test_assets_async(&database)
        .await
        .iter()
        .all(|candidate| candidate.id != asset.id));
    assert!(load_test_mounts_async(&database, Some(&asset.id))
        .await
        .is_empty());
    std::fs::remove_dir_all(source_root).ok();
    std::fs::remove_file(db_path).ok();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn mount_asset_mount_creates_symlink_and_enables_mount() {
    let db_path = unique_temp_path("assetiweave-mount-db");
    let source_root = unique_temp_path("assetiweave-mount-source");
    let target_root = unique_temp_path("assetiweave-mount-target");
    let asset_path = source_root.join("skill-a");
    let target_path = target_root.join("skill-a");
    std::fs::create_dir_all(&asset_path).expect("create asset dir");
    std::fs::create_dir_all(&target_root).expect("create target dir");

    let database = open_test_database_async(&db_path).await;
    let source = test_source("source-with-unmounted-asset", source_root.clone());
    let profile = test_profile("codex", target_root.clone());
    let asset = test_asset(&source, "skill-a", asset_path.clone());
    upsert_test_source_async(&database, &source).await;
    replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
    upsert_test_profile_async(&database, &profile).await;

    let result = mount_asset_mount_record(database.pool(), "default", &asset.id, &profile.id)
        .await
        .expect("mount");

    let metadata = std::fs::symlink_metadata(&target_path).expect("target metadata");
    assert!(metadata.file_type().is_symlink());
    assert_eq!(
        std::fs::read_link(&target_path).expect("read symlink"),
        asset_path.canonicalize().expect("canonical asset path")
    );
    assert!(result.mount.enabled);
    assert_eq!(result.status.state, PhysicalMountStateDto::Mounted);
    assert!(
        is_test_managed_deployment_async(
            &database,
            &profile.id,
            &asset.id,
            &target_path.to_string_lossy()
        )
        .await
    );

    std::fs::remove_dir_all(source_root).ok();
    std::fs::remove_dir_all(target_root).ok();
    std::fs::remove_file(db_path).ok();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn mount_asset_mount_links_to_real_source_directory() {
    let db_path = unique_temp_path("assetiweave-mount-real-source-db");
    let real_root = unique_temp_path("assetiweave-mount-real-source-real");
    let alias_root = unique_temp_path("assetiweave-mount-real-source-alias");
    let target_root = unique_temp_path("assetiweave-mount-real-source-target");
    let real_asset_path = real_root.join("skill-a");
    let alias_asset_path = alias_root.join("skill-a");
    let target_path = target_root.join("skill-a");
    std::fs::create_dir_all(&real_asset_path).expect("create real asset dir");
    std::fs::create_dir_all(&alias_root).expect("create alias root");
    std::fs::create_dir_all(&target_root).expect("create target dir");
    std::os::unix::fs::symlink(&real_asset_path, &alias_asset_path)
        .expect("create alias asset symlink");

    let database = open_test_database_async(&db_path).await;
    let source = test_source("source-with-aliased-asset", alias_root.clone());
    let profile = test_profile("codex", target_root.clone());
    let asset = test_asset(&source, "skill-a", alias_asset_path.clone());
    upsert_test_source_async(&database, &source).await;
    replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
    upsert_test_profile_async(&database, &profile).await;

    let result = mount_asset_mount_record(database.pool(), "default", &asset.id, &profile.id)
        .await
        .expect("mount");

    assert_eq!(
        std::fs::read_link(&target_path).expect("read target symlink"),
        real_asset_path
            .canonicalize()
            .expect("canonical real asset")
    );
    let expected_source = real_asset_path
        .canonicalize()
        .expect("canonical real asset")
        .to_string_lossy()
        .to_string();
    assert_eq!(
        result.status.linked_source.as_deref(),
        Some(expected_source.as_str())
    );
    assert_eq!(result.status.state, PhysicalMountStateDto::Mounted);

    std::fs::remove_dir_all(real_root).ok();
    std::fs::remove_dir_all(alias_root).ok();
    std::fs::remove_dir_all(target_root).ok();
    std::fs::remove_file(db_path).ok();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn set_asset_mount_creates_symlink_before_enabling_mount() {
    let db_path = unique_temp_path("assetiweave-set-mount-db");
    let source_root = unique_temp_path("assetiweave-set-mount-source");
    let target_root = unique_temp_path("assetiweave-set-mount-target");
    let asset_path = source_root.join("skill-a");
    let target_path = target_root.join("skill-a");
    std::fs::create_dir_all(&asset_path).expect("create asset dir");
    std::fs::create_dir_all(&target_root).expect("create target dir");

    let database = open_test_database_async(&db_path).await;
    let source = test_source("source-with-set-mounted-asset", source_root.clone());
    let profile = test_profile("codex", target_root.clone());
    let asset = test_asset(&source, "skill-a", asset_path);
    upsert_test_source_async(&database, &source).await;
    replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
    upsert_test_profile_async(&database, &profile).await;
    let mount = set_asset_mount_record(
        database.pool(),
        "default",
        &asset.id,
        &profile.id,
        true,
        None,
    )
    .await
    .expect("set mount enabled");

    assert!(mount.enabled);
    assert!(std::fs::symlink_metadata(&target_path)
        .expect("target metadata")
        .file_type()
        .is_symlink());

    std::fs::remove_dir_all(source_root).ok();
    std::fs::remove_dir_all(target_root).ok();
    std::fs::remove_file(db_path).ok();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn apply_skill_group_mount_only_mounts_group_members() {
    let db_path = unique_temp_path("assetiweave-group-mount-db");
    let source_root = unique_temp_path("assetiweave-group-mount-source");
    let target_root = unique_temp_path("assetiweave-group-mount-target");
    let asset_path_a = source_root.join("skill-a");
    let asset_path_b = source_root.join("skill-b");
    let target_path_a = target_root.join("skill-a");
    let target_path_b = target_root.join("skill-b");
    std::fs::create_dir_all(&asset_path_a).expect("create asset dir a");
    std::fs::create_dir_all(&asset_path_b).expect("create asset dir b");
    std::fs::create_dir_all(&target_root).expect("create target dir");

    let database = open_test_database_async(&db_path).await;
    let source = test_source("source-with-group-assets", source_root.clone());
    let profile = test_profile("codex", target_root.clone());
    let asset_a = test_asset(&source, "skill-a", asset_path_a.clone());
    let asset_b = test_asset(&source, "skill-b", asset_path_b);
    let assets = vec![asset_a.clone(), asset_b.clone()];
    let group = test_group("frontend");
    upsert_test_source_async(&database, &source).await;
    replace_test_source_assets_async(&database, &source.id, &assets).await;
    upsert_test_profile_async(&database, &profile).await;
    upsert_test_group_async(&database, &group).await;
    replace_test_group_members_async(&database, &group.id, &[asset_a.id.clone()], &assets).await;

    let result =
        apply_skill_group_mount_record(database.pool(), "default", &group.id, &profile.id, true)
            .await
            .expect("apply group");

    assert_eq!(result.requested_count, 1);
    assert_eq!(result.updated_count, 1);
    assert_eq!(result.error_count, 0);
    assert!(std::fs::symlink_metadata(&target_path_a)
        .expect("target a metadata")
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read_link(&target_path_a).expect("read symlink"),
        asset_path_a.canonicalize().expect("canonical asset path a")
    );
    assert!(!target_path_b.exists());
    assert!(load_test_mounts_async(&database, Some(&asset_b.id))
        .await
        .is_empty());

    std::fs::remove_dir_all(source_root).ok();
    std::fs::remove_dir_all(target_root).ok();
    std::fs::remove_file(db_path).ok();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn preview_exclusive_group_mount_uses_enabled_group_union_without_mutation() {
    let db_path = unique_temp_path("assetiweave-exclusive-preview-db");
    let source_root = unique_temp_path("assetiweave-exclusive-preview-source");
    let codex_target = unique_temp_path("assetiweave-exclusive-preview-codex");
    let cursor_target = unique_temp_path("assetiweave-exclusive-preview-cursor");
    let asset_path_a = source_root.join("skill-a");
    let asset_path_b = source_root.join("skill-b");
    let asset_path_c = source_root.join("skill-c");
    std::fs::create_dir_all(&asset_path_a).expect("create asset dir a");
    std::fs::create_dir_all(&asset_path_b).expect("create asset dir b");
    std::fs::create_dir_all(&asset_path_c).expect("create asset dir c");
    std::fs::create_dir_all(&codex_target).expect("create codex target");
    std::fs::create_dir_all(&cursor_target).expect("create cursor target");

    let database = open_test_database_async(&db_path).await;
    let source = test_source("source-with-exclusive-preview-assets", source_root.clone());
    let codex = test_profile("codex", codex_target.clone());
    let cursor = test_profile("cursor", cursor_target.clone());
    let asset_a = test_asset(&source, "skill-a", asset_path_a);
    let asset_b = test_asset(&source, "skill-b", asset_path_b);
    let asset_c = test_asset(&source, "skill-c", asset_path_c);
    let skill_assets = vec![asset_a.clone(), asset_b.clone(), asset_c.clone()];
    let group_a = test_group("frontend");
    let group_b = test_group("automation");
    let mut disabled_group = test_group("disabled");
    disabled_group.enabled = false;
    upsert_test_source_async(&database, &source).await;
    replace_test_source_assets_async(&database, &source.id, &skill_assets).await;
    upsert_test_profile_async(&database, &codex).await;
    upsert_test_profile_async(&database, &cursor).await;
    for group in [&group_a, &group_b, &disabled_group] {
        upsert_test_group_async(&database, group).await;
    }
    replace_test_group_members_async(
        &database,
        &group_a.id,
        &[asset_a.id.clone(), asset_b.id.clone()],
        &skill_assets,
    )
    .await;
    replace_test_group_members_async(&database, &group_b.id, &[asset_b.id.clone()], &skill_assets)
        .await;
    replace_test_group_members_async(
        &database,
        &disabled_group.id,
        &[asset_c.id.clone()],
        &skill_assets,
    )
    .await;
    mount_asset_mount_record(database.pool(), "default", &asset_a.id, &codex.id)
        .await
        .expect("mount skill a");
    mount_asset_mount_record(database.pool(), "default", &asset_c.id, &codex.id)
        .await
        .expect("mount skill c");
    mount_asset_mount_record(database.pool(), "default", &asset_c.id, &cursor.id)
        .await
        .expect("mount skill c cursor");

    let preview = build_skill_group_exclusive_mount_preview_sqlx(
        database.pool(),
        "default",
        &SkillGroupExclusiveMountInput {
            group_ids: vec![
                group_a.id.clone(),
                group_b.id.clone(),
                disabled_group.id.clone(),
                group_a.id.clone(),
            ],
            profile_id: codex.id.clone(),
            mount_selected: true,
            dry_run: true,
        },
    )
    .await
    .expect("preview exclusive mount");

    assert_eq!(
        preview.group_ids,
        vec![group_a.id.clone(), group_b.id.clone()]
    );
    assert_eq!(
        preview.selected_skill_ids,
        vec![asset_a.id.clone(), asset_b.id.clone()]
    );
    assert_eq!(preview.keep, vec![exclusive_item(&asset_a)]);
    assert_eq!(preview.mount, vec![exclusive_item(&asset_b)]);
    assert_eq!(preview.unmount, vec![exclusive_item(&asset_c)]);
    assert_eq!(preview.skipped_count, 0);
    assert!(codex_target.join("skill-c").exists());
    assert!(cursor_target.join("skill-c").exists());
    assert!(load_test_mounts_async(&database, Some(&asset_c.id))
        .await
        .iter()
        .any(|mount| mount.profile_id == codex.id && mount.enabled));

    std::fs::remove_dir_all(source_root).ok();
    std::fs::remove_dir_all(codex_target).ok();
    std::fs::remove_dir_all(cursor_target).ok();
    std::fs::remove_file(db_path).ok();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn apply_exclusive_group_mount_only_changes_target_profile_skill_mounts() {
    let db_path = unique_temp_path("assetiweave-exclusive-apply-db");
    let source_root = unique_temp_path("assetiweave-exclusive-apply-source");
    let codex_target = unique_temp_path("assetiweave-exclusive-apply-codex");
    let cursor_target = unique_temp_path("assetiweave-exclusive-apply-cursor");
    let asset_path_a = source_root.join("skill-a");
    let asset_path_b = source_root.join("skill-b");
    let asset_path_c = source_root.join("skill-c");
    let prompt_path = source_root.join("prompt-a");
    let prompt_target = codex_target.join("prompt-a");
    std::fs::create_dir_all(&asset_path_a).expect("create asset dir a");
    std::fs::create_dir_all(&asset_path_b).expect("create asset dir b");
    std::fs::create_dir_all(&asset_path_c).expect("create asset dir c");
    std::fs::create_dir_all(&prompt_path).expect("create prompt dir");
    std::fs::create_dir_all(&codex_target).expect("create codex target");
    std::fs::create_dir_all(&cursor_target).expect("create cursor target");

    let database = open_test_database_async(&db_path).await;
    let source = test_source("source-with-exclusive-apply-assets", source_root.clone());
    let codex = test_profile("codex", codex_target.clone());
    let cursor = test_profile("cursor", cursor_target.clone());
    let asset_a = test_asset(&source, "skill-a", asset_path_a);
    let asset_b = test_asset(&source, "skill-b", asset_path_b);
    let asset_c = test_asset(&source, "skill-c", asset_path_c);
    let prompt = test_asset_with_kind(&source, "prompt-a", prompt_path.clone(), AssetKind::Prompt);
    let all_assets = vec![
        asset_a.clone(),
        asset_b.clone(),
        asset_c.clone(),
        prompt.clone(),
    ];
    let skill_assets = vec![asset_a.clone(), asset_b.clone(), asset_c.clone()];
    let group_a = test_group("frontend");
    let group_b = test_group("automation");
    let mut disabled_group = test_group("disabled");
    disabled_group.enabled = false;
    upsert_test_source_async(&database, &source).await;
    replace_test_source_assets_async(&database, &source.id, &all_assets).await;
    upsert_test_profile_async(&database, &codex).await;
    upsert_test_profile_async(&database, &cursor).await;
    for group in [&group_a, &group_b, &disabled_group] {
        upsert_test_group_async(&database, group).await;
    }
    replace_test_group_members_async(
        &database,
        &group_a.id,
        &[asset_a.id.clone(), asset_b.id.clone()],
        &skill_assets,
    )
    .await;
    replace_test_group_members_async(&database, &group_b.id, &[asset_b.id.clone()], &skill_assets)
        .await;
    replace_test_group_members_async(
        &database,
        &disabled_group.id,
        &[asset_c.id.clone()],
        &skill_assets,
    )
    .await;
    mount_asset_mount_record(database.pool(), "default", &asset_a.id, &codex.id)
        .await
        .expect("mount skill a");
    mount_asset_mount_record(database.pool(), "default", &asset_c.id, &codex.id)
        .await
        .expect("mount skill c");
    mount_asset_mount_record(database.pool(), "default", &asset_c.id, &cursor.id)
        .await
        .expect("mount skill c cursor");
    std::os::unix::fs::symlink(&prompt_path, &prompt_target).expect("create prompt symlink");
    set_test_asset_mount_async(
        &database,
        &prompt.id,
        &codex.id,
        true,
        DeploymentStrategy::SymlinkToSource,
    )
    .await;

    let result = apply_skill_group_exclusive_mount_record(
        database.pool(),
        "default",
        &SkillGroupExclusiveMountInput {
            group_ids: vec![
                group_a.id.clone(),
                group_b.id.clone(),
                disabled_group.id.clone(),
            ],
            profile_id: codex.id.clone(),
            mount_selected: true,
            dry_run: false,
        },
    )
    .await
    .expect("apply exclusive mount");

    assert_eq!(result.preview.keep_count, 1);
    assert_eq!(result.preview.mount_count, 1);
    assert_eq!(result.preview.unmount_count, 1);
    assert_eq!(result.preview.skipped_count, 0);
    assert!(result.errors.is_empty());
    assert!(codex_target.join("skill-a").exists());
    assert!(codex_target.join("skill-b").exists());
    assert!(!codex_target.join("skill-c").exists());
    assert!(cursor_target.join("skill-c").exists());
    assert!(prompt_target.exists());
    let skill_c_mounts = load_test_mounts_async(&database, Some(&asset_c.id)).await;
    assert!(skill_c_mounts
        .iter()
        .any(|mount| mount.profile_id == codex.id && !mount.enabled));
    assert!(skill_c_mounts
        .iter()
        .any(|mount| mount.profile_id == cursor.id && mount.enabled));
    assert!(load_test_mounts_async(&database, Some(&prompt.id))
        .await
        .iter()
        .any(|mount| mount.profile_id == codex.id && mount.enabled));

    std::fs::remove_dir_all(source_root).ok();
    std::fs::remove_dir_all(codex_target).ok();
    std::fs::remove_dir_all(cursor_target).ok();
    std::fs::remove_file(db_path).ok();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn preview_exclusive_group_mount_reports_risks_without_forcing_repairs() {
    let db_path = unique_temp_path("assetiweave-exclusive-risk-db");
    let external_root = unique_temp_path("assetiweave-exclusive-risk-external");
    let app_local_root = unique_temp_path("assetiweave-exclusive-risk-local");
    let target_root = unique_temp_path("assetiweave-exclusive-risk-target");
    let external_asset_path = external_root.join("external-skill");
    let app_local_asset_path = app_local_root.join("app-local-skill");
    let external_target = target_root.join("external-skill");
    std::fs::create_dir_all(&external_asset_path).expect("create external asset dir");
    std::fs::create_dir_all(&app_local_asset_path).expect("create app local asset dir");
    std::fs::create_dir_all(&target_root).expect("create target dir");
    std::os::unix::fs::symlink(&external_asset_path, &external_target)
        .expect("create unmanaged external symlink");

    let database = open_test_database_async(&db_path).await;
    let external_source = test_source("external-source", external_root.clone());
    let app_local_source = test_source_with_origin(
        "app-local-source",
        app_local_root.clone(),
        SourceOrigin::AppLocal,
    );
    let profile = test_profile("codex", target_root.clone());
    let external_asset = test_asset(&external_source, "external-skill", external_asset_path);
    let app_local_asset = test_asset(&app_local_source, "app-local-skill", app_local_asset_path);
    let assets = vec![external_asset.clone(), app_local_asset.clone()];
    let group = test_group("selected-app-local");
    upsert_test_source_async(&database, &external_source).await;
    upsert_test_source_async(&database, &app_local_source).await;
    replace_test_source_assets_async(&database, &external_source.id, &[external_asset.clone()])
        .await;
    replace_test_source_assets_async(&database, &app_local_source.id, &[app_local_asset.clone()])
        .await;
    upsert_test_profile_async(&database, &profile).await;
    upsert_test_group_async(&database, &group).await;
    replace_test_group_members_async(&database, &group.id, &[app_local_asset.id.clone()], &assets)
        .await;

    let result = apply_skill_group_exclusive_mount_record(
        database.pool(),
        "default",
        &SkillGroupExclusiveMountInput {
            group_ids: vec![group.id.clone()],
            profile_id: profile.id.clone(),
            mount_selected: true,
            dry_run: false,
        },
    )
    .await
    .expect("apply exclusive mount");

    assert_eq!(result.preview.mount_count, 0);
    assert_eq!(result.preview.unmount_count, 0);
    assert_eq!(result.preview.skipped_count, 2);
    assert!(result.preview.skipped.iter().any(
        |item| item.asset_id == app_local_asset.id && item.reason.contains("must be backed up")
    ));
    assert!(result
        .preview
        .skipped
        .iter()
        .any(|item| item.asset_id == external_asset.id
            && item.reason.contains("not managed by AssetIWeave")));
    assert!(result.errors.is_empty());
    assert!(external_target.exists());

    std::fs::remove_dir_all(external_root).ok();
    std::fs::remove_dir_all(app_local_root).ok();
    std::fs::remove_dir_all(target_root).ok();
    std::fs::remove_file(db_path).ok();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn scan_asset_mount_statuses_does_not_mutate_snapshot() {
    let db_path = unique_temp_path("assetiweave-status-scan-db");
    let source_root = unique_temp_path("assetiweave-status-scan-source");
    let target_root = unique_temp_path("assetiweave-status-scan-target");
    let asset_path = source_root.join("skill-a");
    let target_path = target_root.join("skill-a");
    std::fs::create_dir_all(&asset_path).expect("create asset dir");
    std::fs::create_dir_all(&target_root).expect("create target dir");
    std::os::unix::fs::symlink(&asset_path, &target_path).expect("create physical symlink");

    let database = open_test_database_async(&db_path).await;
    let source = test_source("source-with-scanned-asset", source_root.clone());
    let profile = test_profile("codex", target_root.clone());
    let asset = test_asset(&source, "skill-a", asset_path);
    upsert_test_source_async(&database, &source).await;
    replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
    upsert_test_profile_async(&database, &profile).await;
    set_test_asset_mount_async(
        &database,
        &asset.id,
        &profile.id,
        false,
        DeploymentStrategy::SymlinkToSource,
    )
    .await;

    let statuses = scan_asset_mount_statuses_sqlx(database.pool(), "default", None)
        .await
        .expect("scan statuses");

    assert!(statuses.iter().any(|status| {
        status.asset_id == asset.id
            && status.profile_id == profile.id
            && status.state == PhysicalMountStateDto::Mounted
    }));
    assert!(load_test_mounts_async(&database, Some(&asset.id))
        .await
        .iter()
        .all(|mount| !mount.enabled));
    assert!(
        !is_test_managed_deployment_async(
            &database,
            &profile.id,
            &asset.id,
            &target_path.to_string_lossy()
        )
        .await
    );

    std::fs::remove_dir_all(source_root).ok();
    std::fs::remove_dir_all(target_root).ok();
    std::fs::remove_file(db_path).ok();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn sync_asset_mount_observations_records_physical_mount_snapshot() {
    let db_path = unique_temp_path("assetiweave-observation-db");
    let source_root = unique_temp_path("assetiweave-observation-source");
    let target_root = unique_temp_path("assetiweave-observation-target");
    let asset_path = source_root.join("skill-a");
    let target_path = target_root.join("skill-a");
    std::fs::create_dir_all(&asset_path).expect("create asset dir");
    std::fs::create_dir_all(&target_root).expect("create target dir");
    std::os::unix::fs::symlink(&asset_path, &target_path).expect("create physical symlink");

    let database = open_test_database_async(&db_path).await;
    let source = test_source("source-with-observed-asset", source_root.clone());
    let profile = test_profile("codex", target_root.clone());
    let asset = test_asset(&source, "skill-a", asset_path);
    upsert_test_source_async(&database, &source).await;
    replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
    upsert_test_profile_async(&database, &profile).await;
    let original_mount = set_test_asset_mount_async(
        &database,
        &asset.id,
        &profile.id,
        false,
        DeploymentStrategy::SymlinkToSource,
    )
    .await;

    sync_asset_mount_observations(database.pool(), "default", None)
        .await
        .expect("sync observations");

    let observations = load_test_mount_observations_async(&database).await;
    let observation = observations
        .iter()
        .find(|candidate| candidate.asset_id == asset.id && candidate.profile_id == profile.id)
        .expect("asset/profile observation");
    assert_eq!(observation.state, PhysicalMountStateDto::Mounted);
    assert!(!observation.observed_at.is_empty());
    let mounts = load_test_mounts_async(&database, Some(&asset.id)).await;
    let synced_mount = mounts
        .iter()
        .find(|mount| mount.profile_id == profile.id)
        .expect("synced mount");
    assert!(synced_mount.enabled);
    assert_eq!(synced_mount.created_at, original_mount.created_at);
    assert!(
        is_test_managed_deployment_async(
            &database,
            &profile.id,
            &asset.id,
            &target_path.to_string_lossy()
        )
        .await
    );

    std::fs::remove_dir_all(source_root).ok();
    std::fs::remove_dir_all(target_root).ok();
    std::fs::remove_file(db_path).ok();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn sync_asset_mount_observations_repairs_ghost_alias_symlink() {
    let db_path = unique_temp_path("assetiweave-observation-ghost-db");
    let real_root = unique_temp_path("assetiweave-observation-ghost-real");
    let alias_root = unique_temp_path("assetiweave-observation-ghost-alias");
    let target_root = unique_temp_path("assetiweave-observation-ghost-target");
    let real_asset_path = real_root.join("skill-a");
    let alias_asset_path = alias_root.join("skill-a");
    let target_path = target_root.join("skill-a");
    std::fs::create_dir_all(&real_asset_path).expect("create real asset dir");
    std::fs::create_dir_all(&alias_root).expect("create alias root");
    std::fs::create_dir_all(&target_root).expect("create target dir");
    std::os::unix::fs::symlink(&real_asset_path, &alias_asset_path)
        .expect("create alias asset symlink");
    std::os::unix::fs::symlink(&alias_asset_path, &target_path)
        .expect("create ghost target symlink");

    let database = open_test_database_async(&db_path).await;
    let source = test_source("source-with-ghost-asset", alias_root.clone());
    let profile = test_profile("codex", target_root.clone());
    let asset = test_asset(&source, "skill-a", alias_asset_path);
    upsert_test_source_async(&database, &source).await;
    replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
    upsert_test_profile_async(&database, &profile).await;

    sync_asset_mount_observations(database.pool(), "default", None)
        .await
        .expect("sync observations");

    assert_eq!(
        std::fs::read_link(&target_path).expect("read repaired target symlink"),
        real_asset_path
            .canonicalize()
            .expect("canonical real asset")
    );
    let observations = load_test_mount_observations_async(&database).await;
    let observation = observations
        .iter()
        .find(|candidate| candidate.asset_id == asset.id && candidate.profile_id == profile.id)
        .expect("asset/profile observation");
    assert_eq!(observation.state, PhysicalMountStateDto::Mounted);
    let expected_source = real_asset_path
        .canonicalize()
        .expect("canonical real asset")
        .to_string_lossy()
        .to_string();
    assert_eq!(
        observation.linked_source.as_deref(),
        Some(expected_source.as_str())
    );

    std::fs::remove_dir_all(real_root).ok();
    std::fs::remove_dir_all(alias_root).ok();
    std::fs::remove_dir_all(target_root).ok();
    std::fs::remove_file(db_path).ok();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn sync_asset_mount_observations_clears_snapshot_when_link_is_missing() {
    let db_path = unique_temp_path("assetiweave-observation-missing-db");
    let source_root = unique_temp_path("assetiweave-observation-missing-source");
    let target_root = unique_temp_path("assetiweave-observation-missing-target");
    let asset_path = source_root.join("skill-a");
    let target_path = target_root.join("skill-a");
    std::fs::create_dir_all(&asset_path).expect("create asset dir");
    std::fs::create_dir_all(&target_root).expect("create target dir");

    let database = open_test_database_async(&db_path).await;
    let source = test_source("source-with-missing-observed-asset", source_root.clone());
    let profile = test_profile("codex", target_root.clone());
    let asset = test_asset(&source, "skill-a", asset_path);
    upsert_test_source_async(&database, &source).await;
    replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
    upsert_test_profile_async(&database, &profile).await;
    set_test_asset_mount_async(
        &database,
        &asset.id,
        &profile.id,
        true,
        DeploymentStrategy::SymlinkToSource,
    )
    .await;

    sync_asset_mount_observations(database.pool(), "default", None)
        .await
        .expect("sync observations");

    assert!(load_test_mounts_async(&database, Some(&asset.id))
        .await
        .iter()
        .all(|mount| !mount.enabled));
    assert!(
        !is_test_managed_deployment_async(
            &database,
            &profile.id,
            &asset.id,
            &target_path.to_string_lossy()
        )
        .await
    );

    std::fs::remove_dir_all(source_root).ok();
    std::fs::remove_dir_all(target_root).ok();
    std::fs::remove_file(db_path).ok();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn unmount_asset_mount_removes_matching_symlink_and_disables_mount() {
    let db_path = unique_temp_path("assetiweave-unmount-db");
    let source_root = unique_temp_path("assetiweave-unmount-source");
    let target_root = unique_temp_path("assetiweave-unmount-target");
    let asset_path = source_root.join("skill-a");
    let target_path = target_root.join("skill-a");
    std::fs::create_dir_all(&asset_path).expect("create asset dir");
    std::fs::create_dir_all(&target_root).expect("create target dir");
    std::os::unix::fs::symlink(&asset_path, &target_path).expect("create mounted symlink");

    let database = open_test_database_async(&db_path).await;
    let source = test_source("source-with-mounted-asset", source_root.clone());
    let profile = test_profile("codex", target_root.clone());
    let asset = test_asset(&source, "skill-a", asset_path);
    upsert_test_source_async(&database, &source).await;
    replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
    upsert_test_profile_async(&database, &profile).await;
    set_test_asset_mount_async(
        &database,
        &asset.id,
        &profile.id,
        true,
        DeploymentStrategy::SymlinkToSource,
    )
    .await;

    let result = unmount_asset_mount_record(database.pool(), "default", &asset.id, &profile.id)
        .await
        .expect("unmount");

    assert!(!target_path.exists());
    assert!(!std::fs::symlink_metadata(&target_path).is_ok());
    assert!(!result.mount.enabled);
    assert_eq!(result.status.state, PhysicalMountStateDto::NotMounted);
    assert!(load_test_mounts_async(&database, Some(&asset.id))
        .await
        .iter()
        .all(|mount| !mount.enabled));

    std::fs::remove_dir_all(source_root).ok();
    std::fs::remove_dir_all(target_root).ok();
    std::fs::remove_file(db_path).ok();
}

#[test]
fn catalog_assets_fold_backed_up_copy_to_original_source() {
    let original_source = test_source("source-a", PathBuf::from("/tmp/source-a"));
    let backup_source = assetiweave_library_source_with_root("/tmp/assetiweave-backup".to_string());
    let mut original = test_asset(
        &original_source,
        "skill-a",
        PathBuf::from("/tmp/source-a/skill-a"),
    );
    original.content_hash = Some("same-content".to_string());
    let mut backup = test_asset(
        &backup_source,
        "backup-skill-a",
        PathBuf::from("/tmp/assetiweave-backup/backed-up/source-a/skill-a"),
    );
    backup.name = "skill-a".to_string();
    backup.relative_path = "backed-up/source-a/skill-a".to_string();
    backup.content_hash = Some("same-content".to_string());

    let catalog = build_catalog_assets(
        vec![backup.clone(), original.clone()],
        &[backup_source, original_source],
    );

    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].asset.id, original.id);
    let status = catalog[0].backup_status.as_ref().expect("backup status");
    assert_eq!(status.state, SkillBackupState::BackedUp);
    assert_eq!(
        status.backup_path.as_deref(),
        Some(backup.absolute_path.as_str())
    );
    assert_eq!(status.hidden_asset_ids, vec![backup.id]);
}

#[test]
fn catalog_assets_use_backup_copy_for_app_target_duplicate() {
    let app_source = test_source_with_origin(
        "codex-skills",
        PathBuf::from("/tmp/codex"),
        SourceOrigin::AppTarget,
    );
    let backup_source = assetiweave_library_source_with_root("/tmp/assetiweave-backup".to_string());
    let mut app_asset = test_asset(&app_source, "skill-a", PathBuf::from("/tmp/codex/skill-a"));
    app_asset.content_hash = Some("same-content".to_string());
    let mut backup = test_asset(
        &backup_source,
        "backup-skill-a",
        PathBuf::from("/tmp/assetiweave-backup/backed-up/codex/skill-a"),
    );
    backup.name = "skill-a".to_string();
    backup.relative_path = "backed-up/codex/skill-a".to_string();
    backup.content_hash = Some("same-content".to_string());

    let catalog = build_catalog_assets(
        vec![app_asset.clone(), backup.clone()],
        &[app_source, backup_source],
    );

    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].asset.id, backup.id);
    assert_eq!(
        catalog[0].backup_status.as_ref().map(|status| status.state),
        Some(SkillBackupState::BackedUp)
    );
    assert_eq!(
        catalog[0]
            .backup_status
            .as_ref()
            .map(|status| status.hidden_asset_ids.clone()),
        Some(vec![app_asset.id])
    );
}

#[test]
fn catalog_assets_keep_downloaded_unique_skill() {
    let backup_source = assetiweave_library_source_with_root("/tmp/assetiweave-backup".to_string());
    let mut downloaded = test_asset(
        &backup_source,
        "downloaded-skill",
        PathBuf::from("/tmp/assetiweave-backup/downloaded/downloaded-skill"),
    );
    downloaded.relative_path = "downloaded/downloaded-skill".to_string();
    downloaded.content_hash = Some("downloaded-content".to_string());

    let catalog = build_catalog_assets(vec![downloaded.clone()], &[backup_source]);

    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].asset.id, downloaded.id);
    assert_eq!(
        catalog[0].backup_status.as_ref().map(|status| status.state),
        Some(SkillBackupState::Downloaded)
    );
}

#[test]
fn catalog_assets_do_not_fold_skills_without_hash() {
    let original_source = test_source("source-a", PathBuf::from("/tmp/source-a"));
    let backup_source = assetiweave_library_source_with_root("/tmp/assetiweave-backup".to_string());
    let original = test_asset(
        &original_source,
        "skill-a",
        PathBuf::from("/tmp/source-a/skill-a"),
    );
    let mut backup = test_asset(
        &backup_source,
        "backup-skill-a",
        PathBuf::from("/tmp/assetiweave-backup/backed-up/source-a/skill-a"),
    );
    backup.name = "skill-a".to_string();
    backup.relative_path = "backed-up/source-a/skill-a".to_string();

    let catalog = build_catalog_assets(vec![backup, original], &[backup_source, original_source]);

    assert_eq!(catalog.len(), 2);
}

#[test]
fn catalog_assets_attach_each_nested_repository_remote() {
    let collection_root = unique_temp_path("assetiweave-catalog-nested-repositories");
    let first_repo = collection_root.join("first-repo");
    let second_repo = collection_root.join("second-repo");
    let first_skill = first_repo.join("skills").join("first-skill");
    let second_skill = second_repo.join("skills").join("second-skill");
    std::fs::create_dir_all(&first_skill).expect("create first skill");
    std::fs::create_dir_all(&second_skill).expect("create second skill");
    init_git_repo(&first_repo, "https://example.com/first.git");
    init_git_repo(&second_repo, "git@example.com:second.git");

    let source = test_source("repository-collection", collection_root.clone());
    let first_asset = test_asset(&source, "first-skill", first_skill);
    let second_asset = test_asset(&source, "second-skill", second_skill);
    let catalog = build_catalog_assets(
        vec![first_asset.clone(), second_asset.clone()],
        std::slice::from_ref(&source),
    );

    let first_repository = catalog
        .iter()
        .find(|candidate| candidate.asset.id == first_asset.id)
        .and_then(|candidate| candidate.repository.as_ref())
        .expect("first repository");
    let second_repository = catalog
        .iter()
        .find(|candidate| candidate.asset.id == second_asset.id)
        .and_then(|candidate| candidate.repository.as_ref())
        .expect("second repository");
    assert_eq!(
        first_repository.remote_url.as_deref(),
        Some("https://example.com/first.git")
    );
    assert_eq!(
        second_repository.remote_url.as_deref(),
        Some("git@example.com:second.git")
    );
    assert_eq!(PathBuf::from(&first_repository.root_path), first_repo);
    assert_eq!(PathBuf::from(&second_repository.root_path), second_repo);

    std::fs::remove_dir_all(collection_root).ok();
}

#[test]
fn catalog_assets_attach_repository_browser_url_to_asset_directory() {
    let repo = unique_temp_path("assetiweave-catalog-repository-browser-url");
    let skill = repo.join("skills").join("zh-cn").join("office-utils");
    std::fs::create_dir_all(&skill).expect("create skill");
    init_git_repo(&repo, "https://github.com/util6/util6-agents.git");

    let source = test_source("repository-root", repo.clone());
    let asset = test_asset(&source, "office-utils", skill);
    let catalog = build_catalog_assets(vec![asset.clone()], std::slice::from_ref(&source));
    let repository = catalog[0].repository.as_ref().expect("repository");

    assert_eq!(
        repository.web_url.as_deref(),
        Some("https://github.com/util6/util6-agents/tree/main/skills/zh-cn/office-utils")
    );

    std::fs::remove_dir_all(repo).ok();
}

#[test]
fn catalog_assets_convert_github_ssh_remote_to_browser_url() {
    let collection_root = unique_temp_path("assetiweave-catalog-ssh-browser-url");
    let repo = collection_root.join("kicad-happy");
    let skill = repo.join("skills").join("pcbway");
    std::fs::create_dir_all(&skill).expect("create skill");
    init_git_repo(&repo, "git@github.com:aklofas/kicad-happy.git");

    let source = test_source("repository-collection", collection_root.clone());
    let asset = test_asset(&source, "pcbway", skill);
    let catalog = build_catalog_assets(vec![asset.clone()], std::slice::from_ref(&source));
    let repository = catalog[0].repository.as_ref().expect("repository");

    assert_eq!(
        repository.web_url.as_deref(),
        Some("https://github.com/aklofas/kicad-happy/tree/main/skills/pcbway")
    );

    std::fs::remove_dir_all(collection_root).ok();
}

fn test_missing_source(id: &str) -> Source {
    let root_path = unique_temp_path(id);
    test_source(id, root_path)
}

fn test_source(id: &str, root_path: PathBuf) -> Source {
    test_source_with_origin(id, root_path, SourceOrigin::GitRepo)
}

fn test_source_with_origin(id: &str, root_path: PathBuf, source_origin: SourceOrigin) -> Source {
    Source {
        id: id.to_string(),
        name: id.to_string(),
        kind: SourceKind::Local,
        root_path: root_path.to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        origin_provider_id: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: vec![],
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    }
}

fn test_profile(id: &str, target_root: PathBuf) -> TargetProfile {
    TargetProfile {
        id: id.to_string(),
        name: id.to_string(),
        app_kind: Some(AppKind::Custom),
        target_provider_id: "custom".to_string(),
        target_paths: vec![target_root.to_string_lossy().to_string()],
        supported_kinds: vec![AssetKind::Skill],
        deployment_strategy: DeploymentStrategy::SymlinkToSource,
        enabled: true,
        include: RuleSet {
            kinds: vec![AssetKind::Skill],
            tags: vec![],
            groups: vec![],
            sources: vec![],
            path_patterns: vec![],
        },
        exclude: RuleSet {
            kinds: vec![],
            tags: vec![],
            groups: vec![],
            sources: vec![],
            path_patterns: vec![],
        },
        safety: ProfileSafety {
            allow_remove: false,
            allow_overwrite: false,
        },
    }
}

fn test_asset(source: &Source, id: &str, absolute_path: PathBuf) -> Asset {
    test_asset_with_kind(source, id, absolute_path, AssetKind::Skill)
}

fn test_asset_with_kind(
    source: &Source,
    id: &str,
    absolute_path: PathBuf,
    kind: AssetKind,
) -> Asset {
    Asset {
        id: id.to_string(),
        source_id: source.id.clone(),
        name: id.to_string(),
        kind,
        detector_id: "legacy.classifier".to_string(),
        detector_version: 1,
        format: AssetFormat::Directory,
        relative_path: id.to_string(),
        absolute_path: absolute_path.to_string_lossy().to_string(),
        entry_file: None,
        description: None,
        content_hash: None,
        discovered_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

fn test_group(id: &str) -> AssetGroup {
    AssetGroup {
        id: id.to_string(),
        name: id.to_string(),
        description: None,
        color: "#10b981".to_string(),
        asset_kind: AssetKind::Skill,
        display_icon: None,
        icon_svg: None,
        enabled: true,
        sort_order: 0,
        rules: AssetGroupRules {
            source_ids: vec![],
            relative_path_globs: vec![],
            name_contains: None,
        },
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

fn unique_temp_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()))
}

fn init_git_repo(path: &Path, remote_url: &str) {
    std::fs::create_dir_all(path).expect("create repository directory");
    let init = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(path)
        .status()
        .expect("run git init");
    assert!(init.success());
    let remote = Command::new("git")
        .args(["remote", "add", "origin", remote_url])
        .current_dir(path)
        .status()
        .expect("add git remote");
    assert!(remote.success());
    let branch = Command::new("git")
        .args(["checkout", "-b", "main", "--quiet"])
        .current_dir(path)
        .status()
        .expect("create main branch");
    assert!(branch.success());
}

fn prepare_ai_execution_task(
    tasks: Arc<BackgroundTaskRegistry>,
    params: ConversationTranslationRequest,
    emitter: Arc<dyn AiExecutionTaskEmitter>,
) -> RuntimeAppResult<(AiExecutionTaskSnapshot, AiExecutionRequest)> {
    prepare_ai_execution_task_for_tenant("default", tasks, params, emitter)
}
