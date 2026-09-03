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
pub(crate) mod tests {
    use super::*;

    #[test]
    fn non_blank_rejects_empty_or_whitespace_strings() {
        assert!(validate_non_blank("").is_err());
        assert!(validate_non_blank("   ").is_err());
        assert!(validate_non_blank("\t\n ").is_err());
        assert!(validate_non_blank("valid").is_ok());
        assert!(validate_non_blank("  valid  ").is_ok());
    }

    #[test]
    fn byte_limit_preserves_multibyte_boundary() {
        assert!(validate_max_120_bytes(&"界".repeat(40)).is_ok());
        assert!(validate_max_120_bytes(&"界".repeat(41)).is_err());
    }

    #[test]
    fn max_500_bytes_works_correctly() {
        assert!(validate_max_500_bytes(&"a".repeat(500)).is_ok());
        assert!(validate_max_500_bytes(&"a".repeat(501)).is_err());
    }

    #[test]
    fn no_null_bytes_works_correctly() {
        assert!(validate_no_null_bytes("hello world").is_ok());
        assert!(validate_no_null_bytes("hello\0world").is_err());
    }
}
