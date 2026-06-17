use crate::backend::dto::AppResult;
use crate::backend::models::{
    split_markdown_text_parts, ConversationAdapter, ConversationAdapterKind,
    ConversationAdapterTrustState, ConversationPartKind, ConversationPartRole, ConversationSource,
    ConversationSourceKind, NormalizedConversationPart, NormalizedConversationSession,
    NormalizedConversationTurn,
};
use chrono::Utc;
use rusqlite::{params, types::ValueRef, Connection, Row};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const EXTERNAL_ADAPTER_PROTOCOL_VERSION: u32 = 1;
const DEFAULT_PROBE_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_LIST_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_READ_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_MAX_LINE_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_MAX_TOTAL_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterManifest {
    #[serde(alias = "schemaVersion")]
    pub(crate) schema_version: u32,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    #[serde(alias = "protocolVersion")]
    pub(crate) protocol_version: u32,
    pub(crate) command: Vec<String>,
    pub(crate) capabilities: Vec<String>,
    #[serde(alias = "inputKinds")]
    pub(crate) input_kinds: Vec<ConversationSourceKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ExternalAdapterRegisterParams {
    #[serde(alias = "manifestPath")]
    pub(crate) manifest_path: String,
    #[serde(default)]
    pub(crate) yes: bool,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ExternalAdapterScaffoldParams {
    pub(crate) directory: String,
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ExternalAdapterValidateParams {
    #[serde(alias = "manifestPath")]
    pub(crate) manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ExternalAdapterTryRunParams {
    #[serde(alias = "manifestPath")]
    pub(crate) manifest_path: String,
    pub(crate) method: String,
    pub(crate) location: Option<String>,
    #[serde(default, alias = "sessionId")]
    pub(crate) session_id: Option<String>,
    #[serde(default)]
    pub(crate) yes: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExternalAdapterValidationResult {
    pub(crate) valid: bool,
    pub(crate) manifest_path: String,
    pub(crate) manifest_hash: String,
    pub(crate) executable_path: String,
    pub(crate) executable_hash: Option<String>,
    pub(crate) manifest: ConversationAdapterManifest,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExternalAdapterScaffoldResult {
    pub(crate) dry_run: bool,
    pub(crate) manifest_path: String,
    pub(crate) request_fixture_path: String,
    pub(crate) response_fixture_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExternalAdapterRunResult {
    pub(crate) method: String,
    pub(crate) item_count: usize,
    pub(crate) warning_count: usize,
    pub(crate) sessions: Vec<NormalizedConversationSession>,
    pub(crate) warnings: Vec<String>,
    pub(crate) stderr: String,
}

#[derive(Debug, Deserialize)]
struct ExternalAdapterLine {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    item: Option<Value>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    error: Option<Value>,
}

pub(crate) fn read_source_sessions(
    source: &ConversationSource,
) -> AppResult<Vec<NormalizedConversationSession>> {
    match source.adapter_id.as_str() {
        "codex" => read_codex_sessions(source),
        "claude-code" => read_claude_code_sessions(source),
        "opencode" => read_opencode_sessions(source),
        _ => Err(format!(
            "external adapter sync is only available through conversation.adapter.try-run in this build: {}",
            source.adapter_id
        )),
    }
}

pub(crate) fn read_source_sessions_with_adapter(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
) -> AppResult<Vec<NormalizedConversationSession>> {
    match source.adapter_id.as_str() {
        "codex" | "claude-code" | "opencode" => read_source_sessions(source),
        _ => {
            let adapter = adapter
                .ok_or_else(|| format!("conversation adapter not found: {}", source.adapter_id))?;
            read_external_adapter_sessions(adapter, source)
        }
    }
}

fn read_external_adapter_sessions(
    adapter: &ConversationAdapter,
    source: &ConversationSource,
) -> AppResult<Vec<NormalizedConversationSession>> {
    if adapter.kind != ConversationAdapterKind::External {
        return Err(format!(
            "conversation adapter {} is not a built-in or external adapter",
            adapter.id
        ));
    }
    if !adapter.enabled {
        return Err(format!("conversation adapter is disabled: {}", adapter.id));
    }
    if adapter.trust_state != ConversationAdapterTrustState::Trusted {
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
    let manifest_path = adapter.manifest_path.as_deref().ok_or_else(|| {
        format!(
            "external conversation adapter has no manifest: {}",
            adapter.id
        )
    })?;
    let validation = validate_external_adapter_manifest(manifest_path)?;
    if !validation
        .manifest
        .capabilities
        .iter()
        .any(|capability| capability == "read_session")
    {
        return Err(format!(
            "external conversation adapter {} does not declare read_session",
            adapter.id
        ));
    }
    let config = match source.config_json.as_deref() {
        Some(text) if !text.trim().is_empty() => {
            Some(serde_json::from_str::<Value>(text).map_err(|error| error.to_string())?)
        }
        _ => None,
    };
    let request = json!({
        "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        "request_id": format!("sync-{}-{}", source.id, Utc::now().timestamp_millis()),
        "method": "read_session",
        "source": { "location": source.location, "config": config },
        "params": { "session_id": null }
    });
    Ok(run_external_adapter(
        &validation,
        "read_session",
        request,
        Duration::from_millis(DEFAULT_READ_TIMEOUT_MS),
    )?
    .sessions)
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
    if params.dry_run {
        return Ok(ExternalAdapterScaffoldResult {
            dry_run: true,
            manifest_path: manifest_path.to_string_lossy().to_string(),
            request_fixture_path: request_fixture_path.to_string_lossy().to_string(),
            response_fixture_path: response_fixture_path.to_string_lossy().to_string(),
        });
    }

    fs::create_dir_all(request_fixture_path.parent().unwrap())
        .map_err(|error| error.to_string())?;
    let manifest = ConversationAdapterManifest {
        schema_version: 1,
        id: params.id,
        name: params.name,
        version: "0.1.0".to_string(),
        protocol_version: EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        command: vec!["/absolute/path/to/adapter-executable".to_string()],
        capabilities: vec![
            "probe".to_string(),
            "list_sessions".to_string(),
            "read_session".to_string(),
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
        r#"{"type":"item","item":{"kind":"session","session":{"external_id":"example-session","title":"Example session","project_path":null,"started_at":null,"updated_at":null,"source_locator":null,"source_fingerprint":null,"turns":[{"external_id":"turn-1","turn_index":0,"user_text":"Example question","title":null,"started_at":null,"ended_at":null,"parts":[{"role":"assistant","kind":"text","text":"Example answer","language":null,"command":null,"cwd":null,"status":null,"exit_code":null,"metadata_json":null}]}]}}}
{"type":"complete","item":{"session_count":1,"turn_count":1}}
"#,
    )
    .map_err(|error| error.to_string())?;

    Ok(ExternalAdapterScaffoldResult {
        dry_run: false,
        manifest_path: manifest_path.to_string_lossy().to_string(),
        request_fixture_path: request_fixture_path.to_string_lossy().to_string(),
        response_fixture_path: response_fixture_path.to_string_lossy().to_string(),
    })
}

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
    let now = Utc::now().to_rfc3339();
    let adapter = ConversationAdapter {
        id: validation.manifest.id.clone(),
        name: validation.manifest.name.clone(),
        kind: ConversationAdapterKind::External,
        version: validation.manifest.version.clone(),
        enabled: true,
        manifest_path: Some(validation.manifest_path.clone()),
        executable_path: Some(validation.executable_path.clone()),
        content_hash: validation.executable_hash.clone(),
        trusted_hash: validation
            .executable_hash
            .clone()
            .or(Some(validation.manifest_hash.clone())),
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
        "validation": validation
    }))
}

pub(crate) fn try_run_external_adapter(
    params: ExternalAdapterTryRunParams,
) -> AppResult<ExternalAdapterRunResult> {
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
        other => return Err(format!("unsupported adapter method: {other}")),
    };
    let location = params.location.unwrap_or_else(|| ".".to_string());
    let request = json!({
        "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        "request_id": format!("try-run-{}", Utc::now().timestamp_millis()),
        "method": method,
        "source": { "location": location, "config": null },
        "params": { "session_id": params.session_id }
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

fn validate_external_adapter_manifest(
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
    let executable_path = resolve_command_path(manifest_dir, &manifest.command[0]);
    let executable_hash = if executable_path.is_file() {
        Some(hash_file(&executable_path)?)
    } else {
        None
    };
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
        manifest_hash: hash_bytes(manifest_text.as_bytes()),
        executable_path: executable_path.to_string_lossy().to_string(),
        executable_hash,
        manifest,
        warnings,
    })
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
    if manifest.command.is_empty() || manifest.command[0].trim().is_empty() {
        return Err("adapter command must include an executable".to_string());
    }
    for capability in &manifest.capabilities {
        if !matches!(
            capability.as_str(),
            "probe" | "list_sessions" | "read_session" | "web_records"
        ) {
            return Err(format!("unsupported adapter capability: {capability}"));
        }
    }
    Ok(())
}

fn run_external_adapter(
    validation: &ExternalAdapterValidationResult,
    method: &str,
    request: Value,
    timeout: Duration,
) -> AppResult<ExternalAdapterRunResult> {
    let manifest = &validation.manifest;
    let manifest_dir = Path::new(&validation.manifest_path)
        .parent()
        .ok_or_else(|| "adapter manifest path has no parent directory".to_string())?;
    let executable = resolve_command_path(manifest_dir, &manifest.command[0]);
    let args = manifest.command.iter().skip(1).collect::<Vec<_>>();
    let mut child = Command::new(&executable)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start adapter {}: {error}", executable.display()))?;

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

fn parse_external_adapter_output(
    method: &str,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
) -> AppResult<ExternalAdapterRunResult> {
    let stdout = String::from_utf8(stdout).map_err(|error| error.to_string())?;
    let stderr = String::from_utf8_lossy(&stderr).to_string();
    let mut sessions = Vec::new();
    let mut warnings = Vec::new();
    let mut saw_complete = false;
    let mut item_count = 0usize;

    for (index, line) in stdout.lines().enumerate() {
        if line.as_bytes().len() > DEFAULT_MAX_LINE_BYTES {
            return Err(format!(
                "adapter output line {} exceeds max line size",
                index + 1
            ));
        }
        if line.trim().is_empty() {
            continue;
        }
        let parsed: ExternalAdapterLine = serde_json::from_str(line)
            .map_err(|error| format!("invalid adapter NDJSON line {}: {error}", index + 1))?;
        match parsed.kind.as_str() {
            "item" => {
                item_count += 1;
                let item = parsed
                    .item
                    .ok_or_else(|| format!("adapter item line {} missing item", index + 1))?;
                if let Some(session) = parse_adapter_session_item(item)? {
                    sessions.push(session);
                }
            }
            "warning" => warnings.push(
                parsed
                    .message
                    .unwrap_or_else(|| "adapter warning".to_string()),
            ),
            "complete" => saw_complete = true,
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
        sessions,
        warnings,
        stderr,
    })
}

fn parse_adapter_session_item(item: Value) -> AppResult<Option<NormalizedConversationSession>> {
    let kind = item
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("session");
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

fn read_codex_sessions(
    source: &ConversationSource,
) -> AppResult<Vec<NormalizedConversationSession>> {
    let root = crate::backend::path_utils::expand_path(&source.location)?;
    let db_path = if root.is_dir() {
        root.join("state_5.sqlite")
    } else {
        root.clone()
    };
    if !db_path.is_file() {
        return Ok(Vec::new());
    }
    let conn = Connection::open(&db_path).map_err(|error| error.to_string())?;
    let columns = table_columns(&conn, "threads")?;
    if columns.is_empty() {
        return Ok(Vec::new());
    }
    let id_col = pick_column(&columns, &["id", "thread_id", "session_id"])
        .ok_or_else(|| "Codex threads table has no id column".to_string())?;
    let rollout_col = pick_column(
        &columns,
        &["rollout_path", "path", "file_path", "jsonl_path"],
    )
    .ok_or_else(|| "Codex threads table has no rollout path column".to_string())?;
    let title_col = pick_column(&columns, &["title", "name"]);
    let updated_col = pick_column(
        &columns,
        &["updated_at", "last_updated_at", "mtime", "created_at"],
    );
    let query = format!(
        "SELECT {id_col}, {rollout_col}, {} , {} FROM threads ORDER BY rowid DESC LIMIT 500",
        title_col.unwrap_or("NULL"),
        updated_col.unwrap_or("NULL")
    );
    let mut stmt = conn.prepare(&query).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                required_cell_string(row, 0)?,
                optional_cell_string(row, 1)?,
                optional_cell_string(row, 2)?,
                optional_cell_string(row, 3)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut sessions = Vec::new();
    for row in rows {
        let (external_id, rollout_path, title, updated_at) =
            row.map_err(|error| error.to_string())?;
        let Some(rollout_path) = rollout_path else {
            continue;
        };
        let path = crate::backend::path_utils::expand_path(&rollout_path)
            .unwrap_or_else(|_| PathBuf::from(&rollout_path));
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let turns = parse_jsonl_conversation(&text, ParserFlavor::Codex)?;
        if turns.is_empty() {
            continue;
        }
        sessions.push(NormalizedConversationSession {
            external_id,
            title,
            project_path: infer_project_path_from_turns(&turns),
            started_at: turns.first().and_then(|turn| turn.started_at.clone()),
            updated_at,
            source_locator: Some(path.to_string_lossy().to_string()),
            source_fingerprint: Some(hash_bytes(text.as_bytes())),
            turns,
        });
    }
    Ok(sessions)
}

fn read_claude_code_sessions(
    source: &ConversationSource,
) -> AppResult<Vec<NormalizedConversationSession>> {
    let root = crate::backend::path_utils::expand_path(&source.location)?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let files = if root.is_file() {
        vec![root]
    } else {
        collect_files_with_extension(&root, "jsonl", 1000)?
    };
    let mut sessions = Vec::new();
    for path in files {
        let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let turns = parse_jsonl_conversation(&text, ParserFlavor::ClaudeCode)?;
        if turns.is_empty() {
            continue;
        }
        let external_id = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("claude-session")
            .to_string();
        sessions.push(NormalizedConversationSession {
            external_id,
            title: path
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .map(|name| name.replace('-', "/")),
            project_path: infer_project_path_from_turns(&turns),
            started_at: turns.first().and_then(|turn| turn.started_at.clone()),
            updated_at: turns.last().and_then(|turn| turn.ended_at.clone()),
            source_locator: Some(path.to_string_lossy().to_string()),
            source_fingerprint: Some(hash_bytes(text.as_bytes())),
            turns,
        });
    }
    Ok(sessions)
}

fn read_opencode_sessions(
    source: &ConversationSource,
) -> AppResult<Vec<NormalizedConversationSession>> {
    let db_path = crate::backend::path_utils::expand_path(&source.location)?;
    if !db_path.is_file() {
        return Ok(Vec::new());
    }
    let conn = Connection::open(&db_path).map_err(|error| error.to_string())?;
    let session_columns = table_columns(&conn, "session")?;
    let message_columns = table_columns(&conn, "message")?;
    let part_columns = table_columns(&conn, "part")?;
    if session_columns.is_empty() || message_columns.is_empty() || part_columns.is_empty() {
        return Ok(Vec::new());
    }

    let session_id_col = pick_column(&session_columns, &["id", "session_id"])
        .ok_or_else(|| "OpenCode session table has no id column".to_string())?;
    let session_title_col = pick_column(&session_columns, &["title", "name"]);
    let project_col = pick_column(
        &session_columns,
        &["project", "project_path", "cwd", "path"],
    );
    let updated_col = pick_column(
        &session_columns,
        &["updated_at", "time_updated", "timeUpdated", "created_at"],
    );
    let query = format!(
        "SELECT {session_id_col}, {}, {}, {} FROM session ORDER BY rowid DESC LIMIT 500",
        session_title_col.unwrap_or("NULL"),
        project_col.unwrap_or("NULL"),
        updated_col.unwrap_or("NULL")
    );
    let mut stmt = conn.prepare(&query).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                required_cell_string(row, 0)?,
                optional_cell_string(row, 1)?,
                optional_cell_string(row, 2)?,
                optional_cell_string(row, 3)?,
            ))
        })
        .map_err(|error| error.to_string())?;

    let source_fingerprint = hash_file(&db_path).ok();
    let mut sessions = Vec::new();
    for row in rows {
        let (external_id, title, project_path, updated_at) =
            row.map_err(|error| error.to_string())?;
        let turns = read_opencode_turns(&conn, &message_columns, &part_columns, &external_id)?;
        if turns.is_empty() {
            continue;
        }
        sessions.push(NormalizedConversationSession {
            external_id,
            title,
            project_path,
            started_at: turns.first().and_then(|turn| turn.started_at.clone()),
            updated_at,
            source_locator: Some(db_path.to_string_lossy().to_string()),
            source_fingerprint: source_fingerprint.clone(),
            turns,
        });
    }
    Ok(sessions)
}

fn read_opencode_turns(
    conn: &Connection,
    message_columns: &[String],
    part_columns: &[String],
    session_id: &str,
) -> AppResult<Vec<NormalizedConversationTurn>> {
    let part_rows_by_message = read_opencode_part_rows(conn, part_columns, session_id)?;
    let msg_id_col = pick_column(message_columns, &["id", "message_id"])
        .ok_or_else(|| "OpenCode message table has no id column".to_string())?;
    let msg_session_col = pick_column(message_columns, &["session_id", "sessionID", "session"])
        .ok_or_else(|| "OpenCode message table has no session id column".to_string())?;
    let role_col = pick_column(message_columns, &["role", "author"]);
    let time_col = pick_column(
        message_columns,
        &["created_at", "time_created", "timeCreated", "time"],
    );
    let data_col = pick_column(message_columns, &["data", "json", "metadata"]);
    let msg_query = format!(
        "SELECT {msg_id_col}, {}, {}, {} FROM message WHERE {msg_session_col} = ?1 ORDER BY rowid ASC",
        role_col.unwrap_or("NULL"),
        time_col.unwrap_or("NULL"),
        data_col.unwrap_or("NULL")
    );
    let mut stmt = conn
        .prepare(&msg_query)
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![session_id], |row| {
            Ok((
                required_cell_string(row, 0)?,
                optional_cell_string(row, 1)?,
                optional_cell_string(row, 2)?,
                optional_cell_string(row, 3)?,
            ))
        })
        .map_err(|error| error.to_string())?;

    let mut turns = Vec::new();
    let mut current: Option<NormalizedConversationTurn> = None;
    for row in rows {
        let (message_id, role, timestamp, data_json) = row.map_err(|error| error.to_string())?;
        let data_value = data_json
            .as_deref()
            .and_then(|text| serde_json::from_str::<Value>(text).ok());
        let message_role = role
            .or_else(|| {
                data_value
                    .as_ref()
                    .and_then(|value| value_field_any_as_string(value, &["role", "author"]))
            })
            .unwrap_or_default();
        let timestamp = timestamp.or_else(|| {
            data_value
                .as_ref()
                .and_then(|value| value_field_any_as_string(value, &["time", "created_at"]))
        });
        let parts = part_rows_by_message
            .get(&message_id)
            .map(|rows| normalize_opencode_parts(rows, message_role.as_str()))
            .unwrap_or_default();
        if message_role == "user" {
            let user_text = parts
                .iter()
                .filter(|part| part.role == ConversationPartRole::User)
                .filter(|part| part.kind == ConversationPartKind::Text)
                .filter_map(|part| part.text.clone())
                .collect::<Vec<_>>()
                .join("\n\n");
            if user_text.trim().is_empty() {
                continue;
            }
            if let Some(turn) = current.take() {
                turns.push(turn);
            }
            current = Some(NormalizedConversationTurn {
                external_id: message_id,
                turn_index: turns.len() as i64,
                user_text,
                title: None,
                started_at: timestamp,
                ended_at: None,
                parts: parts
                    .into_iter()
                    .filter(|part| part.role != ConversationPartRole::User)
                    .collect(),
            });
        } else if let Some(turn) = current.as_mut() {
            turn.parts.extend(parts);
            turn.ended_at = timestamp;
        }
    }
    if let Some(turn) = current {
        turns.push(turn);
    }
    Ok(turns)
}

#[derive(Debug, Clone)]
struct OpenCodePartRow {
    kind: String,
    text: String,
    command: Option<String>,
    cwd: Option<String>,
    status: Option<String>,
    exit_code: Option<i32>,
    metadata_json: Option<String>,
    ignored: bool,
}

fn read_opencode_part_rows(
    conn: &Connection,
    columns: &[String],
    session_id: &str,
) -> AppResult<BTreeMap<String, Vec<OpenCodePartRow>>> {
    let session_col = pick_column(columns, &["session_id", "sessionID", "session"]);
    let message_col = pick_column(columns, &["message_id", "messageID", "message"])
        .ok_or_else(|| "OpenCode part table has no message id column".to_string())?;
    let type_col = pick_column(columns, &["type", "kind"]).unwrap_or("NULL");
    let text_col = pick_column(columns, &["text", "content", "output"]).unwrap_or("NULL");
    let data_col = pick_column(columns, &["data", "json", "metadata"]).unwrap_or("NULL");

    let query = if let Some(session_col) = session_col {
        format!(
            "SELECT {message_col}, {type_col}, {text_col}, {data_col} FROM part WHERE {session_col} = ?1 ORDER BY rowid ASC"
        )
    } else {
        format!(
            "SELECT {message_col}, {type_col}, {text_col}, {data_col} FROM part ORDER BY rowid ASC"
        )
    };
    let mut stmt = conn.prepare(&query).map_err(|error| error.to_string())?;
    let mut rows = if session_col.is_some() {
        stmt.query(params![session_id])
            .map_err(|error| error.to_string())?
    } else {
        stmt.query([]).map_err(|error| error.to_string())?
    };

    let mut by_message = BTreeMap::<String, Vec<OpenCodePartRow>>::new();
    while let Some(row) = rows.next().map_err(|error| error.to_string())? {
        let message_id = required_cell_string(row, 0).map_err(|error| error.to_string())?;
        let kind = optional_cell_string(row, 1).map_err(|error| error.to_string())?;
        let text = optional_cell_string(row, 2).map_err(|error| error.to_string())?;
        let data_json = optional_cell_string(row, 3).map_err(|error| error.to_string())?;
        let data_value = data_json
            .as_deref()
            .and_then(|text| serde_json::from_str::<Value>(text).ok());
        let kind = kind
            .or_else(|| {
                data_value
                    .as_ref()
                    .and_then(|value| value_field_any_as_string(value, &["type", "kind"]))
            })
            .unwrap_or_else(|| "text".to_string());
        let text = text
            .or_else(|| {
                data_value
                    .as_ref()
                    .and_then(|value| opencode_part_text(&kind, value))
            })
            .unwrap_or_default();
        let command = data_value.as_ref().and_then(command_from_value);
        let cwd = data_value.as_ref().and_then(cwd_from_value);
        let status = data_value.as_ref().and_then(status_from_value);
        let exit_code = data_value.as_ref().and_then(exit_code_from_value);
        let metadata_json = data_value.as_ref().map(compact_json);
        let ignored = data_value.as_ref().is_some_and(is_ignored_content_value);
        by_message
            .entry(message_id)
            .or_default()
            .push(OpenCodePartRow {
                kind,
                text,
                command,
                cwd,
                status,
                exit_code,
                metadata_json,
                ignored,
            });
    }

    Ok(by_message)
}

fn normalize_opencode_parts(
    rows: &[OpenCodePartRow],
    message_role: &str,
) -> Vec<NormalizedConversationPart> {
    let mut parts = Vec::new();
    for row in rows {
        if row.ignored {
            continue;
        }
        if matches!(
            row.kind.as_str(),
            "reasoning" | "step-start" | "step-finish" | "compaction" | "retry" | "snapshot"
        ) {
            continue;
        }
        let mapped_kind = match row.kind.as_str() {
            "tool" | "tool-call" | "tool-result" if row.command.is_some() => {
                ConversationPartKind::Command
            }
            "tool" | "tool-call" | "tool-result" => ConversationPartKind::Tool,
            "command" => ConversationPartKind::Command,
            "file" | "patch" => ConversationPartKind::FileChange,
            "subtask" | "agent" => ConversationPartKind::Subagent,
            _ => ConversationPartKind::Text,
        };
        let role = if message_role == "user" {
            ConversationPartRole::User
        } else if matches!(
            mapped_kind,
            ConversationPartKind::Command | ConversationPartKind::Tool
        ) {
            ConversationPartRole::Tool
        } else {
            ConversationPartRole::Assistant
        };
        if mapped_kind == ConversationPartKind::Text {
            parts.extend(split_markdown_text_parts(role, &row.text));
        } else {
            parts.push(NormalizedConversationPart {
                role,
                kind: mapped_kind,
                text: (!row.text.trim().is_empty()).then_some(row.text.clone()),
                language: None,
                command: row.command.clone(),
                cwd: row.cwd.clone(),
                status: row.status.clone(),
                exit_code: row.exit_code,
                metadata_json: row.metadata_json.clone(),
            });
        }
    }
    parts
}

#[derive(Clone, Copy)]
enum ParserFlavor {
    Codex,
    ClaudeCode,
}

fn parse_jsonl_conversation(
    text: &str,
    flavor: ParserFlavor,
) -> AppResult<Vec<NormalizedConversationTurn>> {
    let mut turns = Vec::new();
    let mut current: Option<NormalizedConversationTurn> = None;

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let timestamp = string_field_any(&value, &["timestamp", "created_at", "updated_at"]);
        if matches!(flavor, ParserFlavor::ClaudeCode)
            && value.get("isSidechain").and_then(Value::as_bool) == Some(true)
        {
            if let Some(turn) = current.as_mut() {
                let text = extract_text(event_payload(&value));
                if !text.trim().is_empty() {
                    turn.parts.push(NormalizedConversationPart {
                        role: ConversationPartRole::Assistant,
                        kind: ConversationPartKind::Subagent,
                        text: Some(text),
                        language: None,
                        command: None,
                        cwd: None,
                        status: None,
                        exit_code: None,
                        metadata_json: Some(compact_json(&value)),
                    });
                }
            }
            continue;
        }
        let payload = event_payload(&value);
        let role = role_of(payload).or_else(|| role_of(&value));
        let record_type = string_field_any(payload, &["type"])
            .or_else(|| string_field_any(&value, &["type"]))
            .unwrap_or_default();

        if let Some(user_text) =
            real_user_text_for_event(&value, payload, flavor, role.as_deref(), &record_type)
        {
            if let Some(turn) = current.take() {
                turns.push(turn);
            }
            current = Some(NormalizedConversationTurn {
                external_id: string_field_any(payload, &["id", "uuid"])
                    .or_else(|| string_field_any(&value, &["id", "uuid"]))
                    .unwrap_or_else(|| format!("turn-{}", turns.len())),
                turn_index: turns.len() as i64,
                user_text,
                title: None,
                started_at: timestamp,
                ended_at: None,
                parts: Vec::new(),
            });
            continue;
        }

        let Some(turn) = current.as_mut() else {
            continue;
        };
        if matches!(flavor, ParserFlavor::ClaudeCode) && is_user_tool_result_message(payload) {
            if let Some(part) = tool_part_from_event(payload) {
                turn.parts.push(part);
            }
            turn.ended_at = timestamp;
            continue;
        }
        if role.as_deref() == Some("assistant") {
            let text = extract_text(payload);
            if !text.trim().is_empty() {
                turn.parts.extend(split_markdown_text_parts(
                    ConversationPartRole::Assistant,
                    &text,
                ));
            }
            turn.ended_at = timestamp;
            continue;
        }
        if is_tool_record(&record_type, payload) {
            if let Some(part) = tool_part_from_event(payload) {
                turn.parts.push(part);
            }
            turn.ended_at = timestamp;
        }
    }
    if let Some(turn) = current {
        turns.push(turn);
    }
    Ok(turns)
}

fn tool_part_from_event(value: &Value) -> Option<NormalizedConversationPart> {
    let command = command_from_value(value);
    let text = tool_text_from_event(value);
    let record_type = string_field_any(value, &["type"]).unwrap_or_default();
    let tool_name = tool_name_from_value(value);
    let fallback_text = tool_name.as_ref().map(|name| {
        let event_type = if record_type.is_empty() {
            "tool".to_string()
        } else {
            record_type.clone()
        };
        format!("{event_type}: {name}")
    });
    let text = if text.trim().is_empty() {
        fallback_text.unwrap_or_default()
    } else {
        text
    };
    if command.is_none() && text.trim().is_empty() {
        return None;
    }
    let kind = if is_file_change_tool(&record_type, tool_name.as_deref(), value) {
        ConversationPartKind::FileChange
    } else if command.is_some() || record_type.contains("shell") {
        ConversationPartKind::Command
    } else {
        ConversationPartKind::Tool
    };
    Some(NormalizedConversationPart {
        role: ConversationPartRole::Tool,
        kind,
        text: (!text.trim().is_empty()).then_some(text),
        language: None,
        command,
        cwd: cwd_from_value(value),
        status: status_from_value(value),
        exit_code: exit_code_from_value(value),
        metadata_json: Some(compact_json(value)),
    })
}

fn is_tool_record(record_type: &str, payload: &Value) -> bool {
    record_type.contains("tool")
        || record_type.contains("function")
        || record_type.contains("exec")
        || record_type.contains("shell")
        || record_type == "patch"
        || payload.get("tool_use_id").is_some()
        || payload.get("toolUseID").is_some()
        || payload.get("call_id").is_some()
        || payload.get("callID").is_some()
        || payload.get("tool_name").is_some()
        || payload.get("toolName").is_some()
}

fn is_file_change_tool(record_type: &str, tool_name: Option<&str>, value: &Value) -> bool {
    record_type == "patch"
        || tool_name.is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name.contains("apply_patch") || name.contains("patch") || name.contains("edit")
        })
        || value
            .get("arguments")
            .and_then(parse_json_string_value)
            .as_ref()
            .is_some_and(value_contains_file_change_key)
        || value
            .get("arguments")
            .is_some_and(value_contains_file_change_key)
}

