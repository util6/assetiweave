use crate::backend::{
    application::AppService,
    runtime::{AppRuntime, RuntimeRole},
};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const MAX_TOOL_CALLS: usize = 24;
const MAX_SINGLE_RESPONSE_BYTES: usize = 32_000;
const MAX_CUMULATIVE_RESPONSE_BYTES: usize = 128_000;

#[derive(Default)]
struct ToolBudget {
    calls: usize,
    response_bytes: usize,
}

impl ToolBudget {
    fn begin_call(&mut self) -> Result<(), String> {
        if self.calls >= MAX_TOOL_CALLS {
            return Err("Memory Generation tool call budget exhausted".to_string());
        }
        self.calls += 1;
        Ok(())
    }

    fn record_response(&mut self, value: &Value) -> Result<(), String> {
        let bytes = serde_json::to_vec(value)
            .map_err(|error| error.to_string())?
            .len();
        if bytes > MAX_SINGLE_RESPONSE_BYTES {
            return Err("Memory Generation single tool response budget exceeded".to_string());
        }
        if self.response_bytes.saturating_add(bytes) > MAX_CUMULATIVE_RESPONSE_BYTES {
            return Err("Memory Generation cumulative tool response budget exhausted".to_string());
        }
        self.response_bytes += bytes;
        Ok(())
    }
}

pub(crate) fn run_memory_generation_mcp_stdio() -> Result<(), String> {
    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to initialize Memory Generation MCP runtime: {error}"))?;
    let db_path = crate::backend::path_utils::app_db_path().map_err(|error| error.to_string())?;
    let runtime = tokio_runtime
        .block_on(AppRuntime::bootstrap(db_path, RuntimeRole::OneShot))
        .map_err(|error| format!("failed to initialize Memory Generation MCP database: {error}"))?;
    let tenant_id = required_env("ASSETIWEAVE_MEMORY_GENERATION_TENANT_ID")?;
    let job_id = required_env("ASSETIWEAVE_MEMORY_GENERATION_JOB_ID")?;
    let ownership_token = required_env("ASSETIWEAVE_MEMORY_GENERATION_OWNERSHIP_TOKEN")?;
    tokio_runtime
        .block_on(runtime.activate_tenant(&tenant_id))
        .map_err(|error| format!("failed to activate Memory Generation MCP tenant: {error}"))?;
    let service = AppService::from_runtime(&runtime);
    run_loop(
        &tokio_runtime,
        &service,
        &job_id,
        &ownership_token,
        std::io::stdin().lock(),
        std::io::BufWriter::new(std::io::stdout()),
    )
}

fn run_loop<R: BufRead, W: Write>(
    tokio_runtime: &tokio::runtime::Runtime,
    service: &AppService,
    job_id: &str,
    ownership_token: &str,
    reader: R,
    mut writer: W,
) -> Result<(), String> {
    let mut budget = ToolBudget::default();
    for line in reader.lines() {
        let line = line.map_err(|error| error.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = serde_json::from_str(&line).map_err(|error| error.to_string())?;
        let id = request.get("id").cloned();
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if method.starts_with("notifications/") {
            continue;
        }
        let result = match method {
            "initialize" => Ok(initialize_result()),
            "tools/list" => Ok(tools_result()),
            "tools/call" => budget.begin_call().and_then(|()| {
                let value = tokio_runtime.block_on(call_tool(
                    service,
                    job_id,
                    ownership_token,
                    request.get("params").unwrap_or(&Value::Null),
                ))?;
                budget.record_response(&value)?;
                Ok(value)
            }),
            _ => Err("unsupported Memory Generation MCP method".to_string()),
        };
        let response = match (id, result) {
            (Some(id), Ok(result)) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            (Some(id), Err(error)) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32001, "message": error }
            }),
            (None, _) => continue,
        };
        serde_json::to_writer(&mut writer, &response).map_err(|error| error.to_string())?;
        writer.write_all(b"\n").map_err(|error| error.to_string())?;
        writer.flush().map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": "2025-06-18",
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": {
            "name": "assetiweave-memory-generation",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

pub(crate) fn tools_result() -> Value {
    let object = |properties: Value, required: &[&str]| {
        json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        })
    };
    json!({
        "tools": [
            {
                "name": "get_session_outline",
                "description": "Read the frozen structured facts and bounded node index for one Work Order session.",
                "inputSchema": object(json!({
                    "session_ref": { "type": "string" }
                }), &["session_ref"])
            },
            {
                "name": "search_session_content",
                "description": "Search only source-referenced content inside one frozen Work Order session.",
                "inputSchema": object(json!({
                    "session_ref": { "type": "string" },
                    "query": { "type": "string" },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "maximum": MAX_SEARCH_RESULTS }
                }), &["session_ref", "query"])
            },
            {
                "name": "read_question_content",
                "description": "Read a bounded slice of a question returned by get_session_outline.",
                "inputSchema": object(json!({
                    "session_ref": { "type": "string" },
                    "question_ref": { "type": "string" },
                    "offset": { "type": ["integer", "null"], "minimum": 0 },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "maximum": 24000 }
                }), &["session_ref", "question_ref"])
            },
            {
                "name": "read_content_node",
                "description": "Read one source-referenced content node returned by get_session_outline.",
                "inputSchema": object(json!({
                    "session_ref": { "type": "string" },
                    "node_ref": { "type": "string" }
                }), &["session_ref", "node_ref"])
            }
        ]
    })
}

async fn call_tool(
    service: &AppService,
    job_id: &str,
    ownership_token: &str,
    params: &Value,
) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let value = service
        .call_memory_generation_tool(job_id, ownership_token, name, &arguments)
        .await
        .map_err(|error| error.view().message)?;
    Ok(json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&value).map_err(|error| error.to_string())?
        }]
    }))
}

fn required_env(key: &str) -> Result<String, String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing Memory Generation MCP environment: {key}"))
}

const MAX_SEARCH_RESULTS: usize = 24;

#[cfg(test)]
#[path = "memory_generation_mcp_tests.rs"]
mod tests;
