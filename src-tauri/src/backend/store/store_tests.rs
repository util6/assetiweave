#[test]
fn stable_sqlx_repositories_have_zero_positional_try_get() {
    let files = [
        (
            "global_memory_repo.rs",
            include_str!("global_memory_repo.rs"),
        ),
        (
            "project_memory_repo.rs",
            include_str!("project_memory_repo.rs"),
        ),
        ("search_index_repo.rs", include_str!("search_index_repo.rs")),
        (
            "memory_recall_repo.rs",
            include_str!("memory_recall_repo.rs"),
        ),
        ("menu_repo.rs", include_str!("menu_repo.rs")),
    ];
    let re = regex::Regex::new(r#"\.try_get(?:::<[^>]+>)?\s*\("#).unwrap();
    let mut violations = Vec::new();
    for (name, content) in files {
        let count = re.find_iter(content).count();
        if count > 0 {
            violations.push(format!("{name}: {count} try_get calls remaining"));
        }
    }
    assert!(
        violations.is_empty(),
        "Stable SQLx repositories must have zero try_get calls:\n{}",
        violations.join("\n")
    );
}
