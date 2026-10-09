use super::*;

#[test]
fn git_source_pull_lifecycle_tracks_progress_and_completion() {
    let registry = BackgroundTaskRegistry::default();
    let source_ids = vec!["source-1".to_string(), "source-2".to_string()];

    let (task, started) = registry
        .begin_git_source_pull("default", source_ids.clone())
        .expect("begin git source pull");
    assert!(started);
    assert_eq!(task.status, BackgroundTaskStatus::Running);
    assert_eq!(task.progress.total_count, 2);
    assert_eq!(task.progress.completed_count, 0);

    // 重复注册同一个源返回已有任务或冲突
    let (second, second_started) = registry
        .begin_git_source_pull("default", source_ids.clone())
        .expect("begin second");
    assert!(!second_started);
    assert_eq!(second.status, BackgroundTaskStatus::Running);

    // 更新进度
    let updated = registry
        .update_git_source_pull_progress(
            &task.id,
            1,
            2,
            Some("Repo Alpha".to_string()),
            Some(GitSourcePullItemResult {
                source_id: "source-1".to_string(),
                source_name: "Repo Alpha".to_string(),
                success: true,
                changed: false,
                message: "Already up to date".to_string(),
            }),
        )
        .expect("update progress");
    assert_eq!(updated.progress.completed_count, 1);
    assert_eq!(updated.results.len(), 1);

    // 完成任务
    let finished = registry
        .finish_git_source_pull(
            &task.id,
            vec![
                GitSourcePullItemResult {
                    source_id: "source-1".to_string(),
                    source_name: "Repo Alpha".to_string(),
                    success: true,
                    changed: false,
                    message: "Already up to date".to_string(),
                },
                GitSourcePullItemResult {
                    source_id: "source-2".to_string(),
                    source_name: "Repo Beta".to_string(),
                    success: true,
                    changed: true,
                    message: "Fast-forward 3 commits".to_string(),
                },
            ],
            None,
        )
        .expect("finish git pull");
    assert_eq!(finished.status, BackgroundTaskStatus::Completed);
    assert_eq!(finished.results.len(), 2);
    assert!(finished.finished_at.is_some());
}
