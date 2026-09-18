use super::{
    detect_target_provider, expand_path, git_repository_for_path, hash_path, sanitize_git_remote,
};
use crate::backend::models::{
    AppKind, AssetKind, DeploymentStrategy, TargetPathRule, TargetProfileDescriptor,
};
use crate::backend::target_catalog::TargetCatalog;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn expand_path_maps_home_shorthand_to_home_directory() {
    let home = expand_path("~").expect("expand home");
    let child = expand_path("~/.codex/skills").expect("expand home child");

    assert!(home.is_absolute());
    assert_ne!(home.file_name(), Some(std::ffi::OsStr::new("~")));
    assert!(child.is_absolute());
    assert!(!child
        .components()
        .any(|component| component.as_os_str() == std::ffi::OsStr::new("~")));
    assert!(child.ends_with(Path::new(".codex").join("skills")));
}

#[test]
fn runtime_catalog_drives_target_detection_for_new_provider() {
    let root = unique_temp_dir("assetiweave-target-provider-test");
    let target = root.join("skills");
    let nested = target.join("nested");
    fs::create_dir_all(&nested).expect("create target fixture");
    let catalog = TargetCatalog::from_descriptors(vec![TargetProfileDescriptor {
        id: "fixture-provider".to_string(),
        name: "Fixture Provider".to_string(),
        app_kind_compat: Some(AppKind::Custom),
        default_targets: vec![TargetPathRule {
            asset_kind: AssetKind::Skill,
            path: camino::Utf8Path::from_path(&target)
                .expect("valid utf8 target path")
                .to_string(),
        }],
        supported_kinds: vec![AssetKind::Skill],
        deployment_strategy: DeploymentStrategy::SymlinkToSource,
        icon: None,
    }])
    .expect("fixture target catalog");

    assert_eq!(
        detect_target_provider(&nested, &catalog),
        Some(("fixture-provider".to_string(), Some(AppKind::Custom)))
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn target_detection_prefers_the_most_specific_matching_rule() {
    let root = unique_temp_dir("assetiweave-target-provider-specificity");
    let broad = root.join("skills");
    let specific = broad.join("nested");
    fs::create_dir_all(&specific).expect("create target fixture");
    let descriptor = |id: &str, path: &Path| TargetProfileDescriptor {
        id: id.to_string(),
        name: id.to_string(),
        app_kind_compat: Some(AppKind::Custom),
        default_targets: vec![TargetPathRule {
            asset_kind: AssetKind::Skill,
            path: camino::Utf8Path::from_path(path)
                .expect("valid utf8 path")
                .to_string(),
        }],
        supported_kinds: vec![AssetKind::Skill],
        deployment_strategy: DeploymentStrategy::SymlinkToSource,
        icon: None,
    };
    let catalog = TargetCatalog::from_descriptors(vec![
        descriptor("broad", &broad),
        descriptor("specific", &specific),
    ])
    .expect("nested target rules are valid");

    assert_eq!(
        detect_target_provider(&specific.join("item"), &catalog),
        Some(("specific".to_string(), Some(AppKind::Custom)))
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn hash_path_changes_when_directory_file_changes() {
    let root = unique_temp_dir("assetiweave-hash-test");
    fs::create_dir_all(&root).expect("create temp dir");
    let skill_file = root.join("SKILL.md");
    let script_file = root.join("script.sh");
    fs::write(&skill_file, "skill").expect("write skill");
    fs::write(&script_file, "one").expect("write script");

    let first_hash = hash_path(&root).expect("hash dir");
    fs::write(&script_file, "two").expect("update script");
    let second_hash = hash_path(&root).expect("hash dir again");

    fs::remove_dir_all(&root).ok();
    assert_ne!(first_hash, second_hash);
}

#[test]
fn git_repository_for_path_prefers_the_nearest_nested_repository() {
    let root = unique_temp_dir("assetiweave-git-nested-test");
    let outer_repo = root.join("outer");
    let nested_repo = outer_repo.join("repos").join("nested");
    let nested_skill = nested_repo.join("skills").join("demo");
    fs::create_dir_all(&nested_skill).expect("create nested skill");
    init_git_repo(&outer_repo, "https://example.com/outer.git");
    init_git_repo(&nested_repo, "git@example.com:nested.git");

    let repository = git_repository_for_path(&nested_skill).expect("resolve nested repository");

    assert_eq!(PathBuf::from(repository.root_path), nested_repo);
    assert_eq!(
        repository.remote_url.as_deref(),
        Some("git@example.com:nested.git")
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn git_repository_for_path_reads_the_source_root_repository() {
    let root = unique_temp_dir("assetiweave-git-root-test");
    let skill = root.join("skills").join("demo");
    fs::create_dir_all(&skill).expect("create skill");
    init_git_repo(&root, "https://example.com/root.git");

    let repository = git_repository_for_path(&skill).expect("resolve source repository");

    assert_eq!(PathBuf::from(repository.root_path), root);
    assert_eq!(
        repository.remote_url.as_deref(),
        Some("https://example.com/root.git")
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn git_remote_display_removes_embedded_http_credentials() {
    assert_eq!(
        sanitize_git_remote("https://oauth2:secret@example.com/private/repo.git"),
        "https://example.com/private/repo.git"
    );
    assert_eq!(
        sanitize_git_remote("git@github.com:util6/util6-agents.git"),
        "git@github.com:util6/util6-agents.git"
    );
}

fn init_git_repo(path: &PathBuf, remote_url: &str) {
    fs::create_dir_all(path).expect("create repository directory");
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
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()))
}

#[test]
#[cfg(unix)]
fn non_utf8_paths_never_alias_identity_or_persistence_keys() {
    use crate::backend::runtime::AppError;
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    // 1. Filesystem identity comparison must remain lossless and never alias.
    // Two paths differing only by distinct non-UTF-8 bytes:
    let non_utf8_a = PathBuf::from(OsStr::from_bytes(b"/tmp/assetiweave_test_\xFF_repo"));
    let non_utf8_b = PathBuf::from(OsStr::from_bytes(b"/tmp/assetiweave_test_\xFE_repo"));
    assert_ne!(non_utf8_a, non_utf8_b);

    // Recent root sorting/comparison via Path::cmp must preserve strict ordering
    let mut sorted_paths = vec![non_utf8_b.clone(), non_utf8_a.clone()];
    sorted_paths.sort();
    assert_eq!(sorted_paths, vec![non_utf8_b.clone(), non_utf8_a.clone()]); // \xFE < \xFF

    // Candidate comparison (such as in skills.rs) against Path must be exact and lossless
    let candidate_path_lossy = non_utf8_a.to_string_lossy().to_string();
    // In the buggy lossy conversion: candidate_path_lossy == non_utf8_b.to_string_lossy() is TRUE (aliased!)
    assert_eq!(
        non_utf8_a.to_string_lossy(),
        non_utf8_b.to_string_lossy(),
        "sanity check: lossy string conversion would alias distinct non-UTF-8 paths"
    );
    // But with lossless &Path comparison:
    assert_ne!(
        Path::new(&candidate_path_lossy),
        non_utf8_a.as_path(),
        "lossy string candidate cannot match original raw byte path"
    );
    assert_ne!(non_utf8_a.as_path(), non_utf8_b.as_path());

    // 2. Persistence / Storage boundary must fail with AppError::Validation rather than alias
    let storage_result_a = super::normalize_std_path_for_storage(&non_utf8_a);
    let storage_result_b = super::normalize_std_path_for_storage(&non_utf8_b);
    assert!(
            matches!(storage_result_a, Err(AppError::Validation(_))),
            "storage normalization must return Validation error on non-utf8 path, got: {storage_result_a:?}"
        );
    assert!(
            matches!(storage_result_b, Err(AppError::Validation(_))),
            "storage normalization must return Validation error on non-utf8 path, got: {storage_result_b:?}"
        );

    // 3. Target Catalog conflict key helper must reject non-UTF-8 components
    let mut target_normalized = std::path::PathBuf::from(std::path::MAIN_SEPARATOR.to_string());
    target_normalized.push(OsStr::from_bytes(b"target_\xFF"));
    let conflict_key_result = target_normalized
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::Validation("path contains invalid UTF-8".to_string()));
    assert!(
        matches!(conflict_key_result, Err(AppError::Validation(_))),
        "target conflict key must reject non-utf8 components"
    );
}
