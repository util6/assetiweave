use super::io_utils::{
    build_adapter_runtime_invocation, ensure_adapter_runtime_available, sort_runtime_requirements,
    upsert_highest_runtime_requirement, validate_runtime_version_constraint,
    AdapterCommandInvocation, LEGACY_JAVASCRIPT_COMMAND_NODE_VERSION,
};
use super::prelude::*;

const HARVESTER_MANIFEST_FILE: &str = "harvester.json";
const HARVESTER_TIMEOUT_MS: u64 = 10 * 60 * 1000;
const OUTPUT_CAPTURE_LIMIT: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
struct HarvesterManifest {
    id: String,
    #[serde(default)]
    entrypoint: Vec<String>,
    #[serde(default)]
    runtime: Option<ConversationAdapterRuntime>,
}

pub(crate) fn run_conversation_harvester_for_source(source: &ConversationSource) -> AppResult<()> {
    let source_dir = crate::backend::path_utils::expand_path(&source.location)?;
    run_conversation_harvester_in_dir(&source_dir).map(|_| ())
}

pub(crate) fn run_conversation_harvester_for_adapter_source(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
) -> AppResult<()> {
    if let Some(adapter_dir) = adapter.and_then(adapter_manifest_dir) {
        if run_conversation_harvester_in_dir(&adapter_dir)? {
            return Ok(());
        }
    }

    run_conversation_harvester_for_source(source)
}

fn adapter_manifest_dir(adapter: &ConversationAdapter) -> Option<PathBuf> {
    let manifest_path = adapter.manifest_path.as_deref()?;
    let manifest_path = crate::backend::path_utils::expand_path(manifest_path).ok()?;
    manifest_path.parent().map(Path::to_path_buf)
}

fn run_conversation_harvester_in_dir(source_dir: &Path) -> AppResult<bool> {
    let manifest_path = source_dir.join(HARVESTER_MANIFEST_FILE);
    if !manifest_path.is_file() {
        return Ok(false);
    }

    let manifest_text = fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?;
    let manifest: HarvesterManifest =
        serde_json::from_str(&manifest_text).map_err(|error| error.to_string())?;
    let invocation = resolve_harvester_invocation(&source_dir, &manifest)?;
    if let Some(runtime) = harvester_execution_runtime(&manifest) {
        ensure_adapter_runtime_available(&runtime, &invocation)?;
    }

    let mut command = Command::new(&invocation.program);
    command
        .args(&invocation.args)
        .current_dir(source_dir)
        .env("ASSETIWEAVE_HARVESTER_DIR", source_dir)
        .env("ASSETIWEAVE_HARVESTER_ID", &manifest.id);
    let output = match crate::backend::host_process::run_command_with_timeout(
        &mut command,
        Duration::from_millis(HARVESTER_TIMEOUT_MS),
        OUTPUT_CAPTURE_LIMIT,
        OUTPUT_CAPTURE_LIMIT,
    ) {
        Ok(output) => output,
        Err(crate::backend::host_process::HostProcessError::Spawn(error)) => {
            return Err(format!("run harvester {}: {error}", manifest.id));
        }
        Err(crate::backend::host_process::HostProcessError::Output(error)) => {
            return Err(format!("capture harvester {} output: {error}", manifest.id));
        }
        Err(crate::backend::host_process::HostProcessError::Timeout {
            stdout,
            stderr,
            stdout_truncated,
            stderr_truncated,
        }) => {
            let mut message = format!(
                "harvester {} timed out after {} ms",
                manifest.id, HARVESTER_TIMEOUT_MS
            );
            append_captured_output(&mut message, "stdout", &stdout, stdout_truncated);
            append_captured_output(&mut message, "stderr", &stderr, stderr_truncated);
            return Err(message);
        }
    };
    if !output.status.success() {
        let mut message = format!(
            "harvester {} failed with status {}",
            manifest.id, output.status
        );
        append_captured_output(
            &mut message,
            "stdout",
            &output.stdout,
            output.stdout_truncated,
        );
        append_captured_output(
            &mut message,
            "stderr",
            &output.stderr,
            output.stderr_truncated,
        );
        return Err(message);
    }
    Ok(true)
}

