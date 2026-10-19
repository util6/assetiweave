use super::*;
use std::fs;

struct TestDir(PathBuf);

impl TestDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{}-{}", prefix, uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create test dir");
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn test_resolve_project_directory_git_repo_subfolder() {
    let temp = TestDir::new("assetiweave-test-git");
    let git_dir = temp.path().join("my-repo").join(".git");
    fs::create_dir_all(&git_dir).expect("create git dir");
    let subpkg = temp.path().join("my-repo").join("packages").join("app");
    fs::create_dir_all(&subpkg).expect("create subpkg");

    let resolved = resolve_project_directory(subpkg.to_str().unwrap(), &[]);
    assert!(resolved.is_some());
    let resolved_path = resolved.unwrap();
    assert!(
        resolved_path.ends_with("my-repo"),
        "Expected resolved path to end with 'my-repo', got: {}",
        resolved_path
    );
}

#[test]
fn test_resolve_project_directory_non_git_folder() {
    let temp = TestDir::new("assetiweave-test-nongit");
    let standalone = temp.path().join("documents").join("notes");
    fs::create_dir_all(&standalone).expect("create standalone");

    let resolved = resolve_project_directory(standalone.to_str().unwrap(), &[]);
    assert!(resolved.is_some());
    let resolved_path = resolved.unwrap();
    assert!(
        resolved_path.ends_with("notes"),
        "Expected non-git folder to resolve to itself, got: {}",
        resolved_path
    );
}

#[test]
fn test_resolve_project_directory_registered_root() {
    let temp = TestDir::new("assetiweave-test-reg");
    let root = temp.path().join("workspace");
    let sub = root.join("project-x").join("src");
    fs::create_dir_all(&sub).expect("create sub");

    let registered = vec![root.to_str().unwrap().to_string()];
    let resolved = resolve_project_directory(sub.to_str().unwrap(), &registered);
    assert!(resolved.is_some());
    let resolved_path = resolved.unwrap();
    assert!(
        resolved_path.ends_with("workspace"),
        "Expected registered root to take precedence, got: {}",
        resolved_path
    );
}

#[test]
fn test_normalize_path_lexically() {
    let raw = Path::new("/a/b/../c/./d");
    let normalized = normalize_path_lexically(raw);
    assert_eq!(normalized, PathBuf::from("/a/c/d"));
}
