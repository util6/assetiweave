use crate::backend::infrastructure::InfraResult;
use std::path::Path;

pub(crate) fn copy_dir(source: &Path, target: &Path) -> InfraResult<()> {
    Ok(
        crate::backend::infrastructure::host_filesystem::HostFilesystem::current()
            .copy_dir(source, target)?,
    )
}

pub(crate) fn copy_dir_without_conflicts(source: &Path, target: &Path) -> InfraResult<()> {
    Ok(
        crate::backend::infrastructure::host_filesystem::HostFilesystem::current()
            .copy_dir_without_conflicts(source, target)?,
    )
}

pub(crate) fn same_path_or_text(left: &Path, right: &Path) -> bool {
    crate::backend::infrastructure::host_filesystem::HostFilesystem::current()
        .same_path(left, right)
}