fn value_contains_file_change_key(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(value_contains_file_change_key),
        Value::Object(object) => {
            object.contains_key("patch")
                || object.contains_key("diff")
                || object.contains_key("file_change")
                || object.contains_key("fileChange")
                || object.values().any(value_contains_file_change_key)
        }
        _ => false,
    }
}

fn event_payload(value: &Value) -> &Value {
    value
        .get("item")
        .or_else(|| value.get("message"))
        .or_else(|| value.get("msg"))
        .or_else(|| value.get("payload"))
        .unwrap_or(value)
}

fn role_of(value: &Value) -> Option<String> {
    string_field_any(value, &["role"]).or_else(|| {
        value
            .get("message")
            .and_then(|message| string_field_any(message, &["role"]))
    })
}

fn real_user_text_for_event(
    value: &Value,
    payload: &Value,
    flavor: ParserFlavor,
    role: Option<&str>,
    record_type: &str,
) -> Option<String> {
    if is_synthetic_user_event(value) || is_synthetic_user_event(payload) {
        return None;
    }
    if matches!(flavor, ParserFlavor::ClaudeCode) && is_user_tool_result_message(payload) {
        return None;
    }

    let is_user_boundary = match flavor {
        ParserFlavor::Codex => {
            role == Some("user") && is_message_like_payload(payload, record_type)
        }
        ParserFlavor::ClaudeCode => {
            (record_type == "user" && value.get("content").is_some())
                || (role == Some("user") && is_message_like_payload(payload, record_type))
        }
    };
    if !is_user_boundary {
        return None;
    }

    let text = extract_user_message_text(payload);
    clean_real_user_text(&text)
}

