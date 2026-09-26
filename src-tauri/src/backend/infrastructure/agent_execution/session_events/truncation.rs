use super::types::TruncationInfo;

pub(crate) fn truncate_head_tail(input: &str, max_bytes: usize) -> (String, TruncationInfo) {
    let original_bytes = input.len();
    if original_bytes <= max_bytes {
        return (
            input.to_string(),
            TruncationInfo {
                original_bytes,
                retained_bytes: original_bytes,
                strategy: "headTail".to_string(),
            },
        );
    }

    const MARKER: &str = "\n... [truncated] ...\n";
    if max_bytes <= MARKER.len() {
        let mut cutoff = max_bytes;
        while !input.is_char_boundary(cutoff) && cutoff > 0 {
            cutoff -= 1;
        }
        let truncated = input[..cutoff].to_string();
        let retained_bytes = truncated.len();
        return (
            truncated,
            TruncationInfo {
                original_bytes,
                retained_bytes,
                strategy: "headTail".to_string(),
            },
        );
    }

    let available = max_bytes - MARKER.len();
    let head_budget = (available * 3) / 4;
    let tail_budget = available - head_budget;

    let mut head_idx = head_budget.min(input.len());
    while !input.is_char_boundary(head_idx) && head_idx > 0 {
        head_idx -= 1;
    }
    let head = &input[..head_idx];

    let mut tail_start = input.len().saturating_sub(tail_budget);
    while !input.is_char_boundary(tail_start) && tail_start < input.len() {
        tail_start += 1;
    }
    let tail = &input[tail_start..];

    let combined = format!("{head}{MARKER}{tail}");
    let retained_bytes = combined.len();
    (
        combined,
        TruncationInfo {
            original_bytes,
            retained_bytes,
            strategy: "headTail".to_string(),
        },
    )
}
