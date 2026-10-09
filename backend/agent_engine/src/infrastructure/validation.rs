pub fn sanitize_public_message(message: &str) -> String {
    let normalized = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "The operation failed.".to_string();
    }
    normalized
}
