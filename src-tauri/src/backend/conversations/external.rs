use super::prelude::*;

pub(super) fn read_external_adapter_sessions(
    adapter: &ConversationAdapter,
    source: &ConversationSource,
) -> AppResult<Vec<NormalizedConversationSession>> {
    run_external_adapter_read_session(adapter, source, None).map(|result| result.sessions)
}

pub(super) fn read_external_adapter_session(
    adapter: &ConversationAdapter,
    source: &ConversationSource,
    session_id: &str,
) -> AppResult<Vec<NormalizedConversationSession>> {
    run_external_adapter_read_session(adapter, source, Some(session_id))
        .map(|result| result.sessions)
}

pub(super) fn discover_external_adapter_sessions(
    adapter: &ConversationAdapter,
    source: &ConversationSource,
) -> AppResult<Option<ExternalAdapterRunResult>> {
    if !adapter
        .capabilities
        .iter()
        .any(|capability| capability == "list_sessions")
    {
        return Ok(None);
    }
    validate_external_adapter_for_method(adapter, source, "list_sessions")?;
    let manifest_path = adapter.manifest_path.as_deref().ok_or_else(|| {
        format!(
            "external conversation adapter has no manifest: {}",
            adapter.id
        )
    })?;
    let validation = validate_external_adapter_manifest(manifest_path)?;
    validate_external_adapter_manifest_for_method(adapter, &validation, "list_sessions")?;
    let request = json!({
        "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        "request_id": format!("list-{}-{}", source.id, Utc::now().timestamp_millis()),
        "method": "list_sessions",
        "source": { "location": source.location, "config": source_config_value(source)? },
        "params": { "cursor": null }
    });
    let result = run_external_adapter(
        &validation,
        "list_sessions",
        request,
        Duration::from_millis(DEFAULT_LIST_TIMEOUT_MS),
    )?;
    if !result.snapshot_complete {
        return Ok(None);
    }
    Ok(Some(result))
}

fn run_external_adapter_read_session(
    adapter: &ConversationAdapter,
    source: &ConversationSource,
    session_id: Option<&str>,
) -> AppResult<ExternalAdapterRunResult> {
    validate_external_adapter_for_method(adapter, source, "read_session")?;
    let manifest_path = adapter.manifest_path.as_deref().ok_or_else(|| {
        format!(
            "external conversation adapter has no manifest: {}",
            adapter.id
        )
    })?;
    let validation = validate_external_adapter_manifest(manifest_path)?;
    validate_external_adapter_manifest_for_method(adapter, &validation, "read_session")?;
    let request = json!({
        "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        "request_id": format!("sync-{}-{}", source.id, Utc::now().timestamp_millis()),
        "method": "read_session",
        "source": { "location": source.location, "config": source_config_value(source)? },
        "params": { "session_id": session_id }
    });
    run_external_adapter(
        &validation,
        "read_session",
        request,
        Duration::from_millis(DEFAULT_READ_TIMEOUT_MS),
    )
}

pub(crate) fn export_external_adapter_markdown(
    adapter: &ConversationAdapter,
    source: &ConversationSource,
    detail: &crate::backend::dto::ConversationSessionDetail,
    question_ids: &[String],
    content_filter: &crate::backend::dto::ConversationExportContentFilter,
    record_kind: &str,
    default_relative_path: &str,
) -> AppResult<ExternalMarkdownExport> {
    validate_external_adapter_for_method(adapter, source, "export_markdown")?;
    let manifest_path = adapter.manifest_path.as_deref().ok_or_else(|| {
        format!(
            "external conversation adapter has no manifest: {}",
            adapter.id
        )
    })?;
    let validation = validate_external_adapter_manifest(manifest_path)?;
    validate_external_adapter_manifest_for_method(adapter, &validation, "export_markdown")?;
    let request = json!({
        "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        "request_id": format!("export-{}-{}", detail.session.id, Utc::now().timestamp_millis()),
        "method": "export_markdown",
        "source": { "location": source.location, "config": source_config_value(source)? },
        "params": {
            "session_detail": detail,
            "question_ids": question_ids,
            "content_filter": content_filter,
            "record_kind": record_kind,
            "default_relative_path": default_relative_path
        }
    });
    run_external_adapter(
        &validation,
        "export_markdown",
        request,
        Duration::from_millis(DEFAULT_READ_TIMEOUT_MS),
    )?
    .markdown_export
    .ok_or_else(|| {
        format!(
            "external conversation adapter {} did not return markdown_export",
            adapter.id
        )
    })
}

