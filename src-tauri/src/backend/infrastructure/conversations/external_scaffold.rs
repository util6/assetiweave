use super::external_manifest::validate_adapter_entry_path;
use super::external_parser::example_session_detail;
use super::prelude::*;

pub(crate) fn scaffold_external_adapter(
    params: ExternalAdapterScaffoldParams,
) -> InfraResult<ExternalAdapterScaffoldResult> {
    let target_dir = crate::backend::infrastructure::path_utils::expand_path(&params.directory)?;
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

    fs::create_dir_all(request_fixture_path.parent().unwrap()).map_err(InfraError::external)?;
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
        card_contract_version: None,
        card_kinds: Vec::new(),
    };
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(InfraError::external)?,
    )
    .map_err(InfraError::external)?;
    fs::write(
        &request_fixture_path,
        serde_json::to_string_pretty(&json!({
            "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
            "request_id": "fixture-read-session",
            "method": "read_session",
            "source": { "location": "/path/to/source", "config": null },
            "params": { "session_id": "example-session" }
        }))
        .map_err(InfraError::external)?,
    )
    .map_err(InfraError::external)?;
    fs::write(
        &response_fixture_path,
        r#"{"type":"item","item":{"kind":"session","session":{"external_id":"example-session","title":"Example session","project_path":null,"started_at":null,"updated_at":null,"source_locator":null,"source_fingerprint":null,"turns":[{"external_id":"turn-1","turn_index":0,"user_text":"Example question","title":null,"started_at":null,"ended_at":null,"parts":[{"role":"assistant","kind":"text","text":"Example answer","language":null,"command":null,"cwd":null,"status":null,"exit_code":null,"metadata_json":{"content_card":{"type":"answer","format":"markdown"}}}]}]}}}
{"type":"complete","item":{"session_count":1,"turn_count":1}}
"#,
    )
    .map_err(InfraError::external)?;
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
        .map_err(InfraError::external)?,
    )
    .map_err(InfraError::external)?;
    fs::write(
        &export_response_fixture_path,
        r##"{"type":"item","item":{"kind":"markdown_export","content":"# Example session\n\n## 1. Example question\n\n### Answer\n\n```markdown\nExample answer\n```\n","relative_path":"example/Example-session.md"}}
{"type":"complete","item":{"export_count":1}}
"##,
    )
    .map_err(InfraError::external)?;

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
) -> InfraResult<()> {
    let entry_path = resolve_command_path(target_dir, &runtime.entry);
    if entry_path.exists() {
        return Ok(());
    }
    if let Some(parent) = entry_path.parent() {
        fs::create_dir_all(parent).map_err(InfraError::external)?;
    }
    fs::write(
        &entry_path,
        scaffold_adapter_entrypoint_template(&runtime.kind),
    )
    .map_err(InfraError::external)?;
    #[cfg(unix)]
    if matches!(
        runtime.kind,
        ConversationAdapterRuntimeKind::Bash | ConversationAdapterRuntimeKind::Executable
    ) {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&entry_path)
            .map_err(InfraError::external)?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&entry_path, permissions).map_err(InfraError::external)?;
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
) -> InfraResult<ConversationAdapterRuntime> {
    let kind = params
        .runtime_type
        .clone()
        .unwrap_or(ConversationAdapterRuntimeKind::Node);
    let entry = match params.runtime_entry.as_deref() {
        Some(entry) if entry.trim().is_empty() => {
            return Err(InfraError::external(
                "adapter runtime entry must not be empty".to_string(),
            ));
        }
        Some(entry) => entry.trim().to_string(),
        None => default_scaffold_runtime_entry(&kind).to_string(),
    };
    validate_adapter_entry_path("adapter runtime entry", &entry)?;
    let version = match params.runtime_version.as_deref() {
        Some(version) if version.trim().is_empty() => {
            return Err(InfraError::external(
                "adapter runtime version must not be empty".to_string(),
            ));
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
