use sha2::{Digest, Sha256};

fn decode_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        let h = decode_hex_nibble(chunk[0])?;
        let l = decode_hex_nibble(chunk[1])?;
        out.push((h << 4) | l);
    }
    Some(out)
}

/// Computes a stable, content-addressed identifier from a prefix and list of parts.
pub fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(b"\0");
    }
    let hash = format!("{:x}", hasher.finalize());
    format!("{prefix}-{hash}")
}

/// Derives the canonical conversation-part ID from turn_id and part_index.
pub fn derive_conversation_part_id(turn_id: &str, part_index: i64) -> String {
    derive_part_id("conversation-part", turn_id, part_index)
}

/// Derives the canonical web-record-part ID from turn_id and part_index.
pub fn derive_web_record_part_id(turn_id: &str, part_index: i64) -> String {
    derive_part_id("web-record-part", turn_id, part_index)
}

/// Derives a part ID with a specific prefix.
pub fn derive_part_id(prefix: &str, turn_id: &str, part_index: i64) -> String {
    let index_str = part_index.to_string();
    stable_id(prefix, &[turn_id, &index_str])
}

/// Derives a canonical standard part ID with a specific prefix.
pub fn derive_standard_part_id(prefix: &str, turn_id: &str, part_index: i64) -> String {
    derive_part_id(prefix, turn_id, part_index)
}

/// Extracts the first 16 hex characters (8 bytes) of the SHA-256 digest from a part ID.
pub fn derive_part_id_key(part_id: &str) -> Option<[u8; 8]> {
    let hex_str = if let Some(idx) = part_id.rfind('-') {
        &part_id[idx + 1..]
    } else {
        part_id
    };
    if hex_str.len() >= 16 {
        let bytes = decode_hex(&hex_str[..16])?;
        bytes.try_into().ok()
    } else {
        None
    }
}

/// Lookup strategy for querying parts by binary id_key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdKeyLookup {
    /// Exact match on the full 8-byte id_key.
    Exact([u8; 8]),
    /// Range scan [low, high) where high is None if the range overflows 0xFF.
    Range { low: [u8; 8], high: Option<[u8; 8]> },
}

/// Result of parsing a user-supplied identifier or prefix for part indexing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedIdPrefix {
    pub lookup: IdKeyLookup,
    pub normalized_hex: String,
    pub node_order: Option<usize>,
}

/// Parses a short ID, full ID, or prefix into an `IdKeyLookup` for index querying.
pub fn parse_id_key_prefix(input: &str) -> Option<ParsedIdPrefix> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. Strip -node-N suffix if present
    let (without_node, node_order) = if let Some(pos) = trimmed.rfind("-node-") {
        let order_str = &trimmed[pos + 6..];
        if let Ok(order) = order_str.parse::<usize>() {
            (&trimmed[..pos], Some(order))
        } else {
            (trimmed, None)
        }
    } else {
        (trimmed, None)
    };

    // 2. Strip standard entity prefixes
    let clean = without_node
        .strip_prefix("conversation-part-")
        .or_else(|| without_node.strip_prefix("web-record-part-"))
        .or_else(|| without_node.strip_prefix("conversation-turn-"))
        .or_else(|| without_node.strip_prefix("web-record-turn-"))
        .or_else(|| without_node.strip_prefix("conversation-session-"))
        .or_else(|| without_node.strip_prefix("web-record-session-"))
        .or_else(|| without_node.strip_prefix("part-"))
        .or_else(|| without_node.strip_prefix("turn-"))
        .or_else(|| without_node.strip_prefix("session-"))
        .unwrap_or(without_node);

    // 3. Ensure clean is valid hex and non-empty
    if clean.is_empty() || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    let normalized_hex = clean.to_ascii_lowercase();

    // 4. Exact match if >= 16 hex characters
    if normalized_hex.len() >= 16 {
        let bytes = decode_hex(&normalized_hex[..16])?;
        let exact: [u8; 8] = bytes.try_into().ok()?;
        return Some(ParsedIdPrefix {
            lookup: IdKeyLookup::Exact(exact),
            normalized_hex,
            node_order,
        });
    }

    // 5. Short prefix (1..15 hex characters) -> construct [low, high) range
    let len = normalized_hex.len();
    let mut low = [0u8; 8];

    if len % 2 == 0 {
        // Even number of hex chars
        let k = len / 2;
        let bytes = decode_hex(&normalized_hex)?;
        low[..k].copy_from_slice(&bytes);

        let mut high = low;
        let mut overflow = true;
        for i in (0..k).rev() {
            if high[i] < 0xFF {
                high[i] += 1;
                overflow = false;
                break;
            } else {
                high[i] = 0x00;
            }
        }
        let high_opt = if overflow { None } else { Some(high) };

        Some(ParsedIdPrefix {
            lookup: IdKeyLookup::Range {
                low,
                high: high_opt,
            },
            normalized_hex,
            node_order,
        })
    } else {
        // Odd number of hex chars (nibble at the end)
        let k = len / 2;
        if k > 0 {
            let bytes = decode_hex(&normalized_hex[..k * 2])?;
            low[..k].copy_from_slice(&bytes);
        }
        let nibble_val = decode_hex_nibble(normalized_hex.as_bytes()[len - 1])?;
        low[k] = nibble_val << 4;

        let mut high = low;
        let mut overflow = true;
        if nibble_val < 15 {
            high[k] = (nibble_val + 1) << 4;
            overflow = false;
        } else {
            high[k] = 0x00;
            for i in (0..k).rev() {
                if high[i] < 0xFF {
                    high[i] += 1;
                    overflow = false;
                    break;
                } else {
                    high[i] = 0x00;
                }
            }
        }
        let high_opt = if overflow { None } else { Some(high) };

        Some(ParsedIdPrefix {
            lookup: IdKeyLookup::Range {
                low,
                high: high_opt,
            },
            normalized_hex,
            node_order,
        })
    }
}

#[cfg(test)]
#[path = "id_tests.rs"]
mod tests;
