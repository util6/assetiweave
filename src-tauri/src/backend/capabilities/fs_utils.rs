use super::prelude::*;

pub(crate) fn copy_dir(source: &Path, target: &Path) -> AppResult<()> {
    crate::backend::host_filesystem::HostFilesystem::current().copy_dir(source, target)
}

pub(crate) fn copy_dir_without_conflicts(source: &Path, target: &Path) -> AppResult<()> {
    crate::backend::host_filesystem::HostFilesystem::current()
        .copy_dir_without_conflicts(source, target)
}

pub(crate) fn same_path_or_text(left: &Path, right: &Path) -> bool {
    crate::backend::host_filesystem::HostFilesystem::current().same_path(left, right)
}
