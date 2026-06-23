use super::prelude::*;

pub(crate) fn copy_dir(source: &Path, target: &Path) -> AppResult<()> {
    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| error.to_string())?;
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::copy(entry.path(), destination).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub(crate) fn copy_dir_without_conflicts(source: &Path, target: &Path) -> AppResult<()> {
    if !source.exists() {
        return Ok(());
    }
    if !source.is_dir() {
        return Err(format!(
            "backup source is not a directory: {}",
            source.display()
        ));
    }

    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| error.to_string())?;
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }

        if destination.exists() {
            if !destination.is_file() {
                return Err(format!(
                    "backup migration target is not a file: {}",
                    destination.display()
                ));
            }
            let source_bytes = fs::read(entry.path()).map_err(|error| error.to_string())?;
            let destination_bytes = fs::read(&destination).map_err(|error| error.to_string())?;
            if source_bytes != destination_bytes {
                return Err(format!(
                    "backup migration target already has different content: {}",
                    destination.display()
                ));
            }
            continue;
        }

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::copy(entry.path(), destination).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn same_path_or_text(left: &Path, right: &Path) -> bool {
    let normalized_left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let normalized_right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    normalized_left == normalized_right || left == right
}
