use super::prelude::*;

pub(super) fn collect_files_with_extension(
    root: &Path,
    extension: &str,
    limit: usize,
) -> AppResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files_with_extension_inner(root, extension, limit, &mut files)?;
    files.sort();
    Ok(files)
}

pub(super) fn collect_files_with_extension_inner(
    root: &Path,
    extension: &str,
    limit: usize,
    files: &mut Vec<PathBuf>,
) -> AppResult<()> {
    if files.len() >= limit {
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_with_extension_inner(&path, extension, limit, files)?;
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        {
            files.push(path);
            if files.len() >= limit {
                return Ok(());
            }
        }
    }
    Ok(())
}

pub(super) fn resolve_command_path(manifest_dir: &Path, command: &str) -> PathBuf {
    let path = PathBuf::from(command);
    if path.is_absolute() {
        path
    } else {
        manifest_dir.join(path)
    }
}

pub(super) fn read_capped<R: Read>(mut reader: R, cap: usize) -> AppResult<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        output.extend_from_slice(&buffer[..read]);
        if output.len() > cap {
            return Err(format!("adapter output exceeded cap of {cap} bytes"));
        }
    }
    Ok(output)
}

pub(super) fn hash_file(path: &Path) -> AppResult<String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    Ok(hash_bytes(&bytes))
}

pub(super) fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub(super) fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

#[allow(dead_code)]
pub(super) fn _metadata_map(value: &Value) -> BTreeMap<String, Value> {
    value
        .as_object()
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default()
}