fn is_message_like_payload(payload: &Value, record_type: &str) -> bool {
    record_type == "message" || payload.get("content").is_some() || payload.get("text").is_some()
}

fn is_synthetic_user_event(value: &Value) -> bool {
    if is_ignored_content_value(value) {
        return true;
    }
    let event_type = string_field_any(value, &["type"]).unwrap_or_default();
    matches!(
        event_type.as_str(),
        "attachment"
            | "auth_status"
            | "compaction"
            | "compaction_summary"
            | "context_compaction"
            | "custom_tool_call"
            | "custom_tool_call_output"
            | "event_msg"
            | "function_call"
            | "function_call_output"
            | "grouped_tool_use"
            | "hook_result"
            | "image_generation_call"
            | "local_shell_call"
            | "mcp_tool_call"
            | "mcp_tool_call_output"
            | "progress"
            | "rate_limit_event"
            | "reasoning"
            | "result"
            | "system"
            | "tombstone"
            | "tool_result"
            | "tool_search_call"
            | "tool_search_output"
            | "tool_use"
            | "tool_use_summary"
            | "turn_context"
            | "web_search_call"
    ) || value.get("tool_use_id").is_some()
        || value.get("toolUseID").is_some()
        || value.get("call_id").is_some()
        || value.get("callID").is_some()
        || value.get("tool_name").is_some()
        || value.get("toolName").is_some()
}

