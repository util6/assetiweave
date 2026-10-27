use crate::args::OutputFormat;
use crate::render::render_json_value;

pub fn render_memory_markdown_view(val: &serde_json::Value, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            render_json_value(val);
        }
        OutputFormat::Table => {
            let exists = val.get("exists").and_then(|v| v.as_bool()).unwrap_or(false);
            let content = val.get("content").and_then(|v| v.as_str()).unwrap_or("");
            if !exists || content.trim().is_empty() {
                println!("暂无记录");
            } else {
                println!("{}", content.trim_end());
            }
        }
    }
}
