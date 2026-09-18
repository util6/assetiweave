use super::*;

fn params(record_kind: Option<&str>) -> ConversationSyncParams {
    ConversationSyncParams {
        source_id: None,
        adapter_id: None,
        record_kind: record_kind.map(str::to_string),
        mode: ConversationSyncMode::Incremental,
        dry_run: false,
    }
}

#[test]
fn duplicate_start_reuses_the_running_sync_task() {
    let registry = BackgroundTaskRegistry::default();

    let (first, should_start_first) = registry
        .begin_conversation_sync(&params(Some("session")))
        .unwrap();
    let (second, should_start_second) = registry
        .begin_conversation_sync(&params(Some("session")))
        .unwrap();

    assert!(should_start_first);
    assert!(!should_start_second);
    assert_eq!(first.id, second.id);
    assert!(registry.has_running_tasks());
}

#[test]
fn background_conversation_sync_launches_task_fn() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    let registry = BackgroundTaskRegistry::default();
    let (task, should_start) = registry
        .begin_conversation_sync(&params(Some("session")))
        .unwrap();
    assert!(should_start);
    assert_eq!(task.status, BackgroundTaskStatus::Running);

    let runtime = registry.task_runtime().expect("task runtime");
    let executed = Arc::new(AtomicBool::new(false));
    let executed_clone = executed.clone();

    runtime
        .start_external_with(
            &task.id,
            Value::Null,
            Box::new(move |_| {
                executed_clone.store(true, Ordering::SeqCst);
                Ok(Value::Null)
            }),
        )
        .expect("start external with");

    for _ in 0..100 {
        if executed.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        executed.load(Ordering::SeqCst),
        "task closure should have been launched and executed"
    );
}

#[test]
fn conversation_data_maintenance_tracks_progress_failure_and_cancellation() {
    let registry = BackgroundTaskRegistry::default();
    let (first, first_started) = registry
        .begin_conversation_data_maintenance_for_tenant(
            "tenant-a",
            "audit",
            Some("source-a".to_string()),
            Some("session".to_string()),
            true,
        )
        .unwrap();
    let (duplicate, duplicate_started) = registry
        .begin_conversation_data_maintenance_for_tenant(
            "tenant-a",
            "repair",
            Some("source-b".to_string()),
            Some("web".to_string()),
            false,
        )
        .unwrap();
    assert!(first_started);
    assert!(!duplicate_started);
    assert_eq!(first.id, duplicate.id);
    assert_eq!(duplicate.operation, "audit");

    let progress = registry
        .update_conversation_data_maintenance_progress(
            &first.id,
            4,
            10,
            Some("reindex".to_string()),
        )
        .unwrap();
    assert_eq!(progress.progress.completed_stage, 4);
    assert_eq!(progress.progress.note.as_deref(), Some("reindex"));

    let failed = registry
        .finish_conversation_data_maintenance(
            &first.id,
            Err(crate::backend::runtime::AppError::Validation(
                "maintenance failed".to_string(),
            )),
        )
        .unwrap();
    assert_eq!(failed.status, BackgroundTaskStatus::Failed);
    assert_eq!(
        failed.error.as_ref().map(|error| error.message.as_str()),
        Some("maintenance failed")
    );

    let (second, _) = registry
        .begin_conversation_data_maintenance_for_tenant("tenant-a", "repair", None, None, false)
        .unwrap();
    let cancelling = registry
        .cancel_conversation_data_maintenance_for_tenant("tenant-a", &second.id)
        .unwrap();
    assert_eq!(cancelling.status, BackgroundTaskStatus::Cancelling);
    let cancelled = registry
        .finish_conversation_data_maintenance(
            &second.id,
            Err(crate::backend::runtime::AppError::Cancelled(
                "cancelled".to_string(),
            )),
        )
        .unwrap();
    assert_eq!(cancelled.status, BackgroundTaskStatus::Cancelled);
    assert!(!registry.has_running_tasks());
}

#[test]
fn tenant_owned_sync_tasks_do_not_deduplicate_across_tenants() {
    let registry = BackgroundTaskRegistry::default();

    let (tenant_a, should_start_a) = registry
        .begin_conversation_sync_for_tenant("tenant-a", &params(Some("session")))
        .unwrap();
    let (tenant_b, should_start_b) = registry
        .begin_conversation_sync_for_tenant("tenant-b", &params(Some("session")))
        .unwrap();

    assert!(should_start_a);
    assert!(should_start_b);
    assert_ne!(tenant_a.id, tenant_b.id);
}

#[test]
fn remote_skill_acquire_is_tenant_scoped_and_deduplicated() {
    let registry = BackgroundTaskRegistry::default();
    let params = SkillAcquireParams {
        url: "https://github.com/example/skills".to_string(),
        branch: Some("main".to_string()),
        path: Some("browser".to_string()),
        name: None,
        dry_run: false,
        yes: true,
    };

    let (first, first_started) = registry
        .begin_remote_skill_acquire_for_tenant("tenant-a", &params)
        .unwrap();
    let (duplicate, duplicate_started) = registry
        .begin_remote_skill_acquire_for_tenant("tenant-a", &params)
        .unwrap();
    let (other_tenant, other_started) = registry
        .begin_remote_skill_acquire_for_tenant("tenant-b", &params)
        .unwrap();

    assert!(first_started);
    assert!(!duplicate_started);
    assert_eq!(first.id, duplicate.id);
    assert!(other_started);
    assert_ne!(first.id, other_tenant.id);
}