fn extract_user_message_text(value: &Value) -> String {
    let mut texts = Vec::new();
    if let Some(content) = value.get("content") {
        collect_user_content_text(content, &mut texts);
    } else if let Some(text) = value.get("text").and_then(Value::as_str) {
        texts.push(text.to_string());
    }
    texts.join("\n\n").trim().to_string()
}

fn collect_user_content_text(value: &Value, texts: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            if !text.trim().is_empty() {
                texts.push(text.clone());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_user_content_text(item, texts);
            }
        }
        Value::Object(object) => {
            let item_type = object.get("type").and_then(Value::as_str).unwrap_or("");
            if object_flag_true(object, &["synthetic", "ignored", "isSynthetic", "isMeta"]) {
                return;
            }
            if matches!(
                item_type,
                "attachment"
                    | "file"
                    | "hook_result"
                    | "image"
                    | "input_image"
                    | "reasoning"
                    | "thinking"
                    | "tool_result"
                    | "tool_use"
            ) {
                return;
            }
            if matches!(item_type, "" | "text" | "input_text" | "user" | "message") {
                if let Some(text) = object.get("text").and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        texts.push(text.to_string());
                    }
                    return;
                }
            }
            if let Some(input_text) = object.get("input_text").and_then(Value::as_str) {
                if !input_text.trim().is_empty() {
                    texts.push(input_text.to_string());
                }
                return;
            }
            if let Some(content) = object.get("content") {
                collect_user_content_text(content, texts);
            }
        }
        _ => {}
    }
}

fn is_user_tool_result_message(value: &Value) -> bool {
    value
        .get("content")
        .is_some_and(|content| value_contains_type(content, "tool_result"))
}

fn value_contains_type(value: &Value, expected_type: &str) -> bool {
    match value {
        Value::Array(items) => items
            .iter()
            .any(|item| value_contains_type(item, expected_type)),
        Value::Object(object) => {
            object.get("type").and_then(Value::as_str) == Some(expected_type)
                || object
                    .get("content")
                    .is_some_and(|content| value_contains_type(content, expected_type))
        }
        _ => false,
    }
}

fn clean_real_user_text(text: &str) -> Option<String> {
    clean_real_user_fragment(text)
}

fn clean_real_user_fragment(fragment: &str) -> Option<String> {
    let mut remaining = fragment.trim();
    loop {
        if remaining.starts_with("# AGENTS.md instructions for ") {
            let end = remaining.find("</INSTRUCTIONS>")?;
            remaining = remaining[end + "</INSTRUCTIONS>".len()..].trim_start();
            continue;
        }
        if remaining.starts_with("<environment_context") {
            let end = remaining.find("</environment_context>")?;
            remaining = remaining[end + "</environment_context>".len()..].trim_start();
            continue;
        }
        if remaining.starts_with("<codex_internal_context") {
            let end = remaining.find("</codex_internal_context>")?;
            remaining = remaining[end + "</codex_internal_context>".len()..].trim_start();
            continue;
        }
        if let Some(next) = strip_wrapped_prefix(remaining, "permissions instructions") {
            remaining = next;
            continue;
        }
        if let Some(next) = strip_wrapped_prefix(remaining, "app-context") {
            remaining = next;
            continue;
        }
        if let Some(next) = strip_wrapped_prefix(remaining, "personality_spec") {
            remaining = next;
            continue;
        }
        if let Some(next) = strip_wrapped_prefix(remaining, "skills_instructions") {
            remaining = next;
            continue;
        }
        if let Some(next) = strip_wrapped_prefix(remaining, "plugins_instructions") {
            remaining = next;
            continue;
        }
        break;
    }
    (!remaining.trim().is_empty()).then(|| remaining.trim().to_string())
}

fn strip_wrapped_prefix<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    if !text.starts_with(&format!("<{tag}")) {
        return None;
    }
    let closing = format!("</{tag}>");
    let end = text.find(&closing)?;
    Some(text[end + closing.len()..].trim_start())
}

fn opencode_part_text(kind: &str, value: &Value) -> Option<String> {
    if is_ignored_content_value(value) {
        return None;
    }
    let text = match kind {
        "text" => string_field_any(value, &["text", "content"]),
        "tool" | "tool-call" | "tool-result" => {
            let text = tool_text_from_event(value);
            (!text.trim().is_empty()).then_some(text)
        }
        "patch" => {
            string_field_any(value, &["text", "patch", "diff", "summary", "path"]).or_else(|| {
                let text = tool_text_from_event(value);
                (!text.trim().is_empty()).then_some(text)
            })
        }
        "file" => string_field_any(value, &["path", "filename", "name", "text", "summary"]),
        "subtask" => string_field_any(value, &["description", "prompt", "command", "agent"]),
        "agent" => string_field_any(value, &["agent", "summary", "text", "description"]),
        _ => {
            let text = extract_text(value);
            (!text.trim().is_empty()).then_some(text)
        }
    };
    text.filter(|text| !text.trim().is_empty())
}

fn tool_text_from_event(value: &Value) -> String {
    let mut texts = Vec::new();
    for key in [
        "output",
        "tool_output",
        "toolOutput",
        "result",
        "content",
        "summary",
        "message",
        "error",
    ] {
        if let Some(child) = value.get(key) {
            collect_tool_text(child, &mut texts);
        }
    }
    if let Some(state) = value.get("state") {
        for key in ["title", "output", "error", "message"] {
            if let Some(child) = state.get(key) {
                collect_tool_text(child, &mut texts);
            }
        }
    }
    if texts.is_empty() {
        if let Some(arguments) = value.get("arguments") {
            if let Some(parsed) = parse_json_string_value(arguments) {
                collect_tool_text(&parsed, &mut texts);
            } else {
                collect_tool_text(arguments, &mut texts);
            }
        }
    }
    texts.join("\n").trim().to_string()
}

fn collect_tool_text(value: &Value, texts: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            if !text.trim().is_empty() {
                texts.push(text.clone());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_tool_text(item, texts);
            }
        }
        Value::Object(object) => {
            if object_flag_true(
                object,
                &[
                    "ignored",
                    "synthetic",
                    "isSynthetic",
                    "isMeta",
                    "isVisibleInTranscriptOnly",
                ],
            ) {
                return;
            }
            let item_type = object.get("type").and_then(Value::as_str).unwrap_or("");
            if matches!(
                item_type,
                "thinking" | "reasoning" | "step-start" | "step-finish" | "snapshot" | "retry"
            ) {
                return;
            }
            for key in [
                "text", "content", "output", "result", "summary", "stdout", "stderr", "preview",
                "message", "title", "patch", "diff",
            ] {
                if let Some(child) = object.get(key) {
                    collect_tool_text(child, texts);
                }
            }
        }
        _ => {}
    }
}

