use super::prelude::*;

pub(super) fn resolve_command_path(manifest_dir: &Path, command: &str) -> PathBuf {
    let path = PathBuf::from(command);
    if path.is_absolute() {
        path
    } else {
        manifest_dir.join(path)
    }
}

pub(super) struct AdapterCommandInvocation {
    pub(super) program: PathBuf,
    pub(super) args: Vec<String>,
    pub(super) display_path: PathBuf,
}

pub(super) fn build_adapter_command_invocation(
    manifest_dir: &Path,
    command: &str,
    args: &[String],
) -> AdapterCommandInvocation {
    let executable = resolve_command_path(manifest_dir, command);
    if is_javascript_adapter_command(&executable) {
        let mut node_args = Vec::with_capacity(args.len() + 1);
        node_args.push(executable.to_string_lossy().to_string());
        node_args.extend_from_slice(args);
        return AdapterCommandInvocation {
            program: PathBuf::from("node"),
            args: node_args,
            display_path: executable,
        };
    }
    AdapterCommandInvocation {
        program: executable.clone(),
        args: args.to_vec(),
        display_path: executable,
    }
}

fn is_javascript_adapter_command(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "cjs" | "js" | "mjs"
            )
        })
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
