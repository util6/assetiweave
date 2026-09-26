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
