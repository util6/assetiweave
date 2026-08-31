use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const EMBEDDING_DIMENSIONS: usize = 64;

#[derive(Debug, Clone)]
pub(crate) struct SemanticDocument {
    pub(crate) key: String,
    pub(crate) text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SemanticMatch {
    pub(crate) key: String,
    pub(crate) score: u64,
}

/// Build a local, deterministic semantic index for a bounded candidate set.
///
/// This is deliberately a derived read model: it has no persistence authority,
/// no network dependency, and is rebuilt from current Conversation facts for
/// every Recall request. The hashed token vectors keep the seam replaceable by
/// a persisted embedding provider without changing the Recall contract.
pub(crate) fn rank_documents(
    query: &str,
    documents: &[SemanticDocument],
    limit: usize,
) -> Vec<SemanticMatch> {
    let query_vector = embedding(query);
    let query_tokens = semantic_tokens(query);
    let mut matches = documents
        .iter()
        .filter(|document| !query_tokens.is_disjoint(&semantic_tokens(&document.text)))
        .map(|document| SemanticMatch {
            key: document.key.clone(),
            score: cosine_score(&query_vector, &embedding(&document.text)),
        })
        .filter(|item| item.score > 0)
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.key.cmp(&right.key))
    });
    matches.truncate(limit);
    matches
}

fn embedding(text: &str) -> [f32; EMBEDDING_DIMENSIONS] {
    let mut vector = [0.0; EMBEDDING_DIMENSIONS];
    for token in semantic_tokens(text) {
        let digest = Sha256::digest(token.as_bytes());
        for (index, byte) in digest.iter().take(8).enumerate() {
            let dimension = (index * 8) + usize::from(byte % 8);
            let sign = if byte & 1 == 0 { 1.0 } else { -1.0 };
            vector[dimension] += sign;
        }
    }
    let length = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if length > f32::EPSILON {
        for value in &mut vector {
            *value /= length;
        }
    }
    vector
}

fn cosine_score(left: &[f32; EMBEDDING_DIMENSIONS], right: &[f32; EMBEDDING_DIMENSIONS]) -> u64 {
    let cosine = left
        .iter()
        .zip(right.iter())
        .map(|(left, right)| left * right)
        .sum::<f32>()
        .clamp(0.0, 1.0);
    (cosine * 10_000.0).round() as u64
}

fn semantic_tokens(text: &str) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    let mut ascii_word = String::new();
    let mut previous_cjk = None;
    let flush_ascii = |tokens: &mut BTreeSet<String>, word: &mut String| {
        if word.is_empty() {
            return;
        }
        let normalized = normalize_ascii_word(word);
        if !normalized.is_empty() {
            add_with_expansions(tokens, normalized);
        }
        word.clear();
    };

    for character in text.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            ascii_word.push(character.to_ascii_lowercase());
            previous_cjk = None;
            continue;
        }
        flush_ascii(&mut tokens, &mut ascii_word);
        if character.is_whitespace() || character.is_ascii_punctuation() {
            previous_cjk = None;
            continue;
        }
        let current = character.to_string();
        add_with_expansions(&mut tokens, current.clone());
        if let Some(previous) = previous_cjk {
            add_with_expansions(&mut tokens, format!("{previous}{character}"));
        }
        previous_cjk = Some(character);
    }
    flush_ascii(&mut tokens, &mut ascii_word);
    tokens
}

fn normalize_ascii_word(word: &str) -> String {
    let word = word.to_ascii_lowercase();
    for suffix in ["ing", "ed", "es", "s"] {
        if word.len() > suffix.len() + 2 && word.ends_with(suffix) {
            return word[..word.len() - suffix.len()].to_string();
        }
    }
    word
}

fn add_with_expansions(tokens: &mut BTreeSet<String>, token: String) {
    if token.is_empty() {
        return;
    }
    tokens.insert(token.clone());
    for expansion in semantic_expansions(&token) {
        tokens.insert(expansion.to_string());
    }
}

fn semantic_expansions(token: &str) -> &'static [&'static str] {
    match token {
        "fix" | "repair" | "resolve" | "解决" | "修复" => {
            &["fix", "repair", "resolve", "解决", "修复"]
        }
        "bug" | "error" | "failure" | "错误" | "故障" => {
            &["bug", "error", "failure", "错误", "故障"]
        }
        "decision" | "choice" | "决定" | "选择" => &["decision", "choice", "决定", "选择"],
        "verify" | "test" | "验证" | "测试" => &["verify", "test", "验证", "测试"],
        "project" | "workspace" | "项目" | "工作区" => {
            &["project", "workspace", "项目", "工作区"]
        }
        "command" | "script" | "命令" | "脚本" => &["command", "script", "命令", "脚本"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_embeddings_recall_synonyms_and_tie_break_by_key() {
        let documents = vec![
            SemanticDocument {
                key: "b".to_string(),
                text: "repair the build failure".to_string(),
            },
            SemanticDocument {
                key: "a".to_string(),
                text: "resolve the error".to_string(),
            },
            SemanticDocument {
                key: "z".to_string(),
                text: "unrelated gardening notes".to_string(),
            },
        ];
        let first = rank_documents("fix bug", &documents, 10);
        let second = rank_documents("fix bug", &documents, 10);

        assert_eq!(first, second);
        assert_eq!(
            first
                .iter()
                .map(|item| item.key.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert!(first[0].score > 0);
    }

    #[test]
    fn chinese_semantic_terms_share_a_candidate_space() {
        let documents = vec![SemanticDocument {
            key: "zh".to_string(),
            text: "解决构建错误".to_string(),
        }];
        assert_eq!(rank_documents("修复故障", &documents, 1)[0].key, "zh");
    }
}