#[test]
fn source_scan_deduplicates_same_scope_and_projects_cancellation() {
    let registry = BackgroundTaskRegistry::default();
    let (first, first_started) = registry
        .begin_source_scan("tenant-a", SourceScanScope::All, None)
        .expect("start source scan");
    let (second, second_started) = registry
        .begin_source_scan("tenant-a", SourceScanScope::All, None)
        .expect("deduplicate source scan");

    assert!(first_started);
    assert!(!second_started);
    assert_eq!(first.id, second.id);
    let cancelling = registry
        .cancel_source_scan(&first.id)
        .expect("cancel source scan");
    assert_eq!(cancelling.status, BackgroundTaskStatus::Cancelling);
}

#[test]
fn batch_mount_uses_profile_conflict_and_projects_terminal_result() {
    let registry = BackgroundTaskRegistry::default();
    let (first, first_started) = registry
        .begin_batch_mount("tenant-a", "group", "profile-a", "group-a:true")
        .expect("start batch mount");
    let (second, second_started) = registry
        .begin_batch_mount("tenant-a", "exclusive", "profile-a", "group-b")
        .expect("conflict batch mount");

    assert!(first_started);
    assert!(!second_started);
    assert_eq!(first.id, second.id);

    let finished = registry
        .finish_batch_mount(&first.id, Ok(serde_json::json!({ "updated_count": 1 })))
        .expect("finish batch mount");
    assert_eq!(finished.status, BackgroundTaskStatus::Completed);
    assert_eq!(finished.result.expect("result")["updated_count"], 1);
}

#[test]
fn duplicate_search_index_rebuild_reuses_running_task() {
    let registry = BackgroundTaskRegistry::default();
    let (first, should_start_first) = registry.begin_conversation_search_index_rebuild().unwrap();
    let (second, should_start_second) = registry.begin_conversation_search_index_rebuild().unwrap();

    assert!(should_start_first);
    assert!(!should_start_second);
    assert_eq!(first.id, second.id);
    assert!(registry.has_running_tasks());

    let finished = registry
        .finish_conversation_search_index_rebuild(
            &first.id,
            Ok(serde_json::json!({ "document_count": 1 })),
        )
        .unwrap();
    assert_eq!(finished.status, BackgroundTaskStatus::Completed);
    assert!(!registry.has_running_tasks());
}

