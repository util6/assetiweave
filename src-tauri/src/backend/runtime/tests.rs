use super::*;
use std::{collections::BTreeSet, sync::Arc, thread, time::Duration};

#[test]
fn plan_scope_lock_orders_keys_and_blocks_conflicts() {
    let locks = Arc::new(RuntimeLocks::default());
    let mut keys = BTreeSet::new();
    keys.insert("profile:b".to_string());
    keys.insert("path:a".to_string());
    let guard = locks.acquire_plan_scope(keys).expect("scope");
    let other = locks.clone();
    let joined = thread::spawn(move || {
        let mut keys = BTreeSet::new();
        keys.insert("path:a".to_string());
        let _guard = other.acquire_plan_scope(keys).expect("other scope");
    });
    thread::sleep(Duration::from_millis(20));
    assert!(!joined.is_finished());
    drop(guard);
    joined.join().expect("joined");
}

#[test]
fn task_runtime_deduplicates_and_cancels_cooperatively() {
    let tasks = tasks::TaskRuntime::new();
    let outcome = tasks
        .spawn(
            tasks::TaskSpec::new(tasks::TaskKind::Scan, Some("source-1".to_string())),
            Box::new(|context| {
                while !context.is_cancelled() {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(AppError::Canceled("cancelled".to_string()))
            }),
        )
        .expect("spawn");
    let id = match outcome {
        tasks::SpawnOutcome::Started(snapshot) => snapshot.task_id,
        tasks::SpawnOutcome::Existing(_) => panic!("first task deduplicated"),
    };
    let second = tasks
        .spawn(
            tasks::TaskSpec::new(tasks::TaskKind::Scan, Some("source-1".to_string())),
            Box::new(|_| Ok(serde_json::Value::Null)),
        )
        .expect("dedup");
    assert!(matches!(second, tasks::SpawnOutcome::Existing(_)));
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
        .expect("register external task")
        .expect("first task must be accepted");
    let duplicate = tasks
        .register_external(
            tasks::TaskSpec::new(tasks::TaskKind::Other, Some("same-key".to_string()))
                .with_task_id("different-task"),
        )
        .expect("deduplicate external task");
    assert_eq!(
        duplicate
            .expect_err("deduplication must return existing task")
            .task_id,
        "same-task"
    );

    let same_id = tasks
        .register_external(
            tasks::TaskSpec::new(tasks::TaskKind::Scan, None).with_task_id("same-task"),
        )
        .expect("same id check");
    assert_eq!(
        same_id
            .expect_err("task ids must never be replaced")
            .task_id,
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
    let finished = tasks
        .complete_external("same-task", Ok(serde_json::json!({"done": true})))
        .expect("complete external task");
    assert_eq!(finished.state, tasks::TaskState::Canceled);
    assert!(!tasks.has_active_tasks());
}

#[test]
fn task_runtime_shutdown_is_bounded_and_reports_unfinished_tasks() {
    let tasks = tasks::TaskRuntime::new();
    let outcome = tasks
        .spawn(
            tasks::TaskSpec::new(tasks::TaskKind::Backup, Some("slow".to_string())),
            Box::new(|_| {
                std::thread::sleep(Duration::from_millis(150));
                Ok(serde_json::Value::Null)
            }),
        )
        .expect("spawn slow task");
    let task_id = match outcome {
        tasks::SpawnOutcome::Started(snapshot) => snapshot.task_id,
        tasks::SpawnOutcome::Existing(_) => panic!("slow task unexpectedly deduplicated"),
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
                tasks::TaskState::Pending | tasks::TaskState::Running
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
