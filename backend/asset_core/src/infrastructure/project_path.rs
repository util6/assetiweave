use std::path::{Component, Path, PathBuf};

use crate::backend::infrastructure::host_filesystem::HostFilesystem;
use crate::backend::infrastructure::path_utils::{
    expand_path, find_git_root, normalize_std_path_for_storage,
};

pub const UNASSIGNED_PROJECT_KEY: &str = "__unassigned__";

pub fn resolve_project_directory(raw_path: &str, registered_roots: &[String]) -> Option<String> {
    let cwd = expand_path(raw_path).ok()?;
    let cwd = canonicalize_or_normalize(&cwd);
    let filesystem = HostFilesystem::current();

    let registered_root = registered_roots
        .iter()
        .filter_map(|root| {
            let root = expand_path(root).ok()?;
            let root = canonicalize_or_normalize(&root);
            filesystem
                .is_within(&cwd, &root)
                .then_some((root.components().count(), root))
        })
        .max_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)))
        .map(|(_, root)| root);

    let project_root = registered_root
        .or_else(|| find_git_root(&cwd).map(|root| canonicalize_or_normalize(&root)))
        .unwrap_or(cwd);
    normalize_std_path_for_storage(&project_root).ok()
}

pub fn canonicalize_or_normalize(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|_| normalize_path_lexically(path))
}

pub fn normalize_path_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

#[cfg(test)]
#[path = "project_path_tests.rs"]
mod tests;
