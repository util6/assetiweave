use super::*;

#[test]
fn test_project_path_to_slug() {
    assert_eq!(project_path_to_slug("/code/my-project"), "code-my-project");
    assert_eq!(
        project_path_to_slug("C:\\Users\\admin\\repo"),
        "c-users-admin-repo"
    );
    assert_eq!(project_path_to_slug("///"), "unspecified");
    assert_eq!(project_path_to_slug("   "), "unspecified");
}

#[test]
fn test_leaf_ablation_logic() {
    let existing_leaves = vec![
        "- [s1#T1](aiwc://session/s1/turn/1): explore redis caching options".to_string(),
        "- [s1#T2](aiwc://session/s1/turn/2): explore grpc benchmark".to_string(),
    ];
    let completed_tasks = vec![
        "- [s2#T1](aiwc://session/s2/turn/1): implemented redis caching and verified".to_string(),
    ];
    let new_leaves =
        vec!["- [s2#T2](aiwc://session/s2/turn/2): check sqlite wal mode performance".to_string()];

    let remaining = ablate_and_merge_leaves(&existing_leaves, &new_leaves, &completed_tasks);

    // Redis caching leaf should be ablated (removed) because it's now completed!
    assert!(!remaining.iter().any(|l| l.contains("redis caching")));
    // GRPC benchmark was not touched, should remain
    assert!(remaining.iter().any(|l| l.contains("grpc benchmark")));
    // New WAL leaf should be added
    assert!(remaining.iter().any(|l| l.contains("sqlite wal mode")));
}
