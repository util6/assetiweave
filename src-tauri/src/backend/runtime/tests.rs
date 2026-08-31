use super::*;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

#[test]
fn task_runtime_deduplicates_and_cancels_cooperatively() {
    let tasks = tasks::TaskRuntime::new();
    let outcome = tasks
        .spawn(
            tasks::TaskSpec::new(tasks::TaskKind::Scan, Some("source-1".to_string()))
                .with_task_id("scan-task"),
            Box::new(|context| {
                while !context.is_cancelled() {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(AppError::Canceled("cancelled".to_string()))
            }),
        )
        .expect("spawn");
    let id = match outcome {
        tasks::SpawnOutcome::Started => "scan-task".to_string(),
        tasks::SpawnOutcome::Existing => panic!("first task deduplicated"),
    };
    let second = tasks
        .spawn(
            tasks::TaskSpec::new(tasks::TaskKind::Scan, Some("source-1".to_string())),
            Box::new(|_| Ok(serde_json::Value::Null)),
        )
        .expect("dedup");
    assert!(matches!(second, tasks::SpawnOutcome::Existing));
    assert!(matches!(
        tasks.cancel(&id),
        tasks::CancelOutcome::Requested(_)
    ));
}

#[test]
fn external_task_runtime_owns_id_deduplication_and_terminal_state() {
    let tasks = tasks::TaskRuntime::new();
    let first = tasks
        .register_external(
            tasks::TaskSpec::new(tasks::TaskKind::Other, Some("same-key".to_string()))
                .with_task_id("same-task"),
        )
        .expect("register external task");
    let first = match first {
        tasks::ExternalRegistrationOutcome::Started(snapshot) => snapshot,
        tasks::ExternalRegistrationOutcome::Existing(_) => panic!("first task deduplicated"),
        tasks::ExternalRegistrationOutcome::Conflict(_) => panic!("first task conflicted"),
    };
    let duplicate = tasks
        .register_external(
            tasks::TaskSpec::new(tasks::TaskKind::Other, Some("same-key".to_string()))
                .with_task_id("different-task"),
        )
        .expect("deduplicate external task");
    assert_eq!(
        match duplicate {
            tasks::ExternalRegistrationOutcome::Existing(snapshot) => snapshot.task_id,
            _ => panic!("deduplication must return existing task"),
        },
        "same-task"
    );

    let same_id = tasks
        .register_external(
            tasks::TaskSpec::new(tasks::TaskKind::Scan, None).with_task_id("same-task"),
        )
        .expect("same id check");
    assert_eq!(
        match same_id {
            tasks::ExternalRegistrationOutcome::Existing(snapshot) => snapshot.task_id,
            _ => panic!("task ids must never be replaced"),
        },
        first.task_id
    );

    tasks
        .start_external("same-task")
        .expect("start external task");
    assert!(tasks
        .list(tasks::TaskFilter {
            kind: None,
            active_only: true,
        })
        .iter()
        .any(|snapshot| snapshot.task_id == "same-task"));
    tasks.cancel("same-task");
    assert_eq!(
        tasks.get("same-task").expect("cancelling task").state,
        tasks::TaskState::Cancelling
    );
    let finished = tasks
        .complete_external("same-task", Ok(serde_json::json!({"done": true})))
        .expect("complete external task");
    assert_eq!(finished.state, tasks::TaskState::Canceled);
    assert!(!tasks.has_active_tasks());
}

#[test]
fn task_runtime_prunes_terminal_tasks_globally_and_preserves_active_tasks() {
    let tasks = tasks::TaskRuntime::new();
    let active = tasks
        .register_external(
            tasks::TaskSpec::new(tasks::TaskKind::Scan, Some("active-scan".to_string()))
                .with_task_id("active-scan"),
        )
        .expect("register active task");
    assert!(matches!(
        active,
        tasks::ExternalRegistrationOutcome::Started(_)
    ));
    tasks
        .start_external("active-scan")
        .expect("start active task");

    for index in 0..101 {
        let task_id = format!("terminal-{index}");
        let kind = if index % 2 == 0 {
            tasks::TaskKind::Scan
        } else {
            tasks::TaskKind::BatchMount
        };
        tasks
            .register_external(tasks::TaskSpec::new(kind, None).with_task_id(task_id.clone()))
            .expect("register terminal task");
        tasks.start_external(&task_id).expect("start terminal task");
        tasks
            .complete_external(&task_id, Ok(serde_json::Value::Null))
            .expect("finish terminal task");
    }

    let snapshots = tasks.list(tasks::TaskFilter::default());
    assert_eq!(
        snapshots
            .iter()
            .filter(|snapshot| snapshot.state.is_terminal())
            .count(),
        100
    );
    assert!(snapshots
        .iter()
        .any(|snapshot| snapshot.task_id == "active-scan"));
    assert!(tasks.get("terminal-0").is_none());
}

#[test]
fn task_runtime_get_prunes_expired_terminal_tasks_before_dedup_and_projection_reads() {
    let tasks = tasks::TaskRuntime::new();
    tasks
        .register_external(
            tasks::TaskSpec::new(tasks::TaskKind::SearchIndexRebuild, Some("rebuild".into()))
                .with_task_id("expired-rebuild"),
        )
        .expect("register task");
    tasks.start_external("expired-rebuild").expect("start task");
    tasks
        .complete_external("expired-rebuild", Ok(serde_json::json!({"done": true})))
        .expect("finish task");
    tasks
        .set_finished_at_for_test(
            "expired-rebuild",
            (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339(),
        )
        .expect("age task");

    assert!(tasks.get("expired-rebuild").is_none());
    let replacement = tasks
        .register_external(
            tasks::TaskSpec::new(tasks::TaskKind::SearchIndexRebuild, Some("rebuild".into()))
                .with_task_id("replacement-rebuild"),
        )
        .expect("register replacement task");
    assert!(matches!(
        replacement,
        tasks::ExternalRegistrationOutcome::Started(_)
    ));
}

#[test]
fn task_runtime_progress_is_bounded_and_monotonic() {
    let tasks = tasks::TaskRuntime::new();
    let registered = tasks
        .register_external(
            tasks::TaskSpec::new(tasks::TaskKind::BatchMount, None).with_task_id("progress-task"),
        )
        .expect("register progress task");
    assert!(matches!(
        registered,
        tasks::ExternalRegistrationOutcome::Started(_)
    ));
    tasks.start_external("progress-task").expect("start task");
    tasks
        .set_progress("progress-task", 1, Some(3), Some("first"))
        .expect("set initial progress");
    assert!(tasks
        .set_progress("progress-task", 0, Some(3), Some("backward"))
        .is_err());
    assert!(tasks
        .set_progress("progress-task", 4, Some(3), Some("overflow"))
        .is_err());
}

#[test]
fn task_runtime_broadcasts_registration_progress_and_terminal_snapshots() {
    let tasks = tasks::TaskRuntime::new();
    let mut events = tasks.subscribe();
    tasks
        .register_external(
            tasks::TaskSpec::new(tasks::TaskKind::TeamRun, Some("team-run-1".into()))
                .with_task_id("team-task-1"),
        )
        .expect("register Team task");
    tasks
        .start_external("team-task-1")
        .expect("start Team task");
    tasks
        .set_progress("team-task-1", 1, Some(2), Some("running"))
        .expect("publish Team progress");
    tasks
        .complete_external("team-task-1", Ok(serde_json::json!({"ok": true})))
        .expect("complete Team task");

    let mut snapshots = Vec::new();
    while let Ok(snapshot) = events.try_recv() {
        snapshots.push(snapshot);
    }
    assert!(snapshots.iter().any(|snapshot| {
        snapshot.task_id == "team-task-1" && snapshot.state == tasks::TaskState::Pending
    }));
    assert!(snapshots.iter().any(|snapshot| {
        snapshot.task_id == "team-task-1"
            && snapshot
                .progress
                .as_ref()
                .is_some_and(|progress| progress.current == 1)
    }));
    assert!(snapshots.iter().any(|snapshot| {
        snapshot.task_id == "team-task-1" && snapshot.state == tasks::TaskState::Succeeded
    }));
}

#[test]
fn external_task_runtime_owns_cross_operation_conflicts() {
    let tasks = tasks::TaskRuntime::new();
    let first = tasks
        .register_external(
            tasks::TaskSpec::new(tasks::TaskKind::ExtensionLifecycle, Some("install".into()))
                .with_task_id("install-task")
                .with_conflict_key("extension:fixture"),
        )
        .expect("register first lifecycle task");
    assert!(matches!(
        first,
        tasks::ExternalRegistrationOutcome::Started(_)
    ));

    let conflict = tasks
        .register_external(
            tasks::TaskSpec::new(tasks::TaskKind::ExtensionLifecycle, Some("remove".into()))
                .with_task_id("remove-task")
                .with_conflict_key("extension:fixture"),
        )
        .expect("register conflicting lifecycle task");
    assert!(matches!(
        conflict,
        tasks::ExternalRegistrationOutcome::Conflict(snapshot)
            if snapshot.task_id == "install-task"
    ));
}

#[test]
fn external_task_runtime_starts_a_registered_task_only_once() {
    let tasks = tasks::TaskRuntime::new();
    tasks
        .register_external(
            tasks::TaskSpec::new(tasks::TaskKind::ExtensionLifecycle, None)
                .with_task_id("once-task"),
        )
        .expect("register task");
    let executions = Arc::new(AtomicUsize::new(0));

    let first_executions = executions.clone();
    tasks
        .start_external_with(
            "once-task",
            serde_json::Value::Null,
            Box::new(move |_| {
                first_executions.fetch_add(1, Ordering::SeqCst);
                Ok(serde_json::Value::Null)
            }),
        )
        .expect("start task");
    let second_executions = executions.clone();
    tasks
        .start_external_with(
            "once-task",
            serde_json::Value::Null,
            Box::new(move |_| {
                second_executions.fetch_add(1, Ordering::SeqCst);
                Ok(serde_json::Value::Null)
            }),
        )
        .expect("reuse started task");

    for _ in 0..100 {
        if !tasks.has_active_tasks() {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(executions.load(Ordering::SeqCst), 1);
}

#[test]
fn task_runtime_shutdown_is_bounded_and_reports_unfinished_tasks() {
    let tasks = tasks::TaskRuntime::new();
    let outcome = tasks
        .spawn(
            tasks::TaskSpec::new(tasks::TaskKind::Backup, Some("slow".to_string()))
                .with_task_id("slow-task"),
            Box::new(|_| {
                std::thread::sleep(Duration::from_millis(150));
                Ok(serde_json::Value::Null)
            }),
        )
        .expect("spawn slow task");
    let task_id = match outcome {
        tasks::SpawnOutcome::Started => "slow-task".to_string(),
        tasks::SpawnOutcome::Existing => panic!("slow task unexpectedly deduplicated"),
    };

    let started = std::time::Instant::now();
    let report = tasks.shutdown_with_grace(Duration::from_millis(20));
    assert!(started.elapsed() < Duration::from_millis(100));
    assert_eq!(report.unfinished_task_ids, vec![task_id]);

    // Let the detached test task converge before the registry is dropped.
    std::thread::sleep(Duration::from_millis(180));
    assert!(tasks
        .list(tasks::TaskFilter::default())
        .iter()
        .all(|snapshot| {
            !matches!(
                snapshot.state,
                tasks::TaskState::Pending
                    | tasks::TaskState::Running
                    | tasks::TaskState::Cancelling
            )
        }));
}

#[test]
fn shutdown_report_without_resident_services_is_clean() {
    let report = super::ShutdownReport::default();

    assert!(report.dispatcher_drained);
    assert_eq!(report.dispatcher_remaining_events, 0);
    assert!(!report.dispatcher_timed_out);
    assert!(report.unfinished_task_ids.is_empty());
}
