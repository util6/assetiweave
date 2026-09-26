use validator::ValidationError;

pub(crate) fn validate_non_blank(value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        Err(ValidationError::new("required"))
    } else {
        Ok(())
    }
}

pub(crate) fn validate_max_120_bytes(value: &str) -> Result<(), ValidationError> {
    if value.len() > 120 {
        Err(ValidationError::new("length_bytes"))
    } else {
        Ok(())
    }
}

pub(crate) fn validate_max_500_bytes(value: &str) -> Result<(), ValidationError> {
    if value.len() > 500 {
        Err(ValidationError::new("length_bytes"))
    } else {
        Ok(())
    }
}

pub(crate) fn validate_no_null_bytes(value: &str) -> Result<(), ValidationError> {
    if value.contains('\0') {
        Err(ValidationError::new("null_bytes"))
    } else {
        Ok(())
    }
}

use serde_json::Value;

pub(crate) fn sanitize_public_message(message: &str) -> String {
    let normalized = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "The operation failed.".to_string();
    }
    let lower = normalized.to_ascii_lowercase();
    let contains_absolute_path = normalized.split_whitespace().any(|word| {
        word.starts_with('/')
            || word.starts_with("~/")
            || word.get(1..3).is_some_and(|drive| {
                drive.starts_with(':')
                    && (word.as_bytes().get(2) == Some(&b'\\')
                        || word.as_bytes().get(2) == Some(&b'/'))
            })
    });
    if contains_absolute_path
        || lower.contains("sql")
        || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("authorization")
        || lower.contains("password")
        || lower.contains("prompt=")
        || lower.contains("prompt:")
        || lower.contains("environment")
    {
        return "The operation failed.".to_string();
    }
    normalized.chars().take(500).collect()
}

pub(crate) fn sanitize_details(value: &Value) -> Option<Value> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => Some(value.clone()),
        Value::String(message) => Some(Value::String(
            if sanitize_public_message(message) == "The operation failed."
                && !message.trim().is_empty()
            {
                "<redacted>".to_string()
            } else {
                message.chars().take(500).collect()
            },
        )),
        Value::Array(values) => Some(Value::Array(
            values.iter().filter_map(sanitize_details).collect(),
        )),
        Value::Object(values) => Some(Value::Object(
            values
                .iter()
                .filter_map(|(key, value)| {
                    let lower_key = key.to_ascii_lowercase();
                    if lower_key.contains("token")
                        || lower_key.contains("secret")
                        || lower_key.contains("password")
                        || lower_key.contains("prompt")
                        || lower_key.contains("environment")
                    {
                        return None;
                    }
                    sanitize_details(value).map(|value| (key.clone(), value))
                })
                .collect(),
        )),
    }
}

pub(crate) fn format_validation_errors(errors: &validator::ValidationErrors) -> String {
    fn collect_codes(prefix: &str, errors: &validator::ValidationErrors, out: &mut Vec<String>) {
        for (field, kind) in errors.errors() {
            let path = if prefix.is_empty() {
                field.to_string()
            } else {
                format!("{prefix}.{field}")
            };
            match kind {
                validator::ValidationErrorsKind::Field(errs) => {
                    for err in errs {
                        out.push(format!("{path}: {}", err.code));
                    }
                }
                validator::ValidationErrorsKind::Struct(nested) => {
                    collect_codes(&path, nested, out);
                }
                validator::ValidationErrorsKind::List(items) => {
                    for (index, nested) in items {
                        collect_codes(&format!("{path}[{index}]"), nested, out);
                    }
                }
            }
        }
    }

    let mut details = Vec::new();
    collect_codes("", errors, &mut details);
    details.sort();
    if details.is_empty() {
        "validation failed".to_string()
    } else {
        format!("validation failed: {}", details.join(", "))
    }
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