fn command_from_value(value: &Value) -> Option<String> {
    command_from_value_inner(value, 0)
}

fn command_from_value_inner(value: &Value, depth: usize) -> Option<String> {
    if depth > 8 {
        return None;
    }
    let object = value.as_object()?;
    if let Some(command) = string_field_any(value, &["command", "cmd", "shell_command"]) {
        return Some(command).filter(|value| !value.trim().is_empty());
    }
    for key in [
        "action",
        "input",
        "tool_input",
        "toolInput",
        "state",
        "request",
        "params",
        "parameters",
    ] {
        if let Some(child) = object.get(key) {
            if let Some(command) = command_from_value_inner(child, depth + 1) {
                return Some(command);
            }
        }
    }
    for key in ["arguments", "args"] {
        if let Some(child) = object.get(key) {
            if let Some(command) = command_from_arguments(child, depth + 1) {
                return Some(command);
            }
        }
    }
    None
}

fn command_from_arguments(value: &Value, depth: usize) -> Option<String> {
    match value {
        Value::Object(_) => command_from_value_inner(value, depth),
        Value::Array(items) => {
            let args = items.iter().filter_map(Value::as_str).collect::<Vec<_>>();
            (!args.is_empty())
                .then(|| args.join(" "))
                .filter(|value| !value.trim().is_empty())
        }
        Value::String(_) => parse_json_string_value(value)
            .as_ref()
            .and_then(|parsed| command_from_value_inner(parsed, depth + 1)),
        _ => None,
    }
}

fn parse_json_string_value(value: &Value) -> Option<Value> {
    let text = value.as_str()?.trim();
    if !text.starts_with('{') && !text.starts_with('[') {
        return None;
    }
    serde_json::from_str::<Value>(text).ok()
}

fn extract_text(value: &Value) -> String {
    let mut texts = Vec::new();
    collect_text(value, &mut texts);
    texts.join("\n").trim().to_string()
}

fn collect_text(value: &Value, texts: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            if !text.trim().is_empty() {
                texts.push(text.clone());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_text(item, texts);
            }
        }
        Value::Object(object) => {
            let item_type = object.get("type").and_then(Value::as_str).unwrap_or("");
            if matches!(
                item_type,
                "thinking"
                    | "reasoning"
                    | "step-start"
                    | "step-finish"
                    | "compaction"
                    | "retry"
                    | "snapshot"
            ) {
                return;
            }
            for key in ["text", "content", "output", "result", "summary"] {
                if let Some(child) = object.get(key) {
                    collect_text(child, texts);
                }
            }
        }
        _ => {}
    }
}

fn tool_name_from_value(value: &Value) -> Option<String> {
    string_field_any(value, &["tool_name", "toolName", "tool", "name"])
}

fn cwd_from_value(value: &Value) -> Option<String> {
    nested_string_field_any(
        value,
        &["cwd", "workdir", "working_directory", "workingDirectory"],
        0,
    )
}

fn status_from_value(value: &Value) -> Option<String> {
    nested_string_field_any(value, &["status", "state"], 0)
}

fn exit_code_from_value(value: &Value) -> Option<i32> {
    nested_i64_field_any(value, &["exit_code", "exitCode", "code"], 0).map(|value| value as i32)
}

fn nested_string_field_any(value: &Value, names: &[&str], depth: usize) -> Option<String> {
    if depth > 6 {
        return None;
    }
    if let Some(value) = value_field_any_as_string(value, names) {
        return Some(value).filter(|value| !value.trim().is_empty());
    }
    let object = value.as_object()?;
    for key in ["arguments", "args"] {
        if let Some(child) = object.get(key) {
            if let Some(parsed) = parse_json_string_value(child) {
                if let Some(value) = nested_string_field_any(&parsed, names, depth + 1) {
                    return Some(value);
                }
            } else if let Some(value) = nested_string_field_any(child, names, depth + 1) {
                return Some(value);
            }
        }
    }
    for key in ["state", "input", "tool_input", "toolInput", "action"] {
        if let Some(child) = object.get(key) {
            if let Some(value) = nested_string_field_any(child, names, depth + 1) {
                return Some(value);
            }
        }
    }
    None
}

fn nested_i64_field_any(value: &Value, names: &[&str], depth: usize) -> Option<i64> {
    if depth > 6 {
        return None;
    }
    for name in names {
        if let Some(child) = value.get(*name) {
            if let Some(number) = child.as_i64() {
                return Some(number);
            }
            if let Some(text) = child.as_str().and_then(|text| text.parse::<i64>().ok()) {
                return Some(text);
            }
        }
    }
    let object = value.as_object()?;
    for key in ["arguments", "args"] {
        if let Some(child) = object.get(key) {
            if let Some(parsed) = parse_json_string_value(child) {
                if let Some(value) = nested_i64_field_any(&parsed, names, depth + 1) {
                    return Some(value);
                }
            } else if let Some(value) = nested_i64_field_any(child, names, depth + 1) {
                return Some(value);
            }
        }
    }
    for key in ["state", "input", "tool_input", "toolInput", "action"] {
        if let Some(child) = object.get(key) {
            if let Some(value) = nested_i64_field_any(child, names, depth + 1) {
                return Some(value);
            }
        }
    }
    None
}

fn is_ignored_content_value(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object_flag_true(
            object,
            &[
                "ignored",
                "synthetic",
                "isSynthetic",
                "isMeta",
                "isVisibleInTranscriptOnly",
            ],
        )
    })
}

fn object_flag_true(object: &serde_json::Map<String, Value>, names: &[&str]) -> bool {
    names
        .iter()
        .any(|name| object.get(*name).and_then(Value::as_bool) == Some(true))
}

fn string_field_any(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_str).map(str::to_string))
}

fn value_field_any_as_string(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(value_as_string))
}

fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn infer_project_path_from_turns(turns: &[NormalizedConversationTurn]) -> Option<String> {
    turns.iter().find_map(|turn| {
        turn.parts.iter().find_map(|part| {
            part.cwd
                .as_deref()
                .filter(|cwd| !cwd.trim().is_empty())
                .map(str::to_string)
        })
    })
}

fn table_columns(conn: &Connection, table: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn pick_column<'a>(columns: &'a [String], candidates: &[&str]) -> Option<&'a str> {
    candidates.iter().find_map(|candidate| {
        columns
            .iter()
            .find(|column| column.eq_ignore_ascii_case(candidate))
            .map(String::as_str)
    })
}

fn required_cell_string(row: &Row<'_>, index: usize) -> rusqlite::Result<String> {
    Ok(optional_cell_string(row, index)?.unwrap_or_default())
}