fn validate_external_adapter_for_method(
    adapter: &ConversationAdapter,
    source: &ConversationSource,
    method: &str,
) -> AppResult<()> {
    if adapter.kind != ConversationAdapterKind::External {
        return Err(format!(
            "conversation adapter {} is not a built-in or external adapter",
            adapter.id
        ));
    }
    if !adapter.enabled {
        return Err(format!("conversation adapter is disabled: {}", adapter.id));
    }
    if !matches!(
        adapter.trust_state,
        ConversationAdapterTrustState::Trusted | ConversationAdapterTrustState::BuiltIn
    ) {
        return Err(format!(
            "external conversation adapter is not trusted: {}",
            adapter.id
        ));
    }
    if !adapter.input_kinds.iter().any(|kind| *kind == source.kind) {
        return Err(format!(
            "external conversation adapter {} does not support source kind {:?}",
            adapter.id, source.kind
        ));
    }
    if !adapter
        .capabilities
        .iter()
        .any(|capability| capability == method)
    {
        return Err(format!(
            "external conversation adapter {} does not declare {method}",
            adapter.id
        ));
    }
    Ok(())
}

fn validate_external_adapter_manifest_for_method(
    adapter: &ConversationAdapter,
    validation: &ExternalAdapterValidationResult,
    method: &str,
) -> AppResult<()> {
    if validation.manifest.id != adapter.id {
        return Err(format!(
            "external conversation adapter manifest id {} does not match registered adapter {}",
            validation.manifest.id, adapter.id
        ));
    }
    if !validation
        .manifest
        .capabilities
        .iter()
        .any(|capability| capability == method)
    {
        return Err(format!(
            "external conversation adapter {} does not declare {method}",
            adapter.id
        ));
    }
    if let Some(trusted_hash) = adapter.trusted_hash.as_deref() {
        if validation.content_hash != trusted_hash {
            return Err(format!(
                "external conversation adapter trusted hash mismatch: {}",
                adapter.id
            ));
        }
    }
    Ok(())
}

fn source_config_value(source: &ConversationSource) -> AppResult<Option<Value>> {
    match source.config_json.as_deref() {
        Some(text) if !text.trim().is_empty() => Ok(Some(
            serde_json::from_str::<Value>(text).map_err(|error| error.to_string())?,
        )),
        _ => Ok(None),
    }
}

pub(crate) fn scaffold_external_adapter(
    params: ExternalAdapterScaffoldParams,
) -> AppResult<ExternalAdapterScaffoldResult> {
    let target_dir = crate::backend::path_utils::expand_path(&params.directory)?;
    let manifest_path = target_dir.join("conversation-adapter.json");
    let request_fixture_path = target_dir
        .join("fixtures")
        .join("read-session.request.json");
    let response_fixture_path = target_dir
        .join("fixtures")
        .join("read-session.response.ndjson");
    let export_request_fixture_path = target_dir
        .join("fixtures")
        .join("export-markdown.request.json");
    let export_response_fixture_path = target_dir
        .join("fixtures")
        .join("export-markdown.response.ndjson");
    if params.dry_run {
        return Ok(ExternalAdapterScaffoldResult {
            dry_run: true,
            manifest_path: manifest_path.to_string_lossy().to_string(),
            request_fixture_path: request_fixture_path.to_string_lossy().to_string(),
            response_fixture_path: response_fixture_path.to_string_lossy().to_string(),
            export_request_fixture_path: export_request_fixture_path.to_string_lossy().to_string(),
            export_response_fixture_path: export_response_fixture_path
                .to_string_lossy()
                .to_string(),
        });
    }

    fs::create_dir_all(request_fixture_path.parent().unwrap())
        .map_err(|error| error.to_string())?;
    let runtime = scaffold_adapter_runtime(&params)?;
    write_scaffold_adapter_entrypoint(&target_dir, &runtime)?;
    let manifest = ConversationAdapterManifest {
        schema_version: 1,
        id: params.id,
        name: params.name,
        version: "0.1.0".to_string(),
        protocol_version: EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        command: Vec::new(),
        runtime: Some(runtime),
        capabilities: vec![
            "probe".to_string(),
            "list_sessions".to_string(),
            "read_session".to_string(),
            "export_markdown".to_string(),
        ],
        input_kinds: vec![
            ConversationSourceKind::Directory,
            ConversationSourceKind::File,
        ],
    };
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        &request_fixture_path,
        serde_json::to_string_pretty(&json!({
            "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
            "request_id": "fixture-read-session",
            "method": "read_session",
            "source": { "location": "/path/to/source", "config": null },
            "params": { "session_id": "example-session" }
        }))
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        &response_fixture_path,
        r#"{"type":"item","item":{"kind":"session","session":{"external_id":"example-session","title":"Example session","project_path":null,"started_at":null,"updated_at":null,"source_locator":null,"source_fingerprint":null,"turns":[{"external_id":"turn-1","turn_index":0,"user_text":"Example question","title":null,"started_at":null,"ended_at":null,"parts":[{"role":"assistant","kind":"text","text":"Example answer","language":null,"command":null,"cwd":null,"status":null,"exit_code":null,"metadata_json":{"content_card":{"type":"answer","format":"markdown"}}}]}]}}}
{"type":"complete","item":{"session_count":1,"turn_count":1}}
"#,
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        &export_request_fixture_path,
        serde_json::to_string_pretty(&json!({
            "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
            "request_id": "fixture-export-markdown",
            "method": "export_markdown",
            "source": { "location": "/path/to/source", "config": null },
            "params": {
                "session_detail": example_session_detail(),
                "question_ids": [],
                "content_filter": {
                    "answer": true,
                    "tool": true,
                    "command": true,
                    "code": true,
                    "result": true
                },
                "record_kind": "session",
                "default_relative_path": "example/Example-session.md"
            }
        }))
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        &export_response_fixture_path,
        r##"{"type":"item","item":{"kind":"markdown_export","content":"# Example session\n\n## 1. Example question\n\n### Answer\n\n```markdown\nExample answer\n```\n","relative_path":"example/Example-session.md"}}
{"type":"complete","item":{"export_count":1}}
"##,
    )
    .map_err(|error| error.to_string())?;

    Ok(ExternalAdapterScaffoldResult {
        dry_run: false,
        manifest_path: manifest_path.to_string_lossy().to_string(),
        request_fixture_path: request_fixture_path.to_string_lossy().to_string(),
        response_fixture_path: response_fixture_path.to_string_lossy().to_string(),
        export_request_fixture_path: export_request_fixture_path.to_string_lossy().to_string(),
        export_response_fixture_path: export_response_fixture_path.to_string_lossy().to_string(),
    })
}

