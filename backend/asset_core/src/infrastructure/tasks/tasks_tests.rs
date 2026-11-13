use super::*;

#[test]
fn prepare_spawn_scopes_conflict_keys_by_tenant_id() {
    let runtime = TaskRuntime::new();

    let spec_tenant_a = TaskSpec::new(TaskKind::Scan, None)
        .with_tenant_id("tenant-a")
        .with_task_id("task-a-1")
        .with_conflict_policy(ConflictPolicy::Reject)
        .with_conflict_keys(["source:shared-path"]);
    let outcome_a = runtime
        .prepare_spawn(spec_tenant_a)
        .expect("tenant-a spawn should succeed");
    assert!(outcome_a.is_some());

    // 不同租户相同 conflict_key 不应冲突
    let spec_tenant_b = TaskSpec::new(TaskKind::Scan, None)
        .with_tenant_id("tenant-b")
        .with_task_id("task-b-1")
        .with_conflict_policy(ConflictPolicy::Reject)
        .with_conflict_keys(["source:shared-path"]);
    let outcome_b = runtime
        .prepare_spawn(spec_tenant_b)
        .expect("tenant-b with same conflict_key should not conflict with tenant-a");
    assert!(outcome_b.is_some());

    // 同租户相同 conflict_key 应当触发冲突拒绝
    let spec_tenant_a_dup = TaskSpec::new(TaskKind::Scan, None)
        .with_tenant_id("tenant-a")
        .with_task_id("task-a-2")
        .with_conflict_policy(ConflictPolicy::Reject)
        .with_conflict_keys(["source:shared-path"]);
    let outcome_a_dup = runtime
        .prepare_spawn(spec_tenant_a_dup)
        .expect("conflict_policy::Reject returns Ok(None)");
    assert!(outcome_a_dup.is_none());
}

#[test]
fn shadow_admission_forces_user_visible_false_in_spawn_and_external() {
    let runtime = TaskRuntime::new();

    let shadow_spawn = TaskSpec::new(TaskKind::Scan, None)
        .with_task_id("shadow-spawn-1")
        .with_admission(TaskAdmission::Shadow)
        .with_user_visible(true);
    let prepared = runtime
        .prepare_spawn(shadow_spawn)
        .expect("shadow spawn succeeds");
    assert!(prepared.is_some());
    assert!(
        !runtime.get("shadow-spawn-1").unwrap().user_visible,
        "prepare_spawn must force user_visible=false when admission=Shadow"
    );

    let shadow_ext = TaskSpec::new(TaskKind::ConversationSync, None)
        .with_task_id("shadow-ext-1")
        .with_admission(TaskAdmission::Shadow)
        .with_user_visible(true);
    let ext_outcome = runtime
        .register_external(shadow_ext)
        .expect("shadow register_external succeeds");
    match ext_outcome {
        ExternalRegistrationOutcome::Started(snap) => {
            assert!(
                !snap.user_visible,
                "register_external must force user_visible=false when admission=Shadow"
            );
        }
        _ => panic!("expected Started outcome"),
    }
}

#[test]
fn task_runtime_metrics_tracks_sliding_window_percentiles_and_recent_errors() {
    let runtime = TaskRuntime::new();

    for idx in 0..105usize {
        let task_id = format!("metric-task-{idx}");
        let spec = TaskSpec::new(TaskKind::Scan, None)
            .with_task_id(&task_id)
            .with_admission(TaskAdmission::Shadow);
        let (_, cancel, _) = runtime
            .prepare_spawn(spec)
            .expect("spawn ok")
            .expect("tuple");

        if idx % 10 == 9 {
            runtime.finish_task(
                &task_id,
                &cancel,
                Err(
                    crate::backend::infrastructure::InfraError::External("boom".to_string()).view(),
                ),
            );
        } else {
            runtime.finish_task(&task_id, &cancel, Ok(serde_json::json!({ "ok": true })));
        }
    }

    let metrics = runtime.metrics_snapshot();
    let scan_stats = metrics
        .iter()
        .find(|m| m.kind == TaskKind::Scan)
        .expect("Scan metrics should exist");

    assert_eq!(scan_stats.sample_count, 100);
    assert_eq!(
        scan_stats.succeeded_count + scan_stats.failed_count + scan_stats.canceled_count,
        100
    );
    assert!(scan_stats.failed_count >= 10);
    assert!(scan_stats.p50_duration_ms <= scan_stats.p95_duration_ms);
    assert!(scan_stats
        .recent_error_codes
        .iter()
        .any(|c| c == "external_error"));
}
