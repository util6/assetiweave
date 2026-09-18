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

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