fn write_scaffold_adapter_entrypoint(
    target_dir: &Path,
    runtime: &ConversationAdapterRuntime,
) -> AppResult<()> {
    let entry_path = resolve_command_path(target_dir, &runtime.entry);
    if entry_path.exists() {
        return Ok(());
    }
    if let Some(parent) = entry_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(
        &entry_path,
        scaffold_adapter_entrypoint_template(&runtime.kind),
    )
    .map_err(|error| error.to_string())?;
    #[cfg(unix)]
    if matches!(
        runtime.kind,
        ConversationAdapterRuntimeKind::Bash | ConversationAdapterRuntimeKind::Executable
    ) {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&entry_path)
            .map_err(|error| error.to_string())?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&entry_path, permissions).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn scaffold_adapter_entrypoint_template(kind: &ConversationAdapterRuntimeKind) -> &'static str {
    match kind {
        ConversationAdapterRuntimeKind::Node => NODE_ADAPTER_STARTER,
        ConversationAdapterRuntimeKind::Python => PYTHON_ADAPTER_STARTER,
        ConversationAdapterRuntimeKind::Bash => BASH_ADAPTER_STARTER,
        ConversationAdapterRuntimeKind::Executable => EXECUTABLE_ADAPTER_STARTER,
    }
}

fn scaffold_adapter_runtime(
    params: &ExternalAdapterScaffoldParams,
) -> AppResult<ConversationAdapterRuntime> {
    let kind = params
        .runtime_type
        .clone()
        .unwrap_or(ConversationAdapterRuntimeKind::Node);
    let entry = match params.runtime_entry.as_deref() {
        Some(entry) if entry.trim().is_empty() => {
            return Err("adapter runtime entry must not be empty".to_string());
        }
        Some(entry) => entry.trim().to_string(),
        None => default_scaffold_runtime_entry(&kind).to_string(),
    };
    validate_adapter_entry_path("adapter runtime entry", &entry)?;
    let version = match params.runtime_version.as_deref() {
        Some(version) if version.trim().is_empty() => {
            return Err("adapter runtime version must not be empty".to_string());
        }
        Some(version) => {
            let version = version.trim().to_string();
            validate_runtime_version_constraint(&version)?;
            Some(version)
        }
        None => default_scaffold_runtime_version(&kind).map(str::to_string),
    };
    Ok(ConversationAdapterRuntime {
        kind,
        entry,
        args: Vec::new(),
        version,
    })
}

