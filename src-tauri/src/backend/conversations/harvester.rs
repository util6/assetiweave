use super::io_utils::{
    build_adapter_runtime_invocation, ensure_adapter_runtime_available, AdapterCommandInvocation,
    LEGACY_JAVASCRIPT_COMMAND_NODE_VERSION,
};
use super::prelude::*;

const HARVESTER_MANIFEST_FILE: &str = "harvester.json";
const HARVESTER_TIMEOUT_MS: u64 = 10 * 60 * 1000;
const OUTPUT_CAPTURE_LIMIT: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
struct HarvesterManifest {
    id: String,
    entrypoint: Vec<String>,
}

pub(crate) fn run_conversation_harvester_for_source(source: &ConversationSource) -> AppResult<()> {
    let source_dir = crate::backend::path_utils::expand_path(&source.location)?;
    let manifest_path = source_dir.join(HARVESTER_MANIFEST_FILE);
    if !manifest_path.is_file() {
        return Ok(());
    }

    let manifest_text = fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?;
    let manifest: HarvesterManifest =
        serde_json::from_str(&manifest_text).map_err(|error| error.to_string())?;
    let invocation = resolve_harvester_invocation(&source_dir, &manifest)?;
    if let Some(runtime) = harvester_entrypoint_runtime(&manifest) {
        ensure_adapter_runtime_available(&runtime, &invocation)?;
    }

    let mut child = Command::new(&invocation.program)
        .args(&invocation.args)
        .current_dir(&source_dir)
        .env("ASSETIWEAVE_HARVESTER_DIR", &source_dir)
        .env("ASSETIWEAVE_HARVESTER_ID", &manifest.id)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("run harvester {}: {error}", manifest.id))?;

    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            let mut stdout = String::new();
            let mut stderr = String::new();
            if let Some(mut pipe) = child.stdout.take() {
                let mut bytes = Vec::new();
                pipe.read_to_end(&mut bytes)
                    .map_err(|error| error.to_string())?;
                stdout = capped_utf8(&bytes, OUTPUT_CAPTURE_LIMIT);
            }
            if let Some(mut pipe) = child.stderr.take() {
                let mut bytes = Vec::new();
                pipe.read_to_end(&mut bytes)
                    .map_err(|error| error.to_string())?;
                stderr = capped_utf8(&bytes, OUTPUT_CAPTURE_LIMIT);
            }
            if !status.success() {
                let mut message =
                    format!("harvester {} failed with status {}", manifest.id, status);
                if !stdout.trim().is_empty() {
                    message.push_str("\nstdout:\n");
                    message.push_str(stdout.trim());
                }
                if !stderr.trim().is_empty() {
                    message.push_str("\nstderr:\n");
                    message.push_str(stderr.trim());
                }
                return Err(message);
            }
            return Ok(());
        }
        if start.elapsed() > Duration::from_millis(HARVESTER_TIMEOUT_MS) {
            let _ = child.kill();
            return Err(format!(
                "harvester {} timed out after {} ms",
                manifest.id, HARVESTER_TIMEOUT_MS
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn resolve_harvester_invocation(
    root: &Path,
    manifest: &HarvesterManifest,
) -> AppResult<AdapterCommandInvocation> {
    let raw_command = manifest
        .entrypoint
        .first()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("harvester {} has no entrypoint", manifest.id))?;
    let relative = Path::new(raw_command);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(format!(
            "unsafe harvester entrypoint for {}: {}",
            manifest.id, raw_command
        ));
    }
    let command_path = root.join(relative);
    if !command_path.is_file() {
        return Err(format!(
            "harvester entrypoint not found for {}: {}",
            manifest.id,
            command_path.to_string_lossy()
        ));
    }
    if let Some(runtime) = harvester_entrypoint_runtime(manifest) {
        return Ok(build_adapter_runtime_invocation(root, &runtime, &[]));
    }
    Ok(AdapterCommandInvocation {
        program: command_path.clone(),
        args: manifest.entrypoint.iter().skip(1).cloned().collect(),
        display_path: command_path,
    })
}

fn harvester_entrypoint_runtime(
    manifest: &HarvesterManifest,
) -> Option<ConversationAdapterRuntime> {
    let (entry, args) = manifest.entrypoint.split_first()?;
    if !is_javascript_harvester_entrypoint(Path::new(entry)) {
        return None;
    }
    Some(ConversationAdapterRuntime {
        kind: ConversationAdapterRuntimeKind::Node,
        entry: entry.trim().to_string(),
        args: args.to_vec(),
        version: Some(LEGACY_JAVASCRIPT_COMMAND_NODE_VERSION.to_string()),
    })
}

fn is_javascript_harvester_entrypoint(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "cjs" | "js" | "mjs"
            )
        })
}

fn capped_utf8(bytes: &[u8], limit: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= limit {
        text.to_string()
    } else {
        format!("{}... [truncated]", &text[..limit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_without_harvester_manifest_is_noop() {
        let fixture = TempFixture::new("assetiweave-harvester-noop");
        let source = source_fixture(fixture.path());

        run_conversation_harvester_for_source(&source).expect("missing manifest should be noop");
    }

    #[cfg(unix)]
    #[test]
    fn runs_external_harvester_entrypoint() {
        let fixture = TempFixture::new("assetiweave-harvester-run");
        fs::write(
            fixture.path().join("harvester.json"),
            r#"{"schema_version":1,"id":"fixture-web","name":"Fixture","version":"0.1.0","entrypoint":["scripts/harvest.sh"]}"#,
        )
        .unwrap();
        fs::create_dir_all(fixture.path().join("scripts")).unwrap();
        fs::write(
            fixture.path().join("scripts").join("harvest.sh"),
            "#!/bin/sh\nmkdir -p output/normalized\nprintf '{\"sessions\":[]}' > output/normalized/sessions.json\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(fixture.path().join("scripts").join("harvest.sh"))
            .unwrap()
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(
            fixture.path().join("scripts").join("harvest.sh"),
            permissions,
        )
        .unwrap();
        let source = source_fixture(fixture.path());

        run_conversation_harvester_for_source(&source).expect("run harvester");

        assert!(fixture
            .path()
            .join("output")
            .join("normalized")
            .join("sessions.json")
            .is_file());
    }

    #[test]
    fn javascript_harvester_entrypoint_uses_node_runtime() {
        let fixture = TempFixture::new("assetiweave-harvester-js-runtime");
        fs::create_dir_all(fixture.path().join("scripts")).unwrap();
        fs::write(fixture.path().join("scripts").join("harvest.js"), "\n").unwrap();
        let manifest = HarvesterManifest {
            id: "fixture-web".to_string(),
            entrypoint: vec!["scripts/harvest.js".to_string(), "--once".to_string()],
        };

        let invocation = resolve_harvester_invocation(fixture.path(), &manifest).unwrap();

        assert_eq!(invocation.program, PathBuf::from("node"));
        assert_eq!(
            invocation.args,
            vec![
                fixture
                    .path()
                    .join("scripts")
                    .join("harvest.js")
                    .to_string_lossy()
                    .to_string(),
                "--once".to_string()
            ]
        );
    }

    fn source_fixture(path: &Path) -> ConversationSource {
        ConversationSource {
            id: "fixture-source".to_string(),
            adapter_id: "fixture-web".to_string(),
            name: "Fixture".to_string(),
            kind: ConversationSourceKind::Directory,
            location: path.to_string_lossy().to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    struct TempFixture {
        path: PathBuf,
    }

    impl TempFixture {
        fn new(name: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!(
                "{name}-{}",
                Utc::now().timestamp_nanos_opt().unwrap_or_default()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
