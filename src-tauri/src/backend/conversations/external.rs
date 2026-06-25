use super::prelude::*;

pub(super) fn read_external_adapter_sessions(
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
    let manifest_path = adapter.manifest_path.as_deref().ok_or_else(|| {
        format!(
            "external conversation adapter has no manifest: {}",
            adapter.id
        )
    })?;
    let validation = validate_external_adapter_manifest(manifest_path)?;
    validate_external_adapter_manifest_for_method(adapter, &validation, "read_session")?;
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
        let executable_matches = validation.executable_hash.as_deref() == Some(trusted_hash);
        let manifest_matches = validation.manifest_hash == trusted_hash;
        if !executable_matches && !manifest_matches {
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
    let manifest = ConversationAdapterManifest {
        schema_version: 1,
        id: params.id,
        name: params.name,
        version: "0.1.0".to_string(),
        protocol_version: EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        command: Vec::new(),
        runtime: Some(ConversationAdapterRuntime {
            kind: ConversationAdapterRuntimeKind::Executable,
            entry: "adapter-executable".to_string(),
            args: Vec::new(),
            version: None,
        }),
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
) -> AppResult<Vec<ConversationAdapterRuntimeStatus>> {
    Ok(list_adapter_runtime_statuses())
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
    let invocation = build_adapter_invocation(manifest_dir, manifest)?;
    if let Some(runtime) = manifest.runtime.as_ref() {
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
    let mut sessions = Vec::new();
    let mut markdown_export = None;
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
                match adapter_item_kind(&item) {
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
        markdown_export,
        warnings,
        stderr,
    })
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