fn default_scaffold_runtime_entry(kind: &ConversationAdapterRuntimeKind) -> &'static str {
    match kind {
        ConversationAdapterRuntimeKind::Node => "adapter.mjs",
        ConversationAdapterRuntimeKind::Python => "adapter.py",
        ConversationAdapterRuntimeKind::Bash => "adapter.sh",
        ConversationAdapterRuntimeKind::Executable => "adapter-executable",
    }
}

fn default_scaffold_runtime_version(kind: &ConversationAdapterRuntimeKind) -> Option<&'static str> {
    match kind {
        ConversationAdapterRuntimeKind::Node => Some(">=20"),
        ConversationAdapterRuntimeKind::Python => Some(">=3.10"),
        ConversationAdapterRuntimeKind::Bash | ConversationAdapterRuntimeKind::Executable => None,
    }
}

const NODE_ADAPTER_STARTER: &str = r##"#!/usr/bin/env node
const chunks = [];
for await (const chunk of process.stdin) chunks.push(chunk);
const request = JSON.parse(Buffer.concat(chunks).toString("utf8") || "{}");
const method = request.method;

function write(item) {
  process.stdout.write(`${JSON.stringify(item)}\n`);
}

if (method === "export_markdown") {
  write({
    type: "item",
    item: {
      kind: "markdown_export",
      content: "# Example session\n\n## 1. Example question\n\nExample answer\n",
      relative_path: request.params?.default_relative_path ?? "example/Example-session.md",
    },
  });
  write({ type: "complete", item: { export_count: 1 } });
} else if (method === "read_session") {
  write({
    type: "item",
    item: {
      kind: "session",
      session: {
        external_id: request.params?.session_id ?? "example-session",
        title: "Example session",
        project_path: null,
        started_at: null,
        updated_at: null,
        source_locator: null,
        source_fingerprint: null,
        turns: [],
      },
    },
  });
  write({ type: "complete", item: { session_count: 1, turn_count: 0 } });
} else {
  write({ type: "complete", item: {} });
}
"##;

const PYTHON_ADAPTER_STARTER: &str = r##"#!/usr/bin/env python3
import json
import sys

request = json.loads(sys.stdin.read() or "{}")
method = request.get("method")

def write(item):
    sys.stdout.write(json.dumps(item, separators=(",", ":")) + "\n")

if method == "export_markdown":
    write({
        "type": "item",
        "item": {
            "kind": "markdown_export",
            "content": "# Example session\n\n## 1. Example question\n\nExample answer\n",
            "relative_path": request.get("params", {}).get("default_relative_path", "example/Example-session.md"),
        },
    })
    write({"type": "complete", "item": {"export_count": 1}})
elif method == "read_session":
    write({
        "type": "item",
        "item": {
            "kind": "session",
            "session": {
                "external_id": request.get("params", {}).get("session_id") or "example-session",
                "title": "Example session",
                "project_path": None,
                "started_at": None,
                "updated_at": None,
                "source_locator": None,
                "source_fingerprint": None,
                "turns": [],
            },
        },
    })
    write({"type": "complete", "item": {"session_count": 1, "turn_count": 0}})
else:
    write({"type": "complete", "item": {}})
"##;

const BASH_ADAPTER_STARTER: &str = r##"#!/usr/bin/env bash
set -euo pipefail

request="$(cat)"
case "$request" in
  *'"method":"export_markdown"'*)
    printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"# Example session\n\n## 1. Example question\n\nExample answer\n","relative_path":"example/Example-session.md"}}'
    printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
    ;;
  *'"method":"read_session"'*)
    printf '%s\n' '{"type":"item","item":{"kind":"session","session":{"external_id":"example-session","title":"Example session","project_path":null,"started_at":null,"updated_at":null,"source_locator":null,"source_fingerprint":null,"turns":[]}}}'
    printf '%s\n' '{"type":"complete","item":{"session_count":1,"turn_count":0}}'
    ;;
  *)
    printf '%s\n' '{"type":"complete","item":{}}'
    ;;
esac
"##;

const EXECUTABLE_ADAPTER_STARTER: &str = BASH_ADAPTER_STARTER;

pub(crate) fn validate_external_adapter(
    params: ExternalAdapterValidateParams,
) -> AppResult<ExternalAdapterValidationResult> {
    validate_external_adapter_manifest(&params.manifest_path)
}

