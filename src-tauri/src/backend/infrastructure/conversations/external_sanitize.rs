use super::prelude::*;

pub(crate) fn sanitize_adapter_progress(line: &ExternalAdapterLine) -> ExternalAdapterProgress {
    let nested = line.progress.as_ref().and_then(|v| v.as_object());

    let stage = line
        .stage
        .as_deref()
        .or_else(|| nested.and_then(|o| o.get("stage")).and_then(|v| v.as_str()))
        .map(|s| sanitize_progress_text(s, 64));

    let operation = line
        .operation
        .as_deref()
        .or_else(|| {
            nested
                .and_then(|o| o.get("operation"))
                .and_then(|v| v.as_str())
        })
        .map(|s| sanitize_progress_text(s, 128));

    let path = line
        .path
        .as_deref()
        .or_else(|| nested.and_then(|o| o.get("path")).and_then(|v| v.as_str()))
        .map(|s| sanitize_progress_path(s, 512));

    let current = line.current.or_else(|| {
        nested
            .and_then(|o| o.get("current"))
            .and_then(|v| v.as_u64())
    });

    let total = line
        .total
        .or_else(|| nested.and_then(|o| o.get("total")).and_then(|v| v.as_u64()));

    let worker = line
        .worker
        .as_deref()
        .or_else(|| {
            nested
                .and_then(|o| o.get("worker"))
                .and_then(|v| v.as_str())
        })
        .map(|s| sanitize_progress_text(s, 64));

    ExternalAdapterProgress {
        stage,
        operation,
        path,
        current,
        total,
        worker,
    }
}

pub(crate) fn sanitize_progress_text(text: &str, max_len: usize) -> String {
    let sanitized: String = text
        .chars()
        .filter(|c| !c.is_control() && *c != '<' && *c != '>')
        .take(max_len)
        .collect();
    sanitized.trim().to_string()
}

pub(crate) fn sanitize_progress_path(path_str: &str, max_len: usize) -> String {
    let sanitized: String = path_str
        .chars()
        .filter(|c| !c.is_control() && *c != '<' && *c != '>')
        .take(max_len)
        .collect();
    let trimmed = sanitized.trim();
    trimmed.replace('\\', "/")
}