fn optional_cell_string(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<String>> {
    match row.get_ref(index)? {
        ValueRef::Null => Ok(None),
        ValueRef::Integer(value) => Ok(Some(value.to_string())),
        ValueRef::Real(value) => {
            if value.fract() == 0.0 {
                Ok(Some(format!("{value:.0}")))
            } else {
                Ok(Some(value.to_string()))
            }
        }
        ValueRef::Text(value) => Ok(Some(String::from_utf8_lossy(value).to_string())),
        ValueRef::Blob(value) => Ok(Some(hash_bytes(value))),
    }
}

fn collect_files_with_extension(
    root: &Path,
    extension: &str,
    limit: usize,
) -> AppResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files_with_extension_inner(root, extension, limit, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files_with_extension_inner(
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

fn resolve_command_path(manifest_dir: &Path, command: &str) -> PathBuf {
    let path = PathBuf::from(command);
    if path.is_absolute() {
        path
    } else {
        manifest_dir.join(path)
    }
}

fn read_capped<R: Read>(mut reader: R, cap: usize) -> AppResult<Vec<u8>> {
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

fn hash_file(path: &Path) -> AppResult<String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    Ok(hash_bytes(&bytes))
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

#[allow(dead_code)]
fn _metadata_map(value: &Value) -> BTreeMap<String, Value> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::{ConversationPartKind, ConversationSourceKind};
    use rusqlite::params;
    use std::io::Cursor;

    struct TempFixture {
        path: PathBuf,
    }

    impl TempFixture {
        fn new(prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
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

    #[test]
    fn codex_adapter_reads_rollout_jsonl_from_state_database() {
        let fixture = TempFixture::new("assetiweave-codex-fixture");
        let rollout_path = fixture.path().join("rollout.jsonl");
        fs::write(
            &rollout_path,
            [
                r#"{"timestamp":"2026-01-01T00:00:00Z","type":"response_item","payload":{"id":"u1","type":"message","role":"user","content":[{"type":"input_text","text":"How should sync work?"}]}}"#,
                r#"{"timestamp":"2026-01-01T00:00:01Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Use normalized turns.\n```sh\ncargo test\n```"}]}}"#,
                r#"{"timestamp":"2026-01-01T00:00:02Z","type":"response_item","payload":{"type":"function_call","command":"cargo test","cwd":"/tmp/project","status":"completed","exit_code":0}}"#,
                r#"{"timestamp":"2026-01-01T00:00:03Z","type":"response_item","payload":{"id":"u2","type":"message","role":"user","content":"Export it"}}"#,
                r#"{"timestamp":"2026-01-01T00:00:04Z","type":"response_item","payload":{"type":"message","role":"assistant","content":"Markdown is ready."}}"#,
            ]
            .join("\n"),
        )
        .unwrap();

        let db_path = fixture.path().join("state_5.sqlite");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, title TEXT, updated_at INTEGER);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO threads (id, rollout_path, title, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                "codex-session-1",
                rollout_path.to_string_lossy().as_ref(),
                "Codex Fixture",
                1_767_225_604i64
            ],
        )
        .unwrap();
        drop(conn);

        let sessions = read_source_sessions(&source_fixture(
            "codex",
            ConversationSourceKind::File,
            db_path.to_string_lossy().as_ref(),
        ))
        .unwrap();

        assert_eq!(sessions.len(), 1);
        let session = &sessions[0];
        assert_eq!(session.external_id, "codex-session-1");
        assert_eq!(session.title.as_deref(), Some("Codex Fixture"));
        assert_eq!(session.updated_at.as_deref(), Some("1767225604"));
        assert_eq!(session.project_path.as_deref(), Some("/tmp/project"));
        assert_eq!(session.turns.len(), 2);
        assert_eq!(session.turns[0].user_text, "How should sync work?");
        assert!(session.turns[0]
            .parts
            .iter()
            .any(|part| part.kind == ConversationPartKind::CodeBlock
                && part.text.as_deref() == Some("cargo test")));
        assert!(session.turns[0]
            .parts
            .iter()
            .any(|part| part.kind == ConversationPartKind::Command
                && part.command.as_deref() == Some("cargo test")));
    }

    #[test]
    fn codex_adapter_ignores_context_only_user_messages() {
        let fixture = TempFixture::new("assetiweave-codex-context-fixture");
        let rollout_path = fixture.path().join("rollout.jsonl");
        fs::write(
            &rollout_path,
            [
                r##"{"timestamp":"2026-01-01T00:00:00Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for /tmp/project\n\n<INSTRUCTIONS>\nUse repo rules.\n</INSTRUCTIONS>"},{"type":"input_text","text":"<environment_context>\n  <cwd>/tmp/project</cwd>\n</environment_context>"}]}}"##,
                r#"{"timestamp":"2026-01-01T00:00:01Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Please implement the conversation importer."}]}}"#,
                r#"{"timestamp":"2026-01-01T00:00:02Z","type":"response_item","payload":{"type":"message","role":"assistant","content":"Importer started."}}"#,
                r#"{"timestamp":"2026-01-01T00:00:03Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<codex_internal_context source=\"goal\">\nContinue working toward the active thread goal.\n</codex_internal_context>"}]}}"#,
                r#"{"timestamp":"2026-01-01T00:00:04Z","type":"response_item","payload":{"type":"message","role":"assistant","content":"Importer finished."}}"#,
            ]
            .join("\n"),
        )
        .unwrap();

        let db_path = fixture.path().join("state_5.sqlite");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, title TEXT, updated_at INTEGER);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO threads (id, rollout_path, title, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                "codex-context-session",
                rollout_path.to_string_lossy().as_ref(),
                "Codex Context Fixture",
                1_767_225_604i64
            ],
        )
        .unwrap();
        drop(conn);

        let sessions = read_source_sessions(&source_fixture(
            "codex",
            ConversationSourceKind::File,
            db_path.to_string_lossy().as_ref(),
        ))
        .unwrap();

        assert_eq!(sessions.len(), 1);
        let turns = &sessions[0].turns;
        assert_eq!(turns.len(), 1);
        assert_eq!(
            turns[0].user_text,
            "Please implement the conversation importer."
        );
        assert!(turns[0]
            .parts
            .iter()
            .any(|part| part.text.as_deref() == Some("Importer started.")));
        assert!(turns[0]
            .parts
            .iter()
            .any(|part| part.text.as_deref() == Some("Importer finished.")));
    }

    #[test]
    fn codex_adapter_classifies_commands_file_changes_code_blocks_and_noise() {
        let fixture = TempFixture::new("assetiweave-codex-rich-fixture");
        let rollout_path = fixture.path().join("rollout.jsonl");
        fs::write(
            &rollout_path,
            [
                json!({
                    "timestamp": "2026-01-01T00:00:00Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "user",
                        "content": [{"type": "input_text", "text": "<environment_context>\n<cwd>/tmp/project</cwd>\n</environment_context>"}]
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:01Z",
                    "type": "response_item",
                    "payload": {
                        "id": "u1",
                        "type": "message",
                        "role": "user",
                        "content": [{
                            "type": "input_text",
                            "text": "Run the importer tests:\n```sh\npnpm test\n```"
                        }]
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:02Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": "Use a parser.\n```rs\nlet parsed = true;\n```"}]
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:03Z",
                    "type": "response_item",
                    "payload": {
                        "type": "local_shell_call",
                        "action": {"command": "pnpm test"},
                        "cwd": "/tmp/project",
                        "status": "completed",
                        "exit_code": 0
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:04Z",
                    "type": "response_item",
                    "payload": {
                        "type": "function_call",
                        "name": "exec_command",
                        "arguments": "{\"cmd\":\"cargo test --workspace\",\"workdir\":\"/tmp/project\"}"
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:05Z",
                    "type": "response_item",
                    "payload": {
                        "type": "function_call",
                        "name": "apply_patch",
                        "arguments": "{\"patch\":\"*** Begin Patch\\n*** Update File: src/main.rs\\n@@\\n+let ok = true;\\n*** End Patch\"}"
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:06Z",
                    "type": "response_item",
                    "payload": {"type": "function_call_output", "output": "patch applied"}
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:07Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "user",
                        "content": [{"type": "input_text", "text": "<codex_internal_context source=\"goal\">\nContinue active goal.\n</codex_internal_context>"}]
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:08Z",
                    "type": "response_item",
                    "payload": {
                        "id": "u2",
                        "type": "message",
                        "role": "user",
                        "content": "Export the result"
                    }
                })
                .to_string(),
            ]
            .join("\n"),
        )
        .unwrap();

        let db_path = fixture.path().join("state_5.sqlite");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, title TEXT, updated_at INTEGER);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO threads (id, rollout_path, title, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                "codex-rich-session",
                rollout_path.to_string_lossy().as_ref(),
                "Codex Rich Fixture",
                1_767_225_604i64
            ],
        )
        .unwrap();
        drop(conn);

        let sessions = read_source_sessions(&source_fixture(
            "codex",
            ConversationSourceKind::File,
            db_path.to_string_lossy().as_ref(),
        ))
        .unwrap();

        let turns = &sessions[0].turns;
        assert_eq!(turns.len(), 2);
        assert!(turns[0].user_text.starts_with("Run the importer tests"));
        assert_eq!(turns[1].user_text, "Export the result");
        assert!(turns[0].parts.iter().any(|part| {
            part.kind == ConversationPartKind::CodeBlock
                && part.language.as_deref() == Some("rs")
                && part.text.as_deref() == Some("let parsed = true;")
        }));
        assert!(turns[0].parts.iter().any(|part| {
            part.kind == ConversationPartKind::Command
                && part.command.as_deref() == Some("pnpm test")
                && part.cwd.as_deref() == Some("/tmp/project")
        }));
        assert!(turns[0].parts.iter().any(|part| {
            part.kind == ConversationPartKind::Command
                && part.command.as_deref() == Some("cargo test --workspace")
                && part.cwd.as_deref() == Some("/tmp/project")
        }));
        assert!(turns[0]
            .parts
            .iter()
            .any(|part| part.kind == ConversationPartKind::FileChange));
        assert!(turns[0].parts.iter().any(|part| {
            part.kind == ConversationPartKind::Tool
                && part
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("patch applied"))
        }));
    }

    #[test]
    fn claude_code_adapter_uses_real_user_boundaries_and_keeps_sidechain_output() {
        let fixture = TempFixture::new("assetiweave-claude-fixture");
        let project_dir = fixture.path().join("Users-util6-project");
        fs::create_dir_all(&project_dir).unwrap();
        let session_path = project_dir.join("claude-session.jsonl");
        fs::write(
            &session_path,
            [
                r#"{"timestamp":"2026-01-01T00:00:00Z","message":{"id":"u1","role":"user","content":"Plan the import"}}"#,
                r#"{"timestamp":"2026-01-01T00:00:01Z","message":{"role":"assistant","content":"Import as turns."}}"#,
                r#"{"timestamp":"2026-01-01T00:00:02Z","isSidechain":true,"message":{"role":"assistant","content":"Subagent checked fixture shape."}}"#,
                r#"{"timestamp":"2026-01-01T00:00:03Z","message":{"id":"u2","role":"user","content":"Now export"}}"#,
                r#"{"timestamp":"2026-01-01T00:00:04Z","message":{"role":"assistant","content":"Exported."}}"#,
            ]
            .join("\n"),
        )
        .unwrap();

        let sessions = read_source_sessions(&source_fixture(
            "claude-code",
            ConversationSourceKind::Directory,
            fixture.path().to_string_lossy().as_ref(),
        ))
        .unwrap();

        assert_eq!(sessions.len(), 1);
        let session = &sessions[0];
        assert_eq!(session.external_id, "claude-session");
        assert_eq!(session.title.as_deref(), Some("Users/util6/project"));
        assert_eq!(session.turns.len(), 2);
        assert_eq!(session.turns[0].user_text, "Plan the import");
        assert!(session.turns[0].parts.iter().any(|part| part.kind
            == ConversationPartKind::Subagent
            && part
                .text
                .as_deref()
                .is_some_and(|text| text.contains("Subagent checked"))));
    }

    #[test]
    fn claude_code_adapter_uses_top_level_user_events_only_as_boundaries() {
        let fixture = TempFixture::new("assetiweave-claude-top-level-fixture");
        let session_path = fixture.path().join("claude-top-level.jsonl");
        fs::write(
            &session_path,
            [
                r#"{"timestamp":"2026-01-01T00:00:00Z","type":"user","content":"Plan the import"}"#,
                r#"{"timestamp":"2026-01-01T00:00:01Z","type":"tool_use","tool_name":"read","tool_input":{"filePath":"/tmp/project/input.jsonl"}}"#,
                r#"{"timestamp":"2026-01-01T00:00:02Z","type":"tool_result","tool_name":"read","tool_output":{"preview":"fixture rows","truncated":false}}"#,
                r#"{"timestamp":"2026-01-01T00:00:03Z","type":"user","content":"Now export it"}"#,
            ]
            .join("\n"),
        )
        .unwrap();

        let sessions = read_source_sessions(&source_fixture(
            "claude-code",
            ConversationSourceKind::File,
            session_path.to_string_lossy().as_ref(),
        ))
        .unwrap();

        assert_eq!(sessions.len(), 1);
        let turns = &sessions[0].turns;
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].user_text, "Plan the import");
        assert_eq!(turns[1].user_text, "Now export it");
        assert!(turns[0].parts.iter().any(|part| {
            part.role == ConversationPartRole::Tool
                && part
                    .metadata_json
                    .as_deref()
                    .is_some_and(|metadata| metadata.contains("tool_use"))
        }));
    }

    #[test]
    fn claude_code_adapter_filters_tool_result_user_wrappers_and_classifies_tools() {
        let fixture = TempFixture::new("assetiweave-claude-tool-wrapper-fixture");
        let session_path = fixture.path().join("claude-tool-wrapper.jsonl");
        fs::write(
            &session_path,
            [
                json!({
                    "timestamp": "2026-01-01T00:00:00Z",
                    "type": "user",
                    "content": "Inspect the project"
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:01Z",
                    "message": {
                        "role": "assistant",
                        "content": [{"type": "text", "text": "I will inspect it.\n```ts\nconst ok = true;\n```"}]
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:02Z",
                    "type": "user",
                    "message": {
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": "toolu_1",
                            "content": "tool result wrapper output"
                        }]
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:03Z",
                    "type": "tool_use",
                    "tool_name": "Bash",
                    "tool_input": {"command": "ls -la"}
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:04Z",
                    "type": "tool_result",
                    "tool_name": "Bash",
                    "tool_output": {"stdout": "listed files", "stderr": ""}
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:05Z",
                    "type": "result",
                    "subtype": "success",
                    "result": "bookkeeping success"
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:06Z",
                    "type": "user",
                    "content": [{"type": "text", "text": "Now summarize"}]
                })
                .to_string(),
            ]
            .join("\n"),
        )
        .unwrap();

        let sessions = read_source_sessions(&source_fixture(
            "claude-code",
            ConversationSourceKind::File,
            session_path.to_string_lossy().as_ref(),
        ))
        .unwrap();

        let turns = &sessions[0].turns;
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].user_text, "Inspect the project");
        assert_eq!(turns[1].user_text, "Now summarize");
        assert!(turns[0].parts.iter().any(|part| {
            part.kind == ConversationPartKind::CodeBlock
                && part.language.as_deref() == Some("ts")
                && part.text.as_deref() == Some("const ok = true;")
        }));
        assert!(turns[0].parts.iter().any(|part| {
            part.kind == ConversationPartKind::Command && part.command.as_deref() == Some("ls -la")
        }));
        assert!(turns[0].parts.iter().any(|part| {
            part.kind == ConversationPartKind::Tool
                && part
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("listed files"))
        }));
        assert!(turns[0].parts.iter().any(|part| {
            part.kind == ConversationPartKind::Tool
                && part
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("tool result wrapper output"))
        }));
        assert!(turns[0].parts.iter().all(|part| {
            !part
                .text
                .as_deref()
                .is_some_and(|text| text.contains("bookkeeping success"))
        }));
    }

    #[test]
    fn opencode_adapter_reads_sqlite_messages_and_filters_reasoning_parts() {
        let fixture = TempFixture::new("assetiweave-opencode-fixture");
        let db_path = fixture.path().join("opencode.sqlite");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE session (
                id TEXT PRIMARY KEY,
                title TEXT,
                project_path TEXT,
                time_updated INTEGER
            );
            CREATE TABLE message (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                time_created INTEGER,
                data TEXT
            );
            CREATE TABLE part (
                session_id TEXT,
                message_id TEXT,
                data TEXT
            );
            "#,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, title, project_path, time_updated) VALUES (?1, ?2, ?3, ?4)",
            params![
                "open-session-1",
                "OpenCode Fixture",
                "/tmp/opencode",
                1_767_225_604i64
            ],
        )
        .unwrap();
        for (id, role, timestamp) in [
            ("m1", "user", 1_767_225_600i64),
            ("m2", "assistant", 1_767_225_601i64),
            ("m3", "user", 1_767_225_602i64),
            ("m4", "assistant", 1_767_225_603i64),
        ] {
            let data = json!({ "role": role, "time": timestamp }).to_string();
            conn.execute(
                "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
                params![id, "open-session-1", timestamp, data],
            )
            .unwrap();
        }
        for (message_id, kind, text) in [
            ("m1", "text", "How should OpenCode import?"),
            ("m2", "reasoning", "hidden chain of thought"),
            (
                "m2",
                "text",
                "Use database rows.\n```ts\nconst ok = true;\n```",
            ),
            ("m2", "command", "pnpm test"),
            ("m3", "text", "Continue"),
            ("m4", "text", "Done"),
        ] {
            let data = json!({ "type": kind, "text": text }).to_string();
            conn.execute(
                "INSERT INTO part (session_id, message_id, data) VALUES (?1, ?2, ?3)",
                params!["open-session-1", message_id, data],
            )
            .unwrap();
        }
        drop(conn);

        let sessions = read_source_sessions(&source_fixture(
            "opencode",
            ConversationSourceKind::Sqlite,
            db_path.to_string_lossy().as_ref(),
        ))
        .unwrap();

        assert_eq!(sessions.len(), 1);
        let session = &sessions[0];
        assert_eq!(session.updated_at.as_deref(), Some("1767225604"));
        assert_eq!(session.turns.len(), 2);
        assert_eq!(session.turns[0].user_text, "How should OpenCode import?");
        assert!(session.turns[0]
            .parts
            .iter()
            .all(|part| part.text.as_deref() != Some("hidden chain of thought")));
        assert!(session.turns[0]
            .parts
            .iter()
            .any(|part| part.kind == ConversationPartKind::CodeBlock
                && part.language.as_deref() == Some("ts")));
        assert!(session.turns[0]
            .parts
            .iter()
            .any(|part| part.kind == ConversationPartKind::Command
                && part.text.as_deref() == Some("pnpm test")));
    }

    #[test]
    fn opencode_adapter_ignores_user_role_bookkeeping_parts_as_turn_boundaries() {
        let fixture = TempFixture::new("assetiweave-opencode-user-bookkeeping-fixture");
        let db_path = fixture.path().join("opencode.sqlite");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE session (
                id TEXT PRIMARY KEY,
                title TEXT,
                project_path TEXT,
                time_updated INTEGER
            );
            CREATE TABLE message (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                time_created INTEGER,
                data TEXT
            );
            CREATE TABLE part (
                session_id TEXT,
                message_id TEXT,
                data TEXT
            );
            "#,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, title, project_path, time_updated) VALUES (?1, ?2, ?3, ?4)",
            params![
                "open-session-bookkeeping",
                "OpenCode Bookkeeping Fixture",
                "/tmp/opencode",
                1_767_225_604i64
            ],
        )
        .unwrap();
        for (id, role, timestamp) in [
            ("m1", "user", 1_767_225_600i64),
            ("m2", "assistant", 1_767_225_601i64),
            ("m3", "user", 1_767_225_602i64),
            ("m4", "user", 1_767_225_603i64),
            ("m5", "user", 1_767_225_604i64),
        ] {
            let data = json!({ "role": role, "time": timestamp }).to_string();
            conn.execute(
                "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
                params![id, "open-session-bookkeeping", timestamp, data],
            )
            .unwrap();
        }
        for (message_id, kind, text) in [
            ("m1", "text", "First real question"),
            ("m2", "text", "First answer"),
            ("m3", "compaction", "Conversation compaction summary"),
            ("m4", "file", "/tmp/opencode/attachment.txt"),
            ("m5", "text", "Second real question"),
        ] {
            let data = json!({ "type": kind, "text": text }).to_string();
            conn.execute(
                "INSERT INTO part (session_id, message_id, data) VALUES (?1, ?2, ?3)",
                params!["open-session-bookkeeping", message_id, data],
            )
            .unwrap();
        }
        drop(conn);

        let sessions = read_source_sessions(&source_fixture(
            "opencode",
            ConversationSourceKind::Sqlite,
            db_path.to_string_lossy().as_ref(),
        ))
        .unwrap();

        assert_eq!(sessions.len(), 1);
        let turns = &sessions[0].turns;
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].user_text, "First real question");
        assert_eq!(turns[1].user_text, "Second real question");
    }

    #[test]
    fn opencode_adapter_filters_synthetic_user_text_and_maps_rich_part_types() {
        let fixture = TempFixture::new("assetiweave-opencode-rich-part-fixture");
        let db_path = fixture.path().join("opencode.sqlite");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE session (
                id TEXT PRIMARY KEY,
                title TEXT,
                project_path TEXT,
                time_updated INTEGER
            );
            CREATE TABLE message (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                time_created INTEGER,
                data TEXT
            );
            CREATE TABLE part (
                session_id TEXT,
                message_id TEXT,
                data TEXT
            );
            "#,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, title, project_path, time_updated) VALUES (?1, ?2, ?3, ?4)",
            params![
                "open-session-rich",
                "OpenCode Rich Fixture",
                "/tmp/opencode",
                1_767_225_608i64
            ],
        )
        .unwrap();
        for (id, role, timestamp) in [
            ("m1", "user", 1_767_225_600i64),
            ("m2", "assistant", 1_767_225_601i64),
            ("m3", "user", 1_767_225_602i64),
            ("m4", "user", 1_767_225_603i64),
            ("m5", "user", 1_767_225_604i64),
            ("m6", "user", 1_767_225_605i64),
            ("m7", "user", 1_767_225_606i64),
        ] {
            let data = json!({ "role": role, "time": timestamp }).to_string();
            conn.execute(
                "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
                params![id, "open-session-rich", timestamp, data],
            )
            .unwrap();
        }
        for (message_id, data) in [
            (
                "m1",
                json!({
                    "type": "text",
                    "text": "The following tool was executed by the user",
                    "synthetic": true
                }),
            ),
            (
                "m1",
                json!({"type": "text", "text": "Real OpenCode question"}),
            ),
            (
                "m2",
                json!({"type": "reasoning", "text": "hidden reasoning"}),
            ),
            ("m2", json!({"type": "step-start", "text": "step start"})),
            ("m2", json!({"type": "retry", "text": "retry bookkeeping"})),
            (
                "m2",
                json!({"type": "snapshot", "text": "snapshot bookkeeping"}),
            ),
            (
                "m2",
                json!({
                    "type": "text",
                    "text": "Here is code.\n```ts\nconst rich = true;\n```"
                }),
            ),
            (
                "m2",
                json!({
                    "type": "tool",
                    "tool": "bash",
                    "state": {
                        "status": "completed",
                        "input": {"command": "pnpm test"},
                        "output": "tests passed",
                        "title": "Run tests"
                    }
                }),
            ),
            (
                "m2",
                json!({
                    "type": "patch",
                    "path": "src/main.ts",
                    "text": "updated src/main.ts"
                }),
            ),
            (
                "m3",
                json!({"type": "text", "text": "Synthetic only", "synthetic": true}),
            ),
            (
                "m4",
                json!({"type": "text", "text": "Ignored only", "ignored": true}),
            ),
            ("m5", json!({"type": "compaction", "text": "summary only"})),
            (
                "m6",
                json!({"type": "file", "path": "/tmp/opencode/input.txt"}),
            ),
            (
                "m7",
                json!({"type": "text", "text": "Second OpenCode question"}),
            ),
        ] {
            conn.execute(
                "INSERT INTO part (session_id, message_id, data) VALUES (?1, ?2, ?3)",
                params!["open-session-rich", message_id, data.to_string()],
            )
            .unwrap();
        }
        drop(conn);

        let sessions = read_source_sessions(&source_fixture(
            "opencode",
            ConversationSourceKind::Sqlite,
            db_path.to_string_lossy().as_ref(),
        ))
        .unwrap();

        let turns = &sessions[0].turns;
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].user_text, "Real OpenCode question");
        assert_eq!(turns[1].user_text, "Second OpenCode question");
        assert!(turns[0].parts.iter().any(|part| {
            part.kind == ConversationPartKind::CodeBlock
                && part.language.as_deref() == Some("ts")
                && part.text.as_deref() == Some("const rich = true;")
        }));
        assert!(turns[0].parts.iter().any(|part| {
            part.kind == ConversationPartKind::Command
                && part.command.as_deref() == Some("pnpm test")
                && part
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("tests passed"))
        }));
        assert!(turns[0]
            .parts
            .iter()
            .any(|part| part.kind == ConversationPartKind::FileChange));
        assert!(turns[0].parts.iter().all(|part| {
            !part.text.as_deref().is_some_and(|text| {
                text.contains("hidden reasoning")
                    || text.contains("retry bookkeeping")
                    || text.contains("snapshot bookkeeping")
                    || text.contains("The following tool was executed")
            })
        }));
    }

    #[test]
    fn adapter_output_requires_complete_line() {
        let stdout = br#"{"type":"item","item":{"kind":"metadata"}}"#.to_vec();
        let error = parse_external_adapter_output("probe", stdout, Vec::new()).unwrap_err();
        assert!(error.contains("complete"));
    }

    #[test]
    fn adapter_output_rejects_oversized_line() {
        let stdout = vec![b'a'; DEFAULT_MAX_LINE_BYTES + 1];
        let error = parse_external_adapter_output("probe", stdout, Vec::new()).unwrap_err();
        assert!(error.contains("exceeds max line size"));
    }

    #[test]
    fn capped_reader_rejects_total_output_limit() {
        let input = vec![b'x'; 10];
        let error = read_capped(Cursor::new(input), 8).unwrap_err();
        assert!(error.contains("exceeded cap"));
    }

    #[cfg(unix)]
    #[test]
    fn external_adapter_validation_hash_changes_when_executable_changes() {
        let fixture = TempFixture::new("assetiweave-adapter-hash-fixture");
        let script = write_executable_script(
            fixture.path(),
            "adapter.sh",
            r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"complete","item":{}}'
"#,
        );
        let manifest = write_manifest(fixture.path(), vec!["adapter.sh".to_string()]);

        let before = validate_external_adapter(ExternalAdapterValidateParams {
            manifest_path: manifest.to_string_lossy().to_string(),
        })
        .unwrap();
        write_executable_script(
            fixture.path(),
            "adapter.sh",
            r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"warning","message":"changed"}'
