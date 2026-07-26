use regex::{Captures, Regex};
use std::sync::OnceLock;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MemoryRedactionResult {
    pub(crate) text: String,
    pub(crate) redaction_count: usize,
}

pub(crate) fn redact_memory_text(value: &str) -> MemoryRedactionResult {
    let mut text = value.to_string();
    let mut redaction_count = 0;

    text = replace_all(
        &text,
        private_key_pattern(),
        "private_key",
        &mut redaction_count,
    );
    text = replace_header_value(
        &text,
        authorization_header_pattern(),
        "bearer_token",
        &mut redaction_count,
    );
    text = replace_all(
        &text,
        bearer_pattern(),
        "bearer_token",
        &mut redaction_count,
    );
    text = replace_header_value(
        &text,
        cookie_header_pattern(),
        "cookie",
        &mut redaction_count,
    );
    text = replace_all(&text, api_key_pattern(), "api_key", &mut redaction_count);
    text = replace_high_entropy_tokens(&text, &mut redaction_count);

    MemoryRedactionResult {
        text,
        redaction_count,
    }
}

fn replace_all(text: &str, pattern: &Regex, kind: &str, count: &mut usize) -> String {
    pattern
        .replace_all(text, |_: &Captures<'_>| {
            *count += 1;
            format!("[REDACTED:{kind}]")
        })
        .into_owned()
}

fn replace_header_value(text: &str, pattern: &Regex, kind: &str, count: &mut usize) -> String {
    pattern
        .replace_all(text, |captures: &Captures<'_>| {
            *count += 1;
            format!("{} [REDACTED:{kind}]", &captures[1])
        })
        .into_owned()
}

fn replace_high_entropy_tokens(text: &str, count: &mut usize) -> String {
    high_entropy_token_pattern()
        .replace_all(text, |captures: &Captures<'_>| {
            let token = &captures[0];
            if looks_like_high_entropy_secret(token) {
                *count += 1;
                "[REDACTED:high_entropy]".to_string()
            } else {
                token.to_string()
            }
        })
        .into_owned()
}

fn looks_like_high_entropy_secret(value: &str) -> bool {
    if value.len() < 32 || value.chars().all(|character| character == '-') {
        return false;
    }

    let mut has_lower = false;
    let mut has_upper = false;
    let mut has_digit = false;
    let mut has_symbol = false;
    for character in value.chars() {
        has_lower |= character.is_ascii_lowercase();
        has_upper |= character.is_ascii_uppercase();
        has_digit |= character.is_ascii_digit();
        has_symbol |= !character.is_ascii_alphanumeric();
    }
    let character_classes = [has_lower, has_upper, has_digit, has_symbol]
        .into_iter()
        .filter(|present| *present)
        .count();
    if character_classes < 2 {
        return false;
    }

    shannon_entropy(value) >= 3.5
}

fn shannon_entropy(value: &str) -> f64 {
    let mut frequencies = [0_usize; 256];
    let mut length = 0_usize;
    for byte in value.bytes() {
        frequencies[byte as usize] += 1;
        length += 1;
    }
    if length == 0 {
        return 0.0;
    }

    frequencies
        .into_iter()
        .filter(|frequency| *frequency > 0)
        .map(|frequency| {
            let probability = frequency as f64 / length as f64;
            -probability * probability.log2()
        })
        .sum()
}

fn private_key_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----",
        )
        .expect("private key regex")
    })
}

fn authorization_header_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?im)^(authorization\s*:\s*bearer)\s+[^\r\n]+$")
            .expect("authorization header regex")
    })
}

fn bearer_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{16,}").expect("bearer regex")
    })
}

fn cookie_header_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?im)^(cookie|set-cookie)\s*:\s*[^\r\n]+$").expect("cookie regex")
    })
}

fn api_key_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?x)\b(?:
                sk-(?:proj-)?[A-Za-z0-9_-]{16,}|
                gh[pousr]_[A-Za-z0-9_]{16,}|
                github_pat_[A-Za-z0-9_]{16,}|
                AIza[0-9A-Za-z_-]{20,}|
                AKIA[0-9A-Z]{16}|
                xox[baprs]-[A-Za-z0-9-]{10,}
            )\b",
        )
        .expect("API key regex")
    })
}

fn high_entropy_token_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"[A-Za-z0-9][A-Za-z0-9_+/=.-]{31,}").expect("high entropy token regex")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_supported_secret_fixtures_before_external_ai_use() {
        let input = concat!(
            "OpenAI: sk-proj-1234567890abcdefghijklmnopqrstuvwxyz\n",
            "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.abcdefghijklmnopqrstuvwxyz.1234567890\n",
            "Cookie: session=super-secret-cookie; theme=dark\n",
            "-----BEGIN PRIVATE KEY-----\n",
            "MIIEvQIBADANBgkqhkiG9w0BAQEFAASC1234567890abcdefghijklmnopqrstuvwxyz\n",
            "-----END PRIVATE KEY-----\n",
            "opaque=QWxhZGRpbjpPcGVuU2VzYW1lMTIzNDU2Nzg5MC9hYmNkZWZnaGlqa2xtbm9w\n",
            "safe=short-value\n",
        );

        let result = redact_memory_text(input);

        assert!(!result.text.contains("sk-proj-"));
        assert!(!result.text.contains("eyJhbGci"));
        assert!(!result.text.contains("super-secret-cookie"));
        assert!(!result.text.contains("MIIEvQIB"));
        assert!(!result.text.contains("QWxhZGRp"));
        assert!(result.text.contains("safe=short-value"));
        assert!(result.redaction_count >= 5);
    }

    #[test]
    fn redaction_is_idempotent() {
        let first = redact_memory_text("Bearer abcdefghijklmnopqrstuvwxyz1234567890");
        let second = redact_memory_text(&first.text);

        assert_eq!(second.text, first.text);
        assert_eq!(second.redaction_count, 0);
    }
}
