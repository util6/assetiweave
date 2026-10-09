use std::fs;
use std::path::{Path, PathBuf};

use super::*;

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn init_git_repo(path: &Path, remote_url: &str) {
    let git_dir = path.join(".git");
    fs::create_dir_all(git_dir.join("objects")).expect("create .git/objects dir");
    fs::create_dir_all(git_dir.join("refs")).expect("create .git/refs dir");
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").expect("write HEAD");
    let config = format!(
        "[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = false\n[remote \"origin\"]\n\turl = {}\n",
        remote_url
    );
    fs::write(git_dir.join("config"), config).expect("write git config");
}

#[test]
fn probe_single_git_repo_detects_git_origin_and_skills() {
    let root = unique_temp_dir("probe-single-repo");
    init_git_repo(&root, "https://github.com/example/my-skill-repo.git");
    let skill_dir = root.join("skills").join("demo-skill");
    fs::create_dir_all(&skill_dir).expect("create skill dir");
    fs::write(skill_dir.join("SKILL.md"), "# Demo Skill").expect("write SKILL.md");

    let probe = probe_source_path_inner(&root.to_string_lossy()).expect("probe single repo");
    assert!(!probe.is_container);
    assert_eq!(probe.default_source_origin, SourceOrigin::GitRepo);
    assert_eq!(probe.candidates.len(), 1);
    assert_eq!(probe.candidates[0].skill_count, 1);
    assert_eq!(
        probe.candidates[0].remote_url.as_deref(),
        Some("https://github.com/example/my-skill-repo.git")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn probe_multi_git_repos_container_detects_sub_repositories() {
    let container = unique_temp_dir("probe-container-repo");
    let repo_a = container.join("repo-alpha");
    let repo_b = container.join("repo-beta");

    init_git_repo(&repo_a, "https://github.com/example/repo-alpha.git");
    init_git_repo(&repo_b, "git@github.com:example/repo-beta.git");

    let skill_a = repo_a.join("skills").join("skill-1");
    fs::create_dir_all(&skill_a).expect("create skill a");
    fs::write(skill_a.join("SKILL.md"), "# Skill 1").expect("write SKILL.md");

    let skill_b = repo_b.join("my-skill");
    fs::create_dir_all(&skill_b).expect("create skill b");
    fs::write(skill_b.join("SKILL.md"), "# Skill 2").expect("write SKILL.md");

    let probe = probe_source_path_inner(&container.to_string_lossy()).expect("probe container");
    assert!(probe.is_container);
    assert_eq!(probe.default_source_origin, SourceOrigin::LocalFolder);
    assert_eq!(probe.candidates.len(), 2);

    let names: Vec<_> = probe.candidates.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["repo-alpha", "repo-beta"]);
    assert_eq!(probe.candidates[0].skill_count, 1);
    assert_eq!(probe.candidates[1].skill_count, 1);
    assert!(probe.candidates[0].is_git_repo);
    assert!(probe.candidates[1].is_git_repo);

    let _ = fs::remove_dir_all(container);
}

#[test]
fn probe_plain_folder_is_not_container_and_has_no_git() {
    let plain = unique_temp_dir("probe-plain-dir");
    let sub = plain.join("just-a-folder");
    fs::create_dir_all(&sub).expect("create sub folder");
    fs::write(sub.join("test.txt"), "hello").expect("write file");

    let probe = probe_source_path_inner(&plain.to_string_lossy()).expect("probe plain");
    assert!(!probe.is_container);
    assert_eq!(probe.default_source_origin, SourceOrigin::LocalFolder);
    assert!(probe.candidates.is_empty());

    let _ = fs::remove_dir_all(plain);
}

#[tokio::test]
async fn split_container_source_creates_individual_git_sources() {
    let container = unique_temp_dir("split-container-test");
    let service = AppService::open_with_db_path(container.join("test.db"))
        .await
        .expect("open service");
    let repo_one = container.join("skill-repo-one");
    let repo_two = container.join("skill-repo-two");

    init_git_repo(&repo_one, "https://github.com/example/repo-one.git");
    init_git_repo(&repo_two, "https://github.com/example/repo-two.git");

    let s1 = repo_one.join("skill-one");
    fs::create_dir_all(&s1).expect("create skill 1");
    fs::write(s1.join("SKILL.md"), "# Skill One").expect("write skill 1");

    let s2 = repo_two.join("skill-two");
    fs::create_dir_all(&s2).expect("create skill 2");
    fs::write(s2.join("SKILL.md"), "# Skill Two").expect("write skill 2");

    // 首先作为普通单一导入源添加
    let initial_source = service
        .add_source(SourceInput {
            id: None,
            name: "My Fork Collection".to_string(),
            kind: SourceKind::Import,
            root_path: container.to_string_lossy().to_string(),
            scanner_kind: Some(SourceScannerKind::Skill),
            source_origin: Some(SourceOrigin::LocalFolder),
            repo_root: None,
            scan_root: None,
            origin_app_kind: None,
            origin_provider_id: None,
            include_globs: vec![],
            exclude_globs: vec![],
            default_kind: Some(AssetKind::Skill),
            enabled: true,
            priority: 10,
        })
        .await
        .expect("add initial container source");

    // 验证初始源是 local_folder
    assert_eq!(initial_source.source_origin, SourceOrigin::LocalFolder);

    // 拆分容器源
    let new_sources = service
        .split_container_source(&initial_source.id)
        .await
        .expect("split container source");

    assert_eq!(new_sources.len(), 2);
    assert_eq!(new_sources[0].source_origin, SourceOrigin::GitRepo);
    assert_eq!(new_sources[1].source_origin, SourceOrigin::GitRepo);

    // 验证旧源已被移除
    let all_sources = service.list_sources().await.expect("list sources");
    assert!(!all_sources.iter().any(|s| s.id == initial_source.id));
    assert!(all_sources.iter().any(|s| s.name == "skill-repo-one"));
    assert!(all_sources.iter().any(|s| s.name == "skill-repo-two"));

    let _ = fs::remove_dir_all(container);
}