pub(super) fn append_harvester_runtime_requirements(
    requirements: &mut Vec<(ConversationAdapterRuntimeKind, String)>,
    sources: &[ConversationSource],
) {
    for source in sources {
        if !source.enabled {
            continue;
        }
        let Ok(source_dir) = crate::backend::path_utils::expand_path(&source.location) else {
            continue;
        };
        let manifest_path = source_dir.join(HARVESTER_MANIFEST_FILE);
        let Ok(manifest_text) = fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<HarvesterManifest>(&manifest_text) else {
            continue;
        };
        if manifest.runtime.is_some() && !manifest.entrypoint.is_empty() {
            continue;
        }
        let Some(runtime) = harvester_execution_runtime(&manifest) else {
            continue;
        };
        if validate_harvester_runtime(&manifest.id, &runtime).is_err() {
            continue;
        }
        let Some(version) = runtime.version.as_deref() else {
            continue;
        };
        if matches!(runtime.kind, ConversationAdapterRuntimeKind::Executable)
            || validate_runtime_version_constraint(version).is_err()
        {
            continue;
        }
        upsert_highest_runtime_requirement(requirements, &runtime.kind, version);
    }
    *requirements = sort_runtime_requirements(std::mem::take(requirements));
}

fn resolve_harvester_invocation(
    root: &Path,
    manifest: &HarvesterManifest,
) -> AppResult<AdapterCommandInvocation> {
    if manifest.runtime.is_some() && !manifest.entrypoint.is_empty() {
        return Err(format!(
            "harvester {} must not declare both runtime and entrypoint",
            manifest.id
        ));
    }
    if let Some(runtime) = manifest.runtime.as_ref() {
        validate_harvester_runtime(&manifest.id, runtime)?;
        validate_harvester_relative_entry(root, &manifest.id, "runtime entry", &runtime.entry)?;
        return Ok(build_adapter_runtime_invocation(root, runtime, &[]));
    }
    let raw_command = manifest
        .entrypoint
        .first()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("harvester {} has no entrypoint", manifest.id))?;
    let command_path =
        validate_harvester_relative_entry(root, &manifest.id, "entrypoint", raw_command)?;
    if let Some(runtime) = harvester_execution_runtime(manifest) {
        return Ok(build_adapter_runtime_invocation(root, &runtime, &[]));
    }
    Ok(AdapterCommandInvocation {
        program: command_path.clone(),
        args: manifest.entrypoint.iter().skip(1).cloned().collect(),
        display_path: command_path,
    })
}

