use super::*;

async fn run_conversation_harvester_for_source(source: &ConversationSource) -> AppResult<()> {
    run_conversation_harvester_for_source_with_settings(source, &serde_json::json!({})).await
}

async fn run_conversation_harvester_for_adapter_source(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
    full_reparse: bool,
) -> AppResult<()> {
    run_conversation_harvester_with_control(
        adapter,
        source,
        full_reparse,
        &serde_json::json!({}),
        None,
    )
    .await
}

fn resolve_harvester_invocation(
    root: &Path,
    manifest: &HarvesterManifest,
) -> AppResult<AdapterCommandInvocation> {
    resolve_harvester_invocation_with_settings(root, manifest, &serde_json::json!({}))
}

#[tokio::test]
async fn source_without_harvester_manifest_is_noop() {
    let fixture = TempFixture::new("assetiweave-harvester-noop");
    let source = source_fixture(fixture.path());

    run_conversation_harvester_for_source(&source)
        .await
        .expect("missing manifest should be noop");
}

#[cfg(unix)]
#[tokio::test]
async fn runs_external_harvester_entrypoint() {
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

    run_conversation_harvester_for_source(&source)
        .await
        .expect("run harvester");

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
        PathBuf::from(&invocation.args[0]),
        fixture.path().join("scripts").join("harvest.js")
    );
    assert_eq!(&invocation.args[1..], &["--once"]);
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
        PathBuf::from(&invocation.args[0]),
        fixture.path().join("scripts").join("harvest.mjs")
    );
    assert_eq!(&invocation.args[1..], &["--once"]);
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
#[tokio::test]
async fn adapter_manifest_directory_harvester_runs_for_normalized_output_source() {
    let adapter_pkg_fixture = TempFixture::new("assetiweave-harvester-adapter-pkg");
    let data_fixture = TempFixture::new("assetiweave-harvester-data-root");
    let normalized_dir = data_fixture.path().join("output").join("normalized");
    fs::create_dir_all(adapter_pkg_fixture.path().join("scripts")).unwrap();
    fs::create_dir_all(&normalized_dir).unwrap();
    fs::write(
            adapter_pkg_fixture.path().join("conversation-adapter.json"),
            r#"{"schema_version":1,"id":"fixture-web","name":"Fixture","version":"0.1.0","protocol_version":1,"command":["adapter.sh"],"capabilities":["read_session","web_records"],"input_kinds":["directory"]}"#,
        )
        .unwrap();
    fs::write(
            adapter_pkg_fixture.path().join("harvester.json"),
            r#"{"schema_version":1,"id":"fixture-web","name":"Fixture","version":"0.1.0","entrypoint":["scripts/harvest.sh"]}"#,
        )
        .unwrap();
    fs::write(
        adapter_pkg_fixture
            .path()
            .join("scripts")
            .join("harvest.sh"),
        "#!/bin/sh\nprintf 'fresh' > output/normalized/fresh.txt\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(
        adapter_pkg_fixture
            .path()
            .join("scripts")
            .join("harvest.sh"),
    )
    .unwrap()
    .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(
        adapter_pkg_fixture
            .path()
            .join("scripts")
            .join("harvest.sh"),
        permissions,
    )
    .unwrap();
    let adapter = adapter_fixture(&adapter_pkg_fixture.path().join("conversation-adapter.json"));
    let source = source_fixture(&normalized_dir);

    run_conversation_harvester_for_adapter_source(Some(&adapter), &source, false)
        .await
        .expect("run adapter-directory harvester");

    assert_eq!(
        fs::read_to_string(normalized_dir.join("fresh.txt")).unwrap(),
        "fresh"
    );
    assert!(
        !adapter_pkg_fixture.path().join("output").exists(),
        "adapter package directory must remain untouched by harvester execution"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn full_reparse_tells_web_harvesters_to_bypass_incremental_caches() {
    let adapter_pkg_fixture = TempFixture::new("assetiweave-harvester-full-adapter-pkg");
    let data_fixture = TempFixture::new("assetiweave-harvester-full-data-root");
    let normalized_dir = data_fixture.path().join("output").join("normalized");
    fs::create_dir_all(adapter_pkg_fixture.path().join("scripts")).unwrap();
    fs::create_dir_all(&normalized_dir).unwrap();
    fs::write(
            adapter_pkg_fixture.path().join("conversation-adapter.json"),
            r#"{"schema_version":1,"id":"fixture-web","name":"Fixture","version":"0.1.0","protocol_version":1,"command":["adapter.sh"],"capabilities":["read_session","web_records"],"input_kinds":["directory"]}"#,
        )
        .unwrap();
    fs::write(
            adapter_pkg_fixture.path().join("harvester.json"),
            r#"{"schema_version":1,"id":"fixture-web","name":"Fixture","version":"0.1.0","entrypoint":["scripts/harvest.sh"]}"#,
        )
        .unwrap();
    fs::write(
            adapter_pkg_fixture.path().join("scripts").join("harvest.sh"),
            "#!/bin/sh\nprintf '%s' \"$ASSETIWEAVE_FULL_REPARSE\" > output/normalized/full-reparse.txt\n",
        )
        .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let script = adapter_pkg_fixture
        .path()
        .join("scripts")
        .join("harvest.sh");
    let mut permissions = fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&script, permissions).unwrap();
    let adapter = adapter_fixture(&adapter_pkg_fixture.path().join("conversation-adapter.json"));
    let source = source_fixture(&normalized_dir);

    run_conversation_harvester_for_adapter_source(Some(&adapter), &source, true)
        .await
        .expect("run full-reparse harvester");

    assert_eq!(
        fs::read_to_string(normalized_dir.join("full-reparse.txt")).unwrap(),
        "1"
    );
    assert!(
        !adapter_pkg_fixture.path().join("output").exists(),
        "adapter package directory must remain untouched by harvester execution"
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
        card_contract_version: None,
        card_kinds: Vec::new(),
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