pub(crate) fn register_external_adapter(params: ExternalAdapterRegisterParams) -> AppResult<Value> {
    if !params.dry_run && !params.yes {
        return Err("conversation.adapter.register requires --yes".to_string());
    }
    let validation = validate_external_adapter_manifest(&params.manifest_path)?;
    let probe = if params.dry_run {
        None
    } else {
        Some(probe_external_adapter_before_trust(&validation)?)
    };
    let now = Utc::now().to_rfc3339();
    let adapter = ConversationAdapter {
        id: validation.manifest.id.clone(),
        name: validation.manifest.name.clone(),
        kind: ConversationAdapterKind::External,
        version: validation.manifest.version.clone(),
        enabled: true,
        manifest_path: Some(validation.manifest_path.clone()),
        executable_path: Some(validation.executable_path.clone()),
        content_hash: Some(validation.content_hash.clone()),
        trusted_hash: Some(validation.content_hash.clone()),
        trust_state: ConversationAdapterTrustState::Trusted,
        protocol_version: Some(validation.manifest.protocol_version),
        capabilities: validation.manifest.capabilities.clone(),
        input_kinds: validation.manifest.input_kinds.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    Ok(json!({
        "dry_run": params.dry_run,
        "adapter": adapter,
        "probe": probe,
        "validation": validation
    }))
}

fn probe_external_adapter_before_trust(
    validation: &ExternalAdapterValidationResult,
) -> AppResult<ExternalAdapterRunResult> {
    if !validation
        .manifest
        .capabilities
        .iter()
        .any(|capability| capability == "probe")
    {
        return Err(format!(
            "adapter {} must declare probe before it can be trusted",
            validation.manifest.id
        ));
    }
    let manifest_dir = Path::new(&validation.manifest_path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let request = json!({
        "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        "request_id": format!("trust-probe-{}", Utc::now().timestamp_millis()),
        "method": "probe",
        "source": { "location": manifest_dir, "config": null },
        "params": {}
    });
    run_external_adapter(
        validation,
        "probe",
        request,
        Duration::from_millis(DEFAULT_PROBE_TIMEOUT_MS),
    )
    .map_err(|error| {
        format!(
            "adapter {} probe failed; refusing to trust: {error}",
            validation.manifest.id
        )
    })
}

pub(crate) fn try_run_external_adapter(
    params: ExternalAdapterTryRunParams,
) -> AppResult<ExternalAdapterRunResult> {
    if !params.yes {
        return Err("conversation.adapter.try-run requires --yes".to_string());
    }
    let validation = validate_external_adapter_manifest(&params.manifest_path)?;
    let method = params.method.trim();
    if !validation
        .manifest
        .capabilities
        .iter()
        .any(|capability| capability == method)
    {
        return Err(format!("adapter does not declare capability: {method}"));
    }
    let timeout_ms = match method {
        "probe" => DEFAULT_PROBE_TIMEOUT_MS,
        "list_sessions" => DEFAULT_LIST_TIMEOUT_MS,
        "read_session" => DEFAULT_READ_TIMEOUT_MS,
        "export_markdown" => DEFAULT_READ_TIMEOUT_MS,
        other => return Err(format!("unsupported adapter method: {other}")),
    };
    let location = params.location.unwrap_or_else(|| ".".to_string());
    let request_params = if method == "export_markdown" {
        json!({
            "session_detail": example_session_detail(),
            "question_ids": params.session_id.as_ref().map(|id| vec![id.clone()]).unwrap_or_default(),
            "content_filter": {
                "answer": true,
                "tool": true,
                "command": true,
                "code": true,
                "result": true
            },
            "record_kind": "session",
            "default_relative_path": "fixture-external/fixture-project/example-session.md"
        })
    } else {
        json!({ "session_id": params.session_id })
    };
    let request = json!({
        "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        "request_id": format!("try-run-{}", Utc::now().timestamp_millis()),
        "method": method,
        "source": { "location": location, "config": null },
        "params": request_params
    });
    run_external_adapter(
        &validation,
        method,
        request,
        Duration::from_millis(timeout_ms),
    )
}

pub(crate) fn adapter_from_registration_preview(value: Value) -> AppResult<ConversationAdapter> {
    let adapter = value
        .get("adapter")
        .cloned()
        .ok_or_else(|| "registration preview did not include adapter".to_string())?;
    serde_json::from_value(adapter).map_err(|error| error.to_string())
}

pub(crate) fn list_conversation_adapter_runtime_statuses(
    adapters: &[ConversationAdapter],
    sources: &[ConversationSource],
) -> AppResult<Vec<ConversationAdapterRuntimeStatus>> {
    let mut requirements = adapter_runtime_requirements(adapters);
    super::harvester::append_harvester_runtime_requirements(&mut requirements, sources);
    Ok(list_adapter_runtime_statuses(&requirements))
}

pub(super) fn validate_external_adapter_manifest(
    manifest_path: &str,
) -> AppResult<ExternalAdapterValidationResult> {
    let path = crate::backend::path_utils::expand_path(manifest_path)?;
    if !path.is_file() {
        return Err(format!("adapter manifest not found: {}", path.display()));
    }
    let manifest_text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let manifest: ConversationAdapterManifest =
        serde_json::from_str(&manifest_text).map_err(|error| error.to_string())?;
    validate_manifest_shape(&manifest)?;
    let manifest_dir = path
        .parent()
        .ok_or_else(|| "adapter manifest path has no parent directory".to_string())?;
    let executable_path = resolve_adapter_entry_path(manifest_dir, &manifest)?;
    let executable_hash = if executable_path.is_file() {
        Some(hash_file(&executable_path)?)
    } else {
        None
    };
    let manifest_hash = hash_bytes(manifest_text.as_bytes());
    let content_hash = adapter_content_hash(&manifest_hash, executable_hash.as_deref());
    let mut warnings = Vec::new();
    if executable_hash.is_none() {
        warnings.push(format!(
            "executable does not exist locally or cannot be hashed: {}",
            executable_path.display()
        ));
    }
    Ok(ExternalAdapterValidationResult {
        valid: true,
        manifest_path: path.to_string_lossy().to_string(),
        content_hash,
        manifest_hash,
        executable_path: executable_path.to_string_lossy().to_string(),
        executable_hash,
        manifest,
        warnings,
    })
}

fn adapter_content_hash(manifest_hash: &str, executable_hash: Option<&str>) -> String {
    let executable_hash = executable_hash.unwrap_or("");
    hash_bytes(format!("manifest:{manifest_hash}\nexecutable:{executable_hash}").as_bytes())
}

fn validate_manifest_shape(manifest: &ConversationAdapterManifest) -> AppResult<()> {
    if manifest.schema_version != 1 {
        return Err("adapter schema_version must be 1".to_string());
    }
    if manifest.protocol_version != EXTERNAL_ADAPTER_PROTOCOL_VERSION {
        return Err(format!(
            "adapter protocol_version must be {EXTERNAL_ADAPTER_PROTOCOL_VERSION}"
        ));
    }
    if manifest.id.trim().is_empty() {
        return Err("adapter id is required".to_string());
    }
    if manifest.name.trim().is_empty() {
        return Err("adapter name is required".to_string());
    }
    if manifest.runtime.is_some() && !manifest.command.is_empty() {
        return Err("adapter manifest must not declare both runtime and command".to_string());
    }
    match manifest.runtime.as_ref() {
        Some(runtime) => {
            if runtime.entry.trim().is_empty() {
                return Err("adapter runtime entry is required".to_string());
            }
            validate_adapter_entry_path("adapter runtime entry", &runtime.entry)?;
            if runtime
                .version
                .as_deref()
                .is_some_and(|version| version.trim().is_empty())
            {
                return Err("adapter runtime version must not be empty".to_string());
            }
            if let Some(version) = runtime.version.as_deref() {
                validate_runtime_version_constraint(version)?;
            }
        }
        None if manifest.command.is_empty() || manifest.command[0].trim().is_empty() => {
            return Err("adapter command must include an executable".to_string());
        }
        None => validate_adapter_entry_path("adapter command", &manifest.command[0])?,
    }
    for capability in &manifest.capabilities {
        if !matches!(
            capability.as_str(),
            "probe" | "list_sessions" | "read_session" | "export_markdown" | "web_records"
        ) {
            return Err(format!("unsupported adapter capability: {capability}"));
        }
    }
    Ok(())
}

fn validate_adapter_entry_path(field: &str, raw: &str) -> AppResult<()> {
    let trimmed = raw.trim();
    let path = Path::new(trimmed);
    if path.is_absolute() || looks_like_windows_rooted_path(trimmed) {
        return Err(format!(
            "{field} must be a relative path inside the adapter directory"
        ));
    }
    if trimmed
        .split(['/', '\\'])
        .any(|component| component == "..")
    {
        return Err(format!("{field} must not escape the adapter directory"));
    }
    Ok(())
}

fn looks_like_windows_rooted_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    if path.starts_with("\\\\") || path.starts_with('\\') {
        return true;
    }
    bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

pub(super) fn run_external_adapter(
    validation: &ExternalAdapterValidationResult,
    method: &str,
    request: Value,
    timeout: Duration,
) -> AppResult<ExternalAdapterRunResult> {
    let manifest = &validation.manifest;
    let manifest_dir = Path::new(&validation.manifest_path)
        .parent()
        .ok_or_else(|| "adapter manifest path has no parent directory".to_string())?;
    let execution_runtime = adapter_execution_runtime(manifest);
    let invocation = match execution_runtime.as_ref() {
        Some(runtime) => build_adapter_runtime_invocation(manifest_dir, runtime, &[]),
        None => build_adapter_invocation(manifest_dir, manifest)?,
    };
    if let Some(runtime) = execution_runtime.as_ref() {
        ensure_adapter_runtime_available(runtime, &invocation)?;
    }
    let mut child = Command::new(&invocation.program)
        .args(&invocation.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!(
                "failed to start adapter {}: {error}",
                invocation.display_path.display()
            )
        })?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "adapter stdin was not available".to_string())?;
    let request_text = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
    thread::spawn(move || {
        let _ = stdin.write_all(&request_text);
        let _ = stdin.write_all(b"\n");
    });

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "adapter stdout was not available".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "adapter stderr was not available".to_string())?;
    let stdout_reader = thread::spawn(move || read_capped(stdout, DEFAULT_MAX_TOTAL_BYTES));
    let stderr_reader = thread::spawn(move || read_capped(stderr, 1024 * 1024));

    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            let stdout = stdout_reader
                .join()
                .map_err(|_| "adapter stdout reader panicked".to_string())??;
            let stderr = stderr_reader
                .join()
                .map_err(|_| "adapter stderr reader panicked".to_string())??;
            if !status.success() {
                return Err(format!(
                    "adapter exited with status {status}: {}",
                    String::from_utf8_lossy(&stderr)
                ));
            }
            return parse_external_adapter_output(method, stdout, stderr);
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            return Err(format!(
                "adapter timed out after {} ms",
                timeout.as_millis()
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

pub(super) fn parse_external_adapter_output(
    method: &str,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
) -> AppResult<ExternalAdapterRunResult> {
    let stdout = String::from_utf8(stdout).map_err(|error| error.to_string())?;
    let stderr = String::from_utf8_lossy(&stderr).to_string();
    let mut session_descriptors = Vec::new();
    let mut snapshot_complete = false;
    let mut sessions = Vec::new();
    let mut markdown_export = None;
    let mut warnings = Vec::new();
    let mut saw_complete = false;
    let mut item_count = 0usize;

    for (index, line) in stdout.lines().enumerate() {
        let line_bytes = line.as_bytes().len();
        validate_external_adapter_item_line_size(index + 1, line_bytes)?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: ExternalAdapterLine = serde_json::from_str(line)
            .map_err(|error| format!("invalid adapter NDJSON line {}: {error}", index + 1))?;
        let is_large_item = parsed.kind == "item"
            && parsed.item.as_ref().is_some_and(|item| {
                matches!(adapter_item_kind(item), "session" | "markdown_export")
            });
        validate_external_adapter_line_size(index + 1, line_bytes, is_large_item)?;
        match parsed.kind.as_str() {
            "item" => {
                item_count += 1;
                let item = parsed
                    .item
                    .ok_or_else(|| format!("adapter item line {} missing item", index + 1))?;
                match adapter_item_kind(&item) {
                    "session_descriptor" => {
                        session_descriptors.push(parse_adapter_session_descriptor_item(item)?);
                    }
                    "session" => {
                        if let Some(session) = parse_adapter_session_item(item)? {
                            sessions.push(session);
                        }
                    }
                    "markdown_export" => {
                        if markdown_export.is_some() {
                            return Err(format!(
                                "adapter returned multiple markdown_export items by line {}",
                                index + 1
                            ));
                        }
                        markdown_export = Some(parse_adapter_markdown_export_item(item)?);
                    }
                    _ => {}
                }
            }
            "warning" => warnings.push(
                parsed
                    .message
                    .unwrap_or_else(|| "adapter warning".to_string()),
            ),
            "complete" => {
                saw_complete = true;
                snapshot_complete = parsed
                    .item
                    .as_ref()
                    .and_then(|item| item.get("snapshot_complete"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            }
            "error" => {
                return Err(format!(
                    "adapter returned error on line {}: {}",
                    index + 1,
                    parsed
                        .error
                        .map(|value| value.to_string())
                        .or(parsed.message)
                        .unwrap_or_else(|| "unknown adapter error".to_string())
                ));
            }
            other => {
                return Err(format!(
                    "unsupported adapter output type on line {}: {other}",
                    index + 1
                ))
            }
        }
    }
    if !saw_complete {
        return Err("adapter output did not include a complete line".to_string());
    }
    Ok(ExternalAdapterRunResult {
        method: method.to_string(),
        item_count,
        warning_count: warnings.len(),
        session_descriptors,
        snapshot_complete,
        sessions,
        markdown_export,
        warnings,
        stderr,
    })
}

fn parse_adapter_session_descriptor_item(item: Value) -> AppResult<ConversationSessionDescriptor> {
    let descriptor_value = item.get("descriptor").cloned().unwrap_or(item);
    let descriptor: ConversationSessionDescriptor =
        serde_json::from_value(descriptor_value).map_err(|error| error.to_string())?;
    if descriptor.external_id.trim().is_empty() {
        return Err("session descriptor external_id is required".to_string());
    }
    if descriptor.version_token.trim().is_empty() {
        return Err("session descriptor version_token is required".to_string());
    }
    Ok(descriptor)
}

pub(super) fn validate_external_adapter_line_size(
    line_number: usize,
    line_bytes: usize,
    is_large_item: bool,
) -> AppResult<()> {
    validate_external_adapter_item_line_size(line_number, line_bytes)?;
    if line_bytes > DEFAULT_MAX_CONTROL_LINE_BYTES && !is_large_item {
        return Err(format!(
            "adapter output line {line_number} exceeds max control line size ({line_bytes} bytes > {DEFAULT_MAX_CONTROL_LINE_BYTES} bytes)"
        ));
    }
    Ok(())
}

fn validate_external_adapter_item_line_size(
    line_number: usize,
    line_bytes: usize,
) -> AppResult<()> {
    if line_bytes > DEFAULT_MAX_ITEM_LINE_BYTES {
        return Err(format!(
            "adapter output line {line_number} exceeds max item line size ({line_bytes} bytes > {DEFAULT_MAX_ITEM_LINE_BYTES} bytes)"
        ));
    }
    Ok(())
}

fn adapter_item_kind(item: &Value) -> &str {
    item.get("kind")
        .and_then(Value::as_str)
        .unwrap_or("session")
}

fn parse_adapter_markdown_export_item(item: Value) -> AppResult<ExternalMarkdownExport> {
    let export_value = item.get("export").cloned().unwrap_or(item);
    let export: ExternalMarkdownExport =
        serde_json::from_value(export_value).map_err(|error| error.to_string())?;
    if export.content.is_empty() {
        return Err("markdown_export content is required".to_string());
    }
    if export.relative_path.trim().is_empty() {
        return Err("markdown_export relative_path is required".to_string());
    }
    Ok(export)
}

fn parse_adapter_session_item(item: Value) -> AppResult<Option<NormalizedConversationSession>> {
    let kind = adapter_item_kind(&item);
    if kind != "session" {
        return Ok(None);
    }
    let session_value = item.get("session").cloned().unwrap_or(item);
    let session: NormalizedConversationSession =
        serde_json::from_value(session_value).map_err(|error| error.to_string())?;
    validate_normalized_session(&session)?;
    Ok(Some(session))
}

fn validate_normalized_session(session: &NormalizedConversationSession) -> AppResult<()> {
    if session.external_id.trim().is_empty() {
        return Err("normalized session external_id is required".to_string());
    }
    for turn in &session.turns {
        if turn.external_id.trim().is_empty() {
            return Err("normalized turn external_id is required".to_string());
        }
        if turn.user_text.trim().is_empty() {
            return Err("normalized turn user_text is required".to_string());
        }
    }
    Ok(())
}

fn example_session_detail() -> Value {
    json!({
        "session": {
            "id": "example-session",
            "source_id": "fixture-source",
            "adapter_id": "fixture-external",
            "external_id": "example-session",
            "title": "Example session",
            "project_path": "/tmp/fixture-project",
            "started_at": null,
            "updated_at": null,
            "source_locator": null,
            "source_fingerprint": null,
            "imported_at": "2026-01-01T00:00:00Z",
            "missing": false
        },
        "questions": [{
            "question": {
                "id": "example-question",
                "session_id": "example-session",
                "question_index": 0,
                "title": null,
                "question_text": "Example question",
                "answer_text": "Example answer",
                "code_text": null,
                "command_text": null,
                "grouping_origin": "imported",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z"
            },
            "turns": [{
                "id": "example-turn",
                "session_id": "example-session",
                "external_id": "example-turn",
                "turn_index": 0,
                "user_text": "Example question",
                "title": null,
                "started_at": null,
                "ended_at": null,
                "fingerprint": "example",
                "missing": false,
                "imported_at": "2026-01-01T00:00:00Z"
            }],
            "parts": [{
                "id": "example-part",
                "turn_id": "example-turn",
                "part_index": 0,
                "role": "assistant",
                "kind": "text",
                "text": "Example answer",
                "language": null,
                "command": null,
                "cwd": null,
                "status": null,
                "exit_code": null,
                "metadata_json": "{\"content_card\":{\"type\":\"answer\",\"format\":\"markdown\"}}"
            }]
        }]
    })
}