fn harvester_execution_runtime(manifest: &HarvesterManifest) -> Option<ConversationAdapterRuntime> {
    if let Some(runtime) = manifest.runtime.as_ref() {
        return Some(runtime.clone());
    }
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

fn validate_harvester_runtime(
    harvester_id: &str,
    runtime: &ConversationAdapterRuntime,
) -> AppResult<()> {
    if runtime.entry.trim().is_empty() {
        return Err(format!(
            "harvester {harvester_id} runtime entry is required"
        ));
    }
    if runtime
        .version
        .as_deref()
        .is_some_and(|version| version.trim().is_empty())
    {
        return Err(format!(
            "harvester {harvester_id} runtime version must not be empty"
        ));
    }
    if let Some(version) = runtime.version.as_deref() {
        validate_runtime_version_constraint(version)?;
    }
    Ok(())
}

fn validate_harvester_relative_entry(
    root: &Path,
    harvester_id: &str,
    field: &str,
    raw: &str,
) -> AppResult<PathBuf> {
    let trimmed = raw.trim();
    let relative = Path::new(trimmed);
    if relative.is_absolute()
        || looks_like_windows_rooted_path(trimmed)
        || trimmed
            .split(['/', '\\'])
            .any(|component| component == "..")
    {
        return Err(format!(
            "unsafe harvester {field} for {harvester_id}: {raw}"
        ));
    }
    let path = root.join(relative);
    if !path.is_file() {
        return Err(format!(
            "harvester {field} not found for {harvester_id}: {}",
            path.to_string_lossy()
        ));
    }
    Ok(path)
}

fn looks_like_windows_rooted_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    if path.starts_with("\\\\") || path.starts_with('\\') {
        return true;
    }
    bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
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

fn append_captured_output(message: &mut String, label: &str, bytes: &[u8], truncated: bool) {
    let text = String::from_utf8_lossy(bytes);
    if text.trim().is_empty() && !truncated {
        return;
    }
    message.push('\n');
    message.push_str(label);
    message.push_str(":\n");
    message.push_str(text.trim());
    if truncated {
        message.push_str("\n... [truncated]");
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
            runtime: None,
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

    #[test]
    fn harvester_manifest_runtime_uses_declared_runtime_without_entrypoint() {
        let fixture = TempFixture::new("assetiweave-harvester-runtime");
        fs::create_dir_all(fixture.path().join("scripts")).unwrap();
        fs::write(fixture.path().join("scripts").join("harvest.mjs"), "\n").unwrap();
        let manifest: HarvesterManifest = serde_json::from_str(
            r#"{"schema_version":1,"id":"fixture-web","runtime":{"type":"node","entry":"scripts/harvest.mjs","version":">=20","args":["--once"]}}"#,
        )
        .unwrap();

        let invocation = resolve_harvester_invocation(fixture.path(), &manifest).unwrap();

        assert_eq!(invocation.program, PathBuf::from("node"));
        assert_eq!(
            invocation.args,
            vec![
                fixture
                    .path()
                    .join("scripts")
                    .join("harvest.mjs")
                    .to_string_lossy()
                    .to_string(),
                "--once".to_string()
            ]
        );
    }

    #[test]
    fn harvester_manifest_rejects_runtime_mixed_with_entrypoint() {
        let fixture = TempFixture::new("assetiweave-harvester-mixed-runtime");
        fs::create_dir_all(fixture.path().join("scripts")).unwrap();
        fs::write(fixture.path().join("scripts").join("harvest.js"), "\n").unwrap();
        let manifest: HarvesterManifest = serde_json::from_str(
            r#"{"schema_version":1,"id":"fixture-web","entrypoint":["scripts/harvest.js"],"runtime":{"type":"node","entry":"scripts/harvest.js","version":">=20"}}"#,
        )
        .unwrap();

        let error = match resolve_harvester_invocation(fixture.path(), &manifest) {
            Ok(_) => panic!("manifest should not mix runtime and entrypoint"),
            Err(error) => error,
        };

        assert!(error.contains("must not declare both runtime and entrypoint"));
    }

    #[test]
    fn harvester_manifest_rejects_unsafe_runtime_entry() {
        for entry in [
            "../harvest.js",
            r"..\harvest.js",
            "/tmp/harvest.js",
            r"C:\tmp\harvest.js",
        ] {
            let fixture = TempFixture::new("assetiweave-harvester-runtime-escape");
            let manifest = HarvesterManifest {
                id: "fixture-web".to_string(),
                entrypoint: Vec::new(),
                runtime: Some(ConversationAdapterRuntime {
                    kind: ConversationAdapterRuntimeKind::Node,
                    entry: entry.to_string(),
                    args: Vec::new(),
                    version: Some(">=20".to_string()),
                }),
            };

            let error = match resolve_harvester_invocation(fixture.path(), &manifest) {
                Ok(_) => panic!("unsafe runtime entry should fail validation"),
                Err(error) => error,
            };

            assert!(
                error.contains("unsafe harvester runtime entry"),
                "entry {entry:?} produced error {error:?}"
            );
        }
    }

    #[test]
    fn harvester_manifest_rejects_unsupported_runtime_version_constraint() {
        let fixture = TempFixture::new("assetiweave-harvester-runtime-version");
        fs::create_dir_all(fixture.path().join("scripts")).unwrap();
        fs::write(fixture.path().join("scripts").join("harvest.js"), "\n").unwrap();
        let manifest = HarvesterManifest {
            id: "fixture-web".to_string(),
            entrypoint: Vec::new(),
            runtime: Some(ConversationAdapterRuntime {
                kind: ConversationAdapterRuntimeKind::Node,
                entry: "scripts/harvest.js".to_string(),
                args: Vec::new(),
                version: Some("^20".to_string()),
            }),
        };

        let error = match resolve_harvester_invocation(fixture.path(), &manifest) {
            Ok(_) => panic!("unsupported runtime version should fail validation"),
            Err(error) => error,
        };

        assert!(error.contains("runtime version constraint"));
    }

    #[test]
    fn harvester_runtime_requirements_skip_invalid_mixed_manifest() {
        let fixture = TempFixture::new("assetiweave-harvester-runtime-requirements");
        fs::create_dir_all(fixture.path().join("scripts")).unwrap();
        fs::write(fixture.path().join("scripts").join("harvest.js"), "\n").unwrap();
        fs::write(
            fixture.path().join("harvester.json"),
            r#"{"schema_version":1,"id":"fixture-web","entrypoint":["scripts/harvest.js"],"runtime":{"type":"node","entry":"scripts/harvest.js","version":">=20"}}"#,
        )
        .unwrap();
        let source = source_fixture(fixture.path());
        let mut requirements = Vec::new();

        append_harvester_runtime_requirements(&mut requirements, &[source]);

        assert!(requirements.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn adapter_manifest_directory_harvester_runs_for_normalized_output_source() {
        let fixture = TempFixture::new("assetiweave-harvester-adapter-dir");
        let normalized_dir = fixture.path().join("output").join("normalized");
        fs::create_dir_all(fixture.path().join("scripts")).unwrap();
        fs::create_dir_all(&normalized_dir).unwrap();
        fs::write(
            fixture.path().join("conversation-adapter.json"),
            r#"{"schema_version":1,"id":"fixture-web","name":"Fixture","version":"0.1.0","protocol_version":1,"command":["adapter.sh"],"capabilities":["read_session","web_records"],"input_kinds":["directory"]}"#,
        )
        .unwrap();
        fs::write(
            fixture.path().join("harvester.json"),
            r#"{"schema_version":1,"id":"fixture-web","name":"Fixture","version":"0.1.0","entrypoint":["scripts/harvest.sh"]}"#,
        )
        .unwrap();
        fs::write(
            fixture.path().join("scripts").join("harvest.sh"),
            "#!/bin/sh\nprintf 'fresh' > output/normalized/fresh.txt\n",
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
        let adapter = adapter_fixture(&fixture.path().join("conversation-adapter.json"));
        let source = source_fixture(&normalized_dir);

        run_conversation_harvester_for_adapter_source(Some(&adapter), &source)
            .expect("run adapter-directory harvester");

        assert_eq!(
            fs::read_to_string(normalized_dir.join("fresh.txt")).unwrap(),
            "fresh"
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

    fn adapter_fixture(manifest_path: &Path) -> ConversationAdapter {
        ConversationAdapter {
            id: "fixture-web".to_string(),
            name: "Fixture".to_string(),
            kind: ConversationAdapterKind::External,
            version: "0.1.0".to_string(),
            enabled: true,
            manifest_path: Some(manifest_path.to_string_lossy().to_string()),
            executable_path: None,
            content_hash: None,
            trusted_hash: None,
            trust_state: ConversationAdapterTrustState::Trusted,
            protocol_version: Some(1),
            capabilities: vec!["read_session".to_string(), "web_records".to_string()],
            input_kinds: vec![ConversationSourceKind::Directory],
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