#[test]
fn search_index_registration_leaves_worker_start_to_task_runtime() {
    let registry = BackgroundTaskRegistry::default();
    let (first, should_start) = registry
        .begin_conversation_search_index_rebuild()
        .expect("register search index task");
    assert!(should_start);

    let executions = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let executions_for_worker = executions.clone();
    registry
        .task_runtime()
        .expect("shared task runtime")
        .start_external_with(
            &first.id,
            Value::Null,
            Box::new(move |_| {
                executions_for_worker.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(Value::Null)
            }),
        )
        .expect("start search index worker");

    for _ in 0..100 {
        if registry
            .task_runtime()
            .and_then(|runtime| runtime.get(&first.id))
            .is_some_and(|snapshot| snapshot.state.is_terminal())
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let runtime = registry.task_runtime().expect("shared task runtime");
    assert_eq!(executions.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        runtime.get(&first.id).expect("completed task").state,
        TaskState::Succeeded
    );
}

#[tokio::test]
async fn production_registry_can_expose_the_shared_task_runtime() {
    let runtime = TaskRuntime::new();
    let registry = BackgroundTaskRegistry::with_task_runtime(runtime.clone());

    assert!(registry.task_runtime().is_some());
    runtime.shutdown_with_grace(Duration::ZERO).await;
}

#[tokio::test]
async fn conversation_and_agent_lifecycle_use_one_kernel_task_runtime() {
    let runtime = TaskRuntime::new();
    let registry = BackgroundTaskRegistry::with_task_runtime(runtime.clone());
    let (agent, _, agent_should_start) = registry
        .begin_agent_lifecycle(
            "shared-kernel-agent".to_string(),
            "install".to_string(),
            None,
            Some("1.0.0".to_string()),
            None,
            None,
            None,
        )
        .unwrap();
    let (adapter, adapter_should_start) = registry
        .begin_conversation_adapter_package_install(&ConversationAdapterPackageInstallParams {
            catalog_url: None,
            package_id: "shared-kernel-agent".to_string(),
            version: Some("1.0.0".to_string()),
            dry_run: false,
            yes: true,
        })
        .unwrap();

    assert!(agent_should_start);
    assert!(adapter_should_start);
    assert_ne!(agent.id, adapter.id);

    let (agent_release, agent_wait) = std::sync::mpsc::channel();
    let (adapter_release, adapter_wait) = std::sync::mpsc::channel();
    let agent_task = registry
        .spawn_extension_lifecycle(
            &agent.id,
            Box::new(move |_| {
                agent_wait
                    .recv_timeout(Duration::from_secs(1))
                    .map_err(|error| crate::backend::runtime::AppError::external(error))?;
                Ok(serde_json::json!({ "domain": "agent" }))
            }),
        )
        .unwrap();
    let adapter_task = registry
        .spawn_extension_lifecycle(
            &adapter.id,
            Box::new(move |_| {
                adapter_wait
                    .recv_timeout(Duration::from_secs(1))
                    .map_err(|error| crate::backend::runtime::AppError::external(error))?;
                Ok(serde_json::json!({ "domain": "conversation" }))
            }),
        )
        .unwrap();

    assert_eq!(
        agent_task.kind,
        crate::backend::runtime::tasks::TaskKind::ExtensionLifecycle
    );
    assert_eq!(
        adapter_task.kind,
        crate::backend::runtime::tasks::TaskKind::ExtensionLifecycle
    );
    assert_eq!(
        runtime
            .list(crate::backend::runtime::tasks::TaskFilter {
                kind: Some(crate::backend::runtime::tasks::TaskKind::ExtensionLifecycle),
                active_only: true,
                ..Default::default()
            })
            .len(),
        2
    );

    agent_release.send(()).unwrap();
    adapter_release.send(()).unwrap();
    for _ in 0..100 {
        if runtime
            .list(crate::backend::runtime::tasks::TaskFilter {
                kind: Some(crate::backend::runtime::tasks::TaskKind::ExtensionLifecycle),
                active_only: true,
                ..Default::default()
            })
            .is_empty()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(runtime
        .list(crate::backend::runtime::tasks::TaskFilter {
            kind: Some(crate::backend::runtime::tasks::TaskKind::ExtensionLifecycle),
            active_only: true,
            ..Default::default()
        })
        .is_empty());

    registry
        .finish_agent_lifecycle(&agent.id, Ok((None, Vec::new())))
        .unwrap();
    registry
        .finish_conversation_script_install(&adapter.id, Ok(serde_json::json!({})))
        .unwrap();
    runtime.shutdown_with_grace(Duration::ZERO).await;
}

#[test]
fn session_and_web_sync_tasks_run_independently() {
    let registry = BackgroundTaskRegistry::default();

    let (session, should_start_session) = registry
        .begin_conversation_sync(&params(Some("session")))
        .unwrap();
    let (web, should_start_web) = registry
        .begin_conversation_sync(&params(Some("web")))
        .unwrap();

    assert!(should_start_session);
    assert!(should_start_web);
    assert_ne!(session.id, web.id);

    registry
        .finish_conversation_sync(&session.id, Ok(serde_json::json!({ "results": [] })))
        .unwrap();
    assert!(registry.has_running_tasks());
}

#[test]
fn full_sync_owns_the_all_record_scope_and_blocks_scoped_syncs() {
    let registry = BackgroundTaskRegistry::default();
    let mut full_params = params(None);
    full_params.mode = ConversationSyncMode::Full;

    let (full, should_start_full) = registry.begin_conversation_sync(&full_params).unwrap();
    let (session, should_start_session) = registry
        .begin_conversation_sync(&params(Some("session")))
        .unwrap();

    assert!(should_start_full);
    assert!(!should_start_session);
    assert_eq!(session.id, full.id);
    assert_eq!(full.record_kind, None);
    assert_eq!(full.mode, ConversationSyncMode::Full);
}

#[test]
fn conversation_sync_progress_tracks_completed_and_current_sources() {
    let registry = BackgroundTaskRegistry::default();
    let (running, _) = registry
        .begin_conversation_sync(&params(Some("session")))
        .unwrap();

    let updated = registry
        .update_conversation_sync_progress(&running.id, 1, 3, Some("Gemini Web".to_string()))
        .unwrap();

    assert_eq!(updated.progress.completed_source_count, 1);
    assert_eq!(updated.progress.total_source_count, 3);
    assert_eq!(
        updated.progress.current_source_name.as_deref(),
        Some("Gemini Web")
    );
}

#[test]
fn finishing_sync_records_success_or_failure() {
    let registry = BackgroundTaskRegistry::default();
    let (running, _) = registry
        .begin_conversation_sync(&params(Some("session")))
        .unwrap();

    let completed = registry
        .finish_conversation_sync(&running.id, Ok(serde_json::json!({ "results": [] })))
        .unwrap();

    assert_eq!(completed.status, BackgroundTaskStatus::Completed);
    assert!(completed.result.is_some());
    assert!(!registry.has_running_tasks());

    let (running, _) = registry
        .begin_conversation_sync(&params(Some("session")))
        .unwrap();
    let failed = registry
        .finish_conversation_sync(
            &running.id,
            Err(crate::backend::runtime::AppError::Domain {
                code: "sync_failed".to_string(),
                message: "sync failed".to_string(),
                retryable: true,
                details: None,
            }),
        )
        .unwrap();

    assert_eq!(failed.status, BackgroundTaskStatus::Failed);
    assert_eq!(
        failed.error.as_ref().map(|error| error.message.as_str()),
        Some("sync failed")
    );
    assert!(!registry.has_running_tasks());
}

#[test]
fn sync_projection_uses_task_runtime_cancellation_before_domain_result() {
    let registry = BackgroundTaskRegistry::default();
    let (running, _) = registry
        .begin_conversation_sync(&params(Some("session")))
        .unwrap();
    let runtime = registry.task_runtime().expect("shared task runtime");
    assert!(matches!(
        runtime.cancel(&running.id),
        crate::backend::runtime::tasks::CancelOutcome::Requested(_)
    ));

    let finished = registry
        .finish_conversation_sync(&running.id, Ok(serde_json::json!({"results": []})))
        .unwrap();

    assert_eq!(finished.status, BackgroundTaskStatus::Cancelled);
    assert_eq!(
        runtime.get(&running.id).unwrap().state,
        crate::backend::runtime::tasks::TaskState::Canceled
    );
}

#[test]
fn lifecycle_projection_does_not_resume_after_runtime_cancellation() {
    let registry = BackgroundTaskRegistry::default();
    let (running, _, _) = registry
        .begin_agent_lifecycle(
            "runtime-cancelled-agent".to_string(),
            "install".to_string(),
            None,
            Some("1.0.0".to_string()),
            None,
            None,
            None,
        )
        .unwrap();
    let runtime = registry.task_runtime().expect("shared task runtime");
    assert!(matches!(
        runtime.cancel(&running.id),
        crate::backend::runtime::tasks::CancelOutcome::Requested(_)
    ));

    let projected = registry
        .update_agent_lifecycle(
            &running.id,
            LifecycleTaskPhase::Downloading,
            7,
            Some(512),
            vec!["late update".to_string()],
        )
        .unwrap();

    assert_eq!(projected.state, LifecycleTaskState::Cancelling);
    assert_eq!(projected.phase, LifecycleTaskPhase::Cancelling);
    assert_eq!(projected.progress.completed_units, 0);
    assert!(projected.warnings.is_empty());
}

#[test]
fn skill_backup_tracks_progress_and_blocks_duplicate_start() {
    let registry = BackgroundTaskRegistry::default();

    let (running, should_start) = registry
        .begin_skill_backup(vec![
            "skill-a".to_string(),
            "skill-a".to_string(),
            " ".to_string(),
            "skill-b".to_string(),
        ])
        .unwrap();
    let (duplicate, should_start_duplicate) = registry
        .begin_skill_backup(vec!["skill-c".to_string()])
        .unwrap();

    assert!(should_start);
    assert!(!should_start_duplicate);
    assert_eq!(running.id, duplicate.id);
    assert_eq!(running.asset_ids, vec!["skill-a", "skill-b"]);
    assert_eq!(running.total_count, 2);
    assert_eq!(running.current_asset_id.as_deref(), Some("skill-a"));
    assert!(registry.has_running_tasks());

    let progress = registry
        .update_skill_backup_progress(&running.id, 1, Some("skill-b".to_string()))
        .unwrap();
    assert_eq!(progress.completed_count, 1);
    assert_eq!(progress.current_asset_id.as_deref(), Some("skill-b"));

    let failed = registry
        .finish_skill_backup(
            &running.id,
            Err(crate::backend::runtime::AppError::External(
                "copy failed".to_string(),
            )),
        )
        .unwrap();
    assert_eq!(failed.status, BackgroundTaskStatus::Failed);
    assert_eq!(failed.failed_count, 1);
    assert_eq!(failed.errors[0].asset_id.as_deref(), Some("skill-b"));
    assert!(!registry.has_running_tasks());

    let completed_copy_registry = BackgroundTaskRegistry::default();
    let (running, _) = completed_copy_registry
        .begin_skill_backup(vec!["skill-a".to_string()])
        .unwrap();
    completed_copy_registry
        .update_skill_backup_progress(&running.id, 1, None)
        .unwrap();
    let refresh_failed = completed_copy_registry
        .finish_skill_backup(
            &running.id,
            Err(crate::backend::runtime::AppError::External(
                "catalog refresh failed".to_string(),
            )),
        )
        .unwrap();
    assert_eq!(refresh_failed.errors[0].asset_id, None);
}

#[test]
fn conversation_script_install_blocks_duplicate_start_and_finishes() {
    let registry = BackgroundTaskRegistry::default();
    let params = ConversationScriptInstallParams {
        catalog_url: Some("https://example.test/catalog.json".to_string()),
        item_id: "codex-session".to_string(),
        dry_run: false,
        yes: true,
    };

    let (running, should_start) = registry
        .begin_conversation_script_install(&params)
        .expect("start install task");
    let (duplicate, should_start_duplicate) = registry
        .begin_conversation_script_install(&ConversationScriptInstallParams {
            item_id: "codex-session".to_string(),
            ..params
        })
        .expect("reuse running install task");

    assert!(should_start);
    assert!(!should_start_duplicate);
    assert_eq!(running.id, duplicate.id);
    assert_eq!(running.item_id, "codex-session");
    assert!(registry.has_running_tasks());

    let completed = registry
        .finish_conversation_script_install(
            &running.id,
            Ok(serde_json::json!({ "installed": true })),
        )
        .expect("finish install task");

    assert_eq!(completed.status, BackgroundTaskStatus::Completed);
    assert_eq!(
        completed.result,
        Some(serde_json::json!({ "installed": true }))
    );
    assert!(!registry.has_running_tasks());
}

#[test]
fn conversation_package_uninstall_uses_the_shared_background_task_registry() {
    let registry = BackgroundTaskRegistry::default();
    let params = ConversationAdapterPackageUninstallParams {
        package_id: "io.github.util6.codex-session".to_string(),
        dry_run: false,
        yes: true,
    };

    let (running, should_start) = registry
        .begin_conversation_adapter_package_uninstall(&params)
        .expect("start uninstall task");

    assert!(should_start);
    assert_eq!(running.action, "uninstall");
    assert_eq!(running.phase.as_deref(), Some("uninstalling"));
    assert!(registry.has_running_tasks());
}

#[test]
fn task_01_02_begin_and_phase_update_preserve_safe_snapshot_state() {
    let registry = BackgroundTaskRegistry::default();
    let (queued, cancellation) = registry
        .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
        .unwrap();
    std::thread::sleep(Duration::from_millis(1));

    let running = registry
        .update_ai_execution_phase(&queued.id, AiExecutionPhase::Initializing)
        .unwrap();

    assert_eq!(queued.state, AiExecutionTaskState::Queued);
    assert_eq!(queued.phase, AiExecutionPhase::Queued);
    assert!(!cancellation.is_cancelled());
    assert_eq!(running.state, AiExecutionTaskState::Running);
    assert_eq!(running.phase, AiExecutionPhase::Initializing);
    assert_ne!(running.updated_at, queued.updated_at);
    assert!(registry.has_running_tasks());
}

#[test]
fn task_03_04_finish_records_public_result_or_stable_error() {
    let registry = BackgroundTaskRegistry::default();
    let (success, _) = registry
        .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
        .unwrap();
    let completed = registry
        .finish_ai_execution(&success.id, Ok(ai_result("translated")))
        .unwrap();
    assert_eq!(completed.state, AiExecutionTaskState::Succeeded);
    assert_eq!(
        completed.result,
        Some(AiExecutionPublicResult {
            text: "translated".to_string()
        })
    );
    assert!(completed.finished_at.is_some());

    let (failure, _) = registry
        .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
        .unwrap();
    registry
        .update_ai_execution_phase(&failure.id, AiExecutionPhase::Prompting)
        .unwrap();
    let failed = registry
        .finish_ai_execution(
            &failure.id,
            Err(AiExecutionError::Protocol {
                operation: "SECRET_OPERATION",
            }),
        )
        .unwrap();
    assert_eq!(failed.state, AiExecutionTaskState::Failed);
    assert_eq!(failed.error.as_ref().unwrap().code, "protocol_failed");
    assert_eq!(
        failed.error.as_ref().unwrap().phase,
        Some(AiExecutionPhase::Prompting)
    );
    assert_eq!(failed.phase, AiExecutionPhase::Prompting);
    assert!(!format!("{failed:?}").contains("SECRET_OPERATION"));
    assert!(!registry.has_running_tasks());
}

#[test]
fn task_05_06_07_cancel_is_active_for_queued_and_running_and_idempotent_for_terminal() {
    let registry = BackgroundTaskRegistry::default();
    let (queued, queued_token) = registry
        .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
        .unwrap();
    let cancelling = registry.cancel_ai_execution(&queued.id).unwrap();
    assert!(queued_token.is_cancelled());
    assert_eq!(cancelling.state, AiExecutionTaskState::Running);
    assert_eq!(cancelling.phase, AiExecutionPhase::Cancelling);
    let cleaning_up = registry
        .update_ai_execution_phase(&queued.id, AiExecutionPhase::CleaningUp)
        .unwrap();
    assert_eq!(cleaning_up.phase, AiExecutionPhase::CleaningUp);
    let cancelled = registry
        .finish_ai_execution(&queued.id, Err(cancelled_error()))
        .unwrap();
    assert_eq!(cancelled.state, AiExecutionTaskState::Cancelled);
    assert_eq!(cancelled.phase, AiExecutionPhase::CleaningUp);
    assert_eq!(
        cancelled.error.as_ref().unwrap().phase,
        Some(AiExecutionPhase::CleaningUp)
    );
    assert_eq!(registry.cancel_ai_execution(&queued.id).unwrap(), cancelled);

    let (running, running_token) = registry
        .begin_ai_execution(AiExecutionPurpose::ConnectionTest, &opencode_id())
        .unwrap();
    registry
        .update_ai_execution_phase(&running.id, AiExecutionPhase::Prompting)
        .unwrap();
    registry.cancel_ai_execution(&running.id).unwrap();
    assert!(running_token.is_cancelled());
}

#[test]
fn task_08_unknown_cancel_returns_not_found() {
    let registry = BackgroundTaskRegistry::default();

    let error = registry.cancel_ai_execution("missing").unwrap_err();

    assert!(error.to_string().contains("not found"));
    assert_eq!(registry.ai_execution_snapshot("missing").unwrap(), None);
}

#[test]
fn task_09_list_snapshot_has_no_prompt_workspace_environment_or_stderr_fields() {
    let registry = BackgroundTaskRegistry::default();
    let _ = registry
        .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
        .unwrap();

    let serialized = serde_json::to_string(&registry.ai_execution_snapshots().unwrap()).unwrap();

    for forbidden in ["prompt", "workspace", "cwd", "environment", "stderr"] {
        assert!(!serialized.contains(forbidden), "leaked field: {forbidden}");
    }
}

#[test]
fn task_runtime_result_is_authoritative_and_not_duplicated_in_projection_detail() {
    let registry = BackgroundTaskRegistry::default();
    let (task, _) = registry
        .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
        .unwrap();
    let large_text = "large-result-".to_string() + &"x".repeat(4096);
    registry
        .finish_ai_execution(
            &task.id,
            Ok(AiExecutionResult {
                text: large_text.clone(),
                ..ai_result("unused")
            }),
        )
        .unwrap();

    let runtime = registry.task_runtime().unwrap();
    let runtime_snapshot = runtime.get(&task.id).expect("runtime snapshot");
    let detail = serde_json::to_string(&runtime_snapshot.detail).unwrap();
    assert!(!detail.contains(&large_text));
    assert_eq!(runtime_snapshot.result.unwrap()["text"], large_text);
    assert_eq!(
        registry
            .ai_execution_snapshot(&task.id)
            .unwrap()
            .unwrap()
            .result
            .unwrap()
            .text,
        large_text
    );
}

#[test]
fn task_10_time_retention_prunes_expired_terminal_tasks() {
    let registry = BackgroundTaskRegistry::default();
    let (task, _) = registry
        .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
        .unwrap();
    registry
        .finish_ai_execution(&task.id, Ok(ai_result("done")))
        .unwrap();
    let runtime = registry.task_runtime().unwrap();
    runtime.set_user_visible_for_test(&task.id, false).unwrap();
    runtime
        .set_finished_at_for_test(
            &task.id,
            (Utc::now()
                - chrono::Duration::from_std(
                    crate::backend::runtime::tasks::TASK_TERMINAL_RETENTION,
                )
                .unwrap()
                - chrono::Duration::seconds(1))
            .to_rfc3339(),
        )
        .unwrap();

    assert!(registry.ai_execution_snapshots().unwrap().is_empty());
}

#[test]
fn task_11_12_count_retention_keeps_50_terminal_tasks_and_all_running_tasks() {
    let registry = BackgroundTaskRegistry::default();
    let (running, _) = registry
        .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
        .unwrap();
    for index in 0..51 {
        let (task, _) = registry
            .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
            .unwrap();
        registry
            .finish_ai_execution(&task.id, Ok(ai_result(&format!("result-{index}"))))
            .unwrap();
    }

    let snapshots = registry.ai_execution_snapshots().unwrap();
    assert_eq!(
        snapshots
            .iter()
            .filter(|snapshot| snapshot.state.is_terminal())
            .count(),
        crate::backend::runtime::tasks::TASK_TERMINAL_LIMIT
    );
    assert!(snapshots.iter().any(|snapshot| snapshot.id == running.id));
}

#[test]
fn task_13_14_ai_tasks_count_as_running_and_cancel_all_sets_every_token() {
    let registry = BackgroundTaskRegistry::default();
    let (first, first_token) = registry
        .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
        .unwrap();
    let (_second, second_token) = registry
        .begin_ai_execution(AiExecutionPurpose::ConnectionTest, &opencode_id())
        .unwrap();
    assert!(registry.has_running_tasks());

    let snapshots = registry.cancel_all_ai_executions().unwrap();

    assert_eq!(snapshots.len(), 2);
    assert!(first_token.is_cancelled());
    assert!(second_token.is_cancelled());
    assert_eq!(
        registry
            .ai_execution_snapshot(&first.id)
            .unwrap()
            .unwrap()
            .phase,
        AiExecutionPhase::Cancelling
    );
}

#[tokio::test]
async fn tauri_07_08_app_close_cancels_all_ai_tasks_and_waits_for_cleanup() {
    let registry = std::sync::Arc::new(BackgroundTaskRegistry::default());
    let (task, token) = registry
        .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
        .unwrap();
    let finisher = registry.clone();
    let task_id = task.id.clone();
    tokio::spawn(async move {
        token.cancelled().await;
        finisher
            .finish_ai_execution(&task_id, Err(cancelled_error()))
            .unwrap();
    });

    let report = registry
        .cancel_ai_executions_and_wait(
            std::time::Duration::from_secs(1),
            std::time::Duration::from_millis(5),
        )
        .await
        .unwrap();

    assert_eq!(report.cancelled_count, 1);
    assert_eq!(report.remaining_count, 0);
    assert!(report.converged);
    assert!(!registry.has_running_tasks());
}

#[tokio::test]
async fn tauri_08_app_close_wait_is_bounded_and_reports_pending_cleanup() {
    let registry = BackgroundTaskRegistry::default();
    registry
        .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
        .unwrap();

    let report = registry
        .cancel_ai_executions_and_wait(
            std::time::Duration::from_millis(25),
            std::time::Duration::from_millis(5),
        )
        .await
        .unwrap();

    assert_eq!(report.cancelled_count, 1);
    assert_eq!(report.remaining_count, 1);
    assert!(!report.converged);
}

#[test]
fn agent_lifecycle_task_deduplicates_same_agent_and_finishes_with_stable_state() {
    let registry = BackgroundTaskRegistry::default();
    let (first, cancellation, should_start) = registry
        .begin_agent_lifecycle(
            "fixture-agent".to_string(),
            "install".to_string(),
            Some("catalog-v1".to_string()),
            Some("1.0.0".to_string()),
            Some("fixture-system".to_string()),
            None,
            None,
        )
        .unwrap();
    let (duplicate, duplicate_cancellation, duplicate_should_start) = registry
        .begin_agent_lifecycle(
            "fixture-agent".to_string(),
            "install".to_string(),
            Some("catalog-v1".to_string()),
            Some("1.0.0".to_string()),
            Some("fixture-system".to_string()),
            None,
            None,
        )
        .unwrap();

    assert!(should_start);
    assert!(!duplicate_should_start);
    assert_eq!(first.id, duplicate.id);
    let running = registry
        .update_agent_lifecycle(
            &first.id,
            LifecycleTaskPhase::Downloading,
            2,
            Some(128),
            vec!["fixture warning".to_string()],
        )
        .unwrap();
    assert_eq!(running.state, LifecycleTaskState::Running);
    assert_eq!(running.progress.completed_units, 2);
    assert_eq!(running.progress.downloaded_bytes, Some(128));

    let cancelled = registry.cancel_agent_lifecycle(&first.id).unwrap();
    assert_eq!(cancelled.state, LifecycleTaskState::Cancelling);
    assert_eq!(cancelled.phase, LifecycleTaskPhase::Cancelling);
    assert!(!cancelled.state.is_terminal());
    assert_eq!(
        registry
            .task_runtime()
            .unwrap()
            .get(&first.id)
            .unwrap()
            .state,
        TaskState::Cancelling
    );
    assert!(cancellation.is_cancelled());
    assert!(duplicate_cancellation.is_cancelled());
    let terminal = registry
        .finish_agent_lifecycle(&first.id, Ok((None, Vec::new())))
        .unwrap();
    assert_eq!(terminal.state, LifecycleTaskState::Cancelled);
    assert_eq!(terminal.phase, LifecycleTaskPhase::Cancelled);
    assert!(!terminal.cancellable);
    assert!(!registry.has_running_tasks());
}

#[test]
fn different_agents_can_reserve_lifecycle_tasks_concurrently() {
    let registry = BackgroundTaskRegistry::default();
    let begin = |agent_id: &str| {
        registry
            .begin_agent_lifecycle(
                agent_id.to_string(),
                "install".to_string(),
                Some("catalog-v1".to_string()),
                Some("1.0.0".to_string()),
                Some("fixture-system".to_string()),
                None,
                None,
            )
            .unwrap()
    };

    let (first, _, first_should_start) = begin("fixture-agent-a");
    let (second, _, second_should_start) = begin("fixture-agent-b");

    assert!(first_should_start);
    assert!(second_should_start);
    assert_ne!(first.id, second.id);
    let active = registry
        .agent_lifecycle_snapshots()
        .unwrap()
        .into_iter()
        .filter(|task| !task.state.is_terminal())
        .collect::<Vec<_>>();
    assert_eq!(active.len(), 2);
    assert_eq!(
        active
            .iter()
            .map(|task| task.agent_id.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["fixture-agent-a", "fixture-agent-b"])
    );
}

#[test]
fn application_owned_agent_lifecycle_tasks_conflict_across_tenants() {
    let registry = BackgroundTaskRegistry::default();
    let begin = |tenant_id: &str| {
        registry
            .begin_agent_lifecycle_for_tenant(
                tenant_id,
                "fixture-agent".to_string(),
                "install".to_string(),
                None,
                Some("1.0.0".to_string()),
                Some("fixture-system".to_string()),
                None,
                None,
            )
            .unwrap()
    };

    let (agent_a, _, started_a) = begin("tenant-a");
    let (agent_b, _, started_b) = begin("tenant-b");

    assert!(started_a);
    assert!(!started_b);
    assert_eq!(agent_a.id, agent_b.id);
    assert_eq!(
        registry
            .agent_lifecycle_snapshots_for_tenant("tenant-a")
            .unwrap()
            .iter()
            .map(|snapshot| snapshot.id.as_str())
            .collect::<Vec<_>>(),
        vec![agent_a.id.as_str()]
    );
    assert_eq!(
        registry
            .agent_lifecycle_snapshots_for_tenant("tenant-b")
            .unwrap()
            .iter()
            .map(|snapshot| snapshot.id.as_str())
            .collect::<Vec<_>>(),
        vec![agent_a.id.as_str()]
    );
    assert_eq!(
        registry
            .agent_lifecycle_snapshot_for_tenant("tenant-b", &agent_a.id)
            .unwrap()
            .id,
        agent_a.id
    );
    assert_eq!(
        registry
            .cancel_agent_lifecycle_for_tenant("tenant-b", &agent_a.id)
            .unwrap()
            .state,
        LifecycleTaskState::Cancelling
    );
}

#[test]
fn application_owned_conversation_script_tasks_conflict_across_tenants() {
    let registry = BackgroundTaskRegistry::default();
    let params = ConversationAdapterPackageInstallParams {
        catalog_url: None,
        package_id: "fixture-package".to_string(),
        version: Some("1.0.0".to_string()),
        dry_run: false,
        yes: true,
    };

    let (tenant_a, started_a) = registry
        .begin_conversation_adapter_package_install_for_tenant("tenant-a", &params)
        .unwrap();
    let (tenant_b, started_b) = registry
        .begin_conversation_adapter_package_install_for_tenant("tenant-b", &params)
        .unwrap();

    assert!(started_a);
    assert!(!started_b);
    assert_eq!(tenant_a.id, tenant_b.id);
    assert_eq!(
        registry
            .conversation_script_install_snapshot_for_tenant("tenant-b")
            .unwrap()
            .expect("global task is visible after a tenant switch")
            .id,
        tenant_a.id
    );
}

#[test]
fn lifecycle_projection_does_not_return_another_operation_snapshot() {
    let registry = BackgroundTaskRegistry::default();
    registry
        .begin_agent_lifecycle(
            "fixture-agent".to_string(),
            "install".to_string(),
            None,
            Some("1.0.0".to_string()),
            None,
            None,
            None,
        )
        .unwrap();

    let error = registry
        .begin_agent_lifecycle(
            "fixture-agent".to_string(),
            "update".to_string(),
            None,
            Some("2.0.0".to_string()),
            None,
            None,
            None,
        )
        .expect_err("a different active operation must not reuse the install task");
    assert!(error.to_string().contains("conflicts"));
}

#[test]
fn agent_market_refresh_deduplicates_running_task_and_retains_terminal_snapshot() {
    let registry = BackgroundTaskRegistry::default();
    let (first, should_start) = registry.begin_agent_market_refresh().unwrap();
    let (duplicate, duplicate_should_start) = registry.begin_agent_market_refresh().unwrap();
    assert!(should_start);
    assert!(!duplicate_should_start);
    assert_eq!(first.id, duplicate.id);

    let result = AgentMarketRefreshResult {
        status: "updated".to_string(),
        catalog_version: "catalog-v1".to_string(),
        active_catalog_version: "catalog-v1".to_string(),
        downloaded_catalog_version: "catalog-v1".to_string(),
        item_count: 1,
        source: "bundled".to_string(),
        etag: None,
    };
    let finished = registry
        .finish_agent_market_refresh(&first.id, Ok(result.clone()))
        .unwrap();
    assert_eq!(finished.state, AgentMarketRefreshTaskState::Succeeded);
    assert_eq!(finished.result, Some(result));
    assert_eq!(registry.agent_market_refresh_snapshots().unwrap().len(), 1);
}

#[test]
fn task_runtime_deletion_removes_running_authority_from_the_projection() {
    let registry = BackgroundTaskRegistry::default();
    let (task, _) = registry
        .begin_conversation_sync(&params(Some("session")))
        .unwrap();
    let runtime = registry.task_runtime().expect("shared task runtime");

    assert!(registry.has_running_tasks());
    assert!(runtime.remove(&task.id).is_some());
    assert!(!registry.has_running_tasks());

    let (replacement, should_start) = registry
        .begin_conversation_sync(&params(Some("session")))
        .unwrap();
    assert!(should_start);
    assert_ne!(replacement.id, task.id);
}

#[test]
fn projection_getter_returns_not_found_after_runtime_deletion() {
    let registry = BackgroundTaskRegistry::default();
    let (task, _) = registry
        .begin_conversation_sync(&params(Some("session")))
        .unwrap();
    let runtime = registry.task_runtime().expect("shared task runtime");

    assert!(runtime.remove(&task.id).is_some());
    assert!(registry.conversation_sync_snapshot().unwrap().is_none());
    assert!(registry.conversation_sync_snapshots().unwrap().is_empty());
    assert!(registry
        .update_conversation_sync_progress(&task.id, 1, 1, None)
        .is_err());
}

#[test]
fn optional_projection_getters_surface_decode_errors_instead_of_not_found() {
    let registry = BackgroundTaskRegistry::default();
    let (ai_task, _) = registry
        .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
        .unwrap();
    let runtime = registry.task_runtime().expect("shared task runtime");

    runtime
        .update_detail(&ai_task.id, serde_json::json!("malformed-ai-projection"))
        .unwrap();
    let ai_error = registry.ai_execution_snapshot(&ai_task.id).unwrap_err();
    assert!(ai_error.to_string().contains("could not be decoded"));
}

#[test]
fn all_projection_lists_drop_orphan_running_entries() {
    let registry = BackgroundTaskRegistry::default();
    let (agent, _, _) = registry
        .begin_agent_lifecycle(
            "orphan-agent".to_string(),
            "install".to_string(),
            None,
            Some("1.0.0".to_string()),
            None,
            None,
            None,
        )
        .unwrap();
    let (market, _) = registry.begin_agent_market_refresh().unwrap();
    let (search, _) = registry.begin_conversation_search_index_rebuild().unwrap();
    let (sync, _) = registry
        .begin_conversation_sync(&params(Some("session")))
        .unwrap();
    let (script, _) = registry
        .begin_conversation_script_install(&ConversationScriptInstallParams {
            catalog_url: None,
            item_id: "orphan-script".to_string(),
            dry_run: false,
            yes: true,
        })
        .unwrap();
    let (backup, _) = registry
        .begin_skill_backup(vec!["orphan-skill".to_string()])
        .unwrap();
    let (ai, _) = registry
        .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
        .unwrap();
    let runtime = registry.task_runtime().expect("shared task runtime");

    for task_id in [
        agent.id.as_str(),
        market.id.as_str(),
        search.id.as_str(),
        sync.id.as_str(),
        script.id.as_str(),
        backup.id.as_str(),
        ai.id.as_str(),
    ] {
        assert!(runtime.remove(task_id).is_some(), "remove {task_id}");
    }

    assert!(registry.agent_lifecycle_snapshots().unwrap().is_empty());
    assert!(registry
        .agent_market_refresh_snapshots()
        .unwrap()
        .is_empty());
    assert!(registry
        .conversation_search_index_snapshot()
        .unwrap()
        .is_none());
    assert!(registry.conversation_sync_snapshots().unwrap().is_empty());
    assert!(registry
        .conversation_script_install_snapshot()
        .unwrap()
        .is_none());
    assert!(registry.skill_backup_snapshot().unwrap().is_none());
    assert!(registry.ai_execution_snapshots().unwrap().is_empty());
}

fn opencode_id() -> AgentId {
    AgentId::parse("opencode").unwrap()
}

fn ai_result(text: &str) -> AiExecutionResult {
    AiExecutionResult {
        text: text.to_string(),
        agent_id: opencode_id(),
        protocol: crate::backend::agents::types::AgentProtocol::Acp,
        requested_model: None,
        elapsed_ms: 1,
        persistent_binding: None,
        replay_text: None,
        session_cleanup: crate::backend::ai_execution::SessionCleanupStatus::Deleted,
    }
}

fn cancelled_error() -> AiExecutionError {
    AiExecutionError::Cancelled {
        program: std::path::PathBuf::from("opencode"),
    }
}

impl BackgroundTaskRegistry {
    pub(crate) fn begin_agent_lifecycle_for_tenant(
        &self,
        _tenant_id: &str,
        agent_id: String,
        action: String,
        catalog_version: Option<String>,
        agent_version: Option<String>,
        distribution_id: Option<String>,
        distribution_type: Option<crate::backend::agent_market::types::DistributionType>,
        ownership: Option<crate::backend::agent_market::types::Ownership>,
    ) -> AppResult<(
        AgentLifecycleTaskSnapshot,
        tokio_util::sync::CancellationToken,
        bool,
    )> {
        self.begin_agent_lifecycle(
            agent_id,
            action,
            catalog_version,
            agent_version,
            distribution_id,
            distribution_type,
            ownership,
        )
    }
    pub(crate) fn agent_lifecycle_snapshot_for_tenant(
        &self,
        _tenant_id: &str,
        task_id: &str,
    ) -> AppResult<AgentLifecycleTaskSnapshot> {
        self.projection(task_id)
    }
    pub(crate) fn agent_lifecycle_snapshots_for_tenant(
        &self,
        _tenant_id: &str,
    ) -> AppResult<Vec<AgentLifecycleTaskSnapshot>> {
        self.agent_lifecycle_snapshots()
    }
    pub(crate) fn cancel_agent_lifecycle_for_tenant(
        &self,
        _tenant_id: &str,
        task_id: &str,
    ) -> AppResult<AgentLifecycleTaskSnapshot> {
        self.cancel_agent_lifecycle(task_id)
    }
    pub(crate) fn begin_conversation_search_index_rebuild(
        &self,
    ) -> AppResult<(ConversationSearchIndexTaskSnapshot, bool)> {
        self.begin_conversation_search_index_rebuild_for_tenant("default")
    }
    pub(crate) fn begin_conversation_sync(
        &self,
        params: &ConversationSyncParams,
    ) -> AppResult<(ConversationSyncTaskSnapshot, bool)> {
        self.begin_conversation_sync_for_tenant("default", params)
    }
    pub(crate) fn begin_conversation_adapter_package_install_for_tenant(
        &self,
        _tenant_id: &str,
        params: &ConversationAdapterPackageInstallParams,
    ) -> AppResult<(ConversationScriptInstallTaskSnapshot, bool)> {
        self.begin_conversation_adapter_package_install(params)
    }
    pub(crate) fn conversation_script_install_snapshot_for_tenant(
        &self,
        _tenant_id: &str,
    ) -> AppResult<Option<ConversationScriptInstallTaskSnapshot>> {
        self.conversation_script_install_snapshot()
    }
    pub(crate) fn begin_skill_backup(
        &self,
        asset_ids: Vec<String>,
    ) -> AppResult<(SkillBackupTaskSnapshot, bool)> {
        self.begin_skill_backup_for_tenant("default", asset_ids)
    }
    pub(crate) fn begin_ai_execution(
        &self,
        purpose: AiExecutionPurpose,
        agent_id: &AgentId,
    ) -> AppResult<(AiExecutionTaskSnapshot, AiExecutionCancellation)> {
        self.begin_ai_execution_for_tenant("default", purpose, agent_id)
    }
}
