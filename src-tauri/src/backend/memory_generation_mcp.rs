use crate::backend::application::AppService;
use serde_json::{json, Value};

const MAX_TOOL_CALLS: usize = 24;
const MAX_SINGLE_RESPONSE_BYTES: usize = 32_000;
const MAX_CUMULATIVE_RESPONSE_BYTES: usize = 128_000;

#[derive(Default)]
pub(crate) struct ToolBudget {
    calls: usize,
    response_bytes: usize,
}

impl ToolBudget {
    pub(crate) fn begin_call(&mut self) -> Result<(), String> {
        if self.calls >= MAX_TOOL_CALLS {
            return Err("Memory Generation tool call budget exhausted".to_string());
        }
        self.calls += 1;
        Ok(())
    }

    pub(crate) fn record_response(&mut self, value: &Value) -> Result<(), String> {
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

pub(crate) fn initialize_result() -> Value {
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

pub(crate) async fn call_tool(
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

pub(crate) fn required_env(key: &str) -> Result<String, String> {
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