printf '%s\n' '{"type":"complete","item":{}}'
"#,
        );
        let after = validate_external_adapter(ExternalAdapterValidateParams {
            manifest_path: manifest.to_string_lossy().to_string(),
        })
        .unwrap();

        assert_eq!(before.executable_path, script.to_string_lossy());
        assert_ne!(before.executable_hash, after.executable_hash);
    }

    #[cfg(unix)]
    #[test]
    fn external_adapter_try_run_parses_sessions_without_shell_joining_args() {
        let fixture = TempFixture::new("assetiweave-adapter-run-fixture");
        let hacked_path = fixture.path().join("hacked");
        write_executable_script(
            fixture.path(),
            "adapter.sh",
            r#"#!/bin/sh
printf '%s\n' "$1" >&2
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"session","session":{"external_id":"external-session-1","title":"External Fixture","project_path":null,"started_at":null,"updated_at":null,"source_locator":null,"source_fingerprint":null,"turns":[{"external_id":"turn-1","turn_index":0,"user_text":"External question","title":null,"started_at":null,"ended_at":null,"parts":[{"role":"assistant","kind":"text","text":"External answer","language":null,"command":null,"cwd":null,"status":null,"exit_code":null,"metadata_json":null}]}]}}}'
printf '%s\n' '{"type":"warning","message":"fixture warning"}'
printf '%s\n' '{"type":"complete","item":{"session_count":1}}'
"#,
        );
        let injection_arg = format!("literal; touch {}", hacked_path.display());
        let manifest = write_manifest(
            fixture.path(),
            vec!["adapter.sh".to_string(), injection_arg.clone()],
        );

        let result = try_run_external_adapter(ExternalAdapterTryRunParams {
            manifest_path: manifest.to_string_lossy().to_string(),
            method: "read_session".to_string(),
            location: Some(fixture.path().to_string_lossy().to_string()),
            session_id: Some("external-session-1".to_string()),
            yes: true,
        })
        .unwrap();

        assert_eq!(result.item_count, 1);
        assert_eq!(result.warning_count, 1);
        assert_eq!(result.sessions[0].turns[0].user_text, "External question");
        assert!(result.stderr.contains(&injection_arg));
        assert!(!hacked_path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn external_adapter_sync_reads_registered_adapter_sessions() {
        let fixture = TempFixture::new("assetiweave-adapter-sync-fixture");
        write_executable_script(
            fixture.path(),
            "adapter.sh",
            r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"session","session":{"external_id":"web-session-1","title":"Web Fixture","project_path":null,"started_at":null,"updated_at":null,"source_locator":"fixture://web-session-1","source_fingerprint":"fixture-hash","turns":[{"external_id":"turn-1","turn_index":0,"user_text":"Web question","title":null,"started_at":null,"ended_at":null,"parts":[{"role":"assistant","kind":"text","text":"Web answer","language":null,"command":null,"cwd":null,"status":null,"exit_code":null,"metadata_json":null}]}]}}}'
printf '%s\n' '{"type":"complete","item":{"session_count":1}}'
"#,
        );
        let manifest = write_manifest(fixture.path(), vec!["adapter.sh".to_string()]);
        let adapter = ConversationAdapter {
            id: "fixture-external".to_string(),
            name: "Fixture External".to_string(),
            kind: ConversationAdapterKind::External,
            version: "0.1.0".to_string(),
            enabled: true,
            manifest_path: Some(manifest.to_string_lossy().to_string()),
            executable_path: Some(
                fixture
                    .path()
                    .join("adapter.sh")
                    .to_string_lossy()
                    .to_string(),
            ),
            content_hash: None,
            trusted_hash: None,
            trust_state: ConversationAdapterTrustState::Trusted,
            protocol_version: Some(EXTERNAL_ADAPTER_PROTOCOL_VERSION),
            capabilities: vec!["read_session".to_string()],
            input_kinds: vec![ConversationSourceKind::Directory],
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let source = source_fixture(
            "fixture-external",
            ConversationSourceKind::Directory,
            &fixture.path().to_string_lossy(),
        );

        let sessions = read_source_sessions_with_adapter(Some(&adapter), &source).unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].external_id, "web-session-1");
        assert_eq!(sessions[0].turns[0].user_text, "Web question");
        assert_eq!(
            sessions[0].turns[0].parts[0].text.as_deref(),
            Some("Web answer")
        );
    }

    #[cfg(unix)]
    #[test]
    fn external_adapter_run_times_out() {
        let fixture = TempFixture::new("assetiweave-adapter-timeout-fixture");
        write_executable_script(
            fixture.path(),
            "adapter.sh",
            r#"#!/bin/sh
cat >/dev/null
sleep 1
printf '%s\n' '{"type":"complete","item":{}}'
"#,
        );
        let manifest = write_manifest(fixture.path(), vec!["adapter.sh".to_string()]);
        let validation =
            validate_external_adapter_manifest(manifest.to_string_lossy().as_ref()).unwrap();

        let error = run_external_adapter(
            &validation,
            "probe",
            json!({"method":"probe"}),
            Duration::from_millis(50),
        )
        .unwrap_err();

        assert!(error.contains("timed out"));
    }

    fn source_fixture(
        adapter_id: &str,
        kind: ConversationSourceKind,
        location: &str,
    ) -> ConversationSource {
        ConversationSource {
            id: format!("{adapter_id}-fixture"),
            adapter_id: adapter_id.to_string(),
            name: format!("{adapter_id} fixture"),
            kind,
            location: location.to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[cfg(unix)]
    fn write_executable_script(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        path
    }

    #[cfg(unix)]
    fn write_manifest(dir: &Path, command: Vec<String>) -> PathBuf {
        let manifest = ConversationAdapterManifest {
            schema_version: 1,
            id: "fixture-external".to_string(),
            name: "Fixture External".to_string(),
            version: "0.1.0".to_string(),
            protocol_version: EXTERNAL_ADAPTER_PROTOCOL_VERSION,
            command,
            capabilities: vec![
                "probe".to_string(),
                "list_sessions".to_string(),
                "read_session".to_string(),
            ],
            input_kinds: vec![ConversationSourceKind::Directory],
        };
        let path = dir.join("conversation-adapter.json");
        fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
        path
    }
}
