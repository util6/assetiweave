use super::*;
use tantivy::query::AllQuery;

impl ConversationSearchDocument {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn scoped_card(
        record_kind: &str,
        session_id: &str,
        question_id: &str,
        document_id: &str,
        card_kind: &str,
        question_title: &str,
        content: &str,
        adapter_id: &str,
        source_id: &str,
        project_path: &str,
        turn_id: &str,
        part_id: &str,
    ) -> Self {
        Self::scoped_document(
            "card",
            record_kind,
            session_id,
            question_id,
            document_id,
            card_kind,
            "",
            question_title,
            content,
            adapter_id,
            source_id,
            project_path,
            turn_id,
            part_id,
        )
    }
}

#[test]
fn in_memory_index_searches_cards_and_applies_scope_filters() {
    let index = InMemoryConversationIndex::new().expect("create conversation index");
    index
        .replace_documents(&[
            ConversationSearchDocument::scoped_document(
                "question",
                "session",
                "session-1",
                "question-1",
                "card-1",
                "",
                "",
                "Tantivy 本地搜索",
                "如何实现中文全文搜索",
                "",
                "",
                "",
                "",
                "",
            ),
            ConversationSearchDocument::scoped_document(
                "card",
                "web",
                "session-2",
                "question-2",
                "card-2",
                "answer",
                "answer",
                "Deploy pipeline",
                "Use a release pipeline with rollback support",
                "",
                "",
                "",
                "",
                "",
            ),
        ])
        .expect("index conversation cards");

    let reader = index.index.reader().expect("open diagnostic reader");
    let searcher = reader.searcher();
    assert_eq!(searcher.search(&AllQuery, &Count).expect("count cards"), 2);
    let card_filter = TermQuery::new(
        Term::from_field_text(index.fields.document_kind, "card"),
        IndexRecordOption::Basic,
    );
    assert_eq!(
        searcher
            .search(&card_filter, &Count)
            .expect("count card documents"),
        1
    );
    let chinese_term = TermQuery::new(
        Term::from_field_text(index.fields.content_zh, "全文"),
        IndexRecordOption::WithFreqsAndPositions,
    );
    assert_eq!(
        searcher
            .search(&chinese_term, &Count)
            .expect("count Chinese term"),
        1
    );
    assert!(tokens_for(&index.index, JIEBA_TOKENIZER, "全文搜索")
        .expect("tokenize Chinese query")
        .iter()
        .any(|token| token == "全文"));

    let chinese = index
        .search_cards(&ConversationCardQuery {
            query: "全文搜索".to_string(),
            record_kind: "session".to_string(),
            card_kinds: Vec::new(),
            semantic_roles: Vec::new(),
            include_questions: true,
            include_cards: false,
            limit: 20,
            offset: 0,
            adapter_id: None,
            source_id: None,
            project_path: None,
        })
        .expect("search Chinese card");
    assert_eq!(chinese.total_count, 1);
    assert_eq!(chinese.hits[0].document_id, "card-1");
    assert_eq!(chinese.hits[0].session_id, "session-1");
    assert_eq!(chinese.content_type_counts.get("question"), Some(&1));
    assert_eq!(chinese.content_type_counts.get("answer"), None);

    let partial_title = index
        .search_cards(&ConversationCardQuery {
            query: "antiv".to_string(),
            record_kind: "session".to_string(),
            card_kinds: Vec::new(),
            semantic_roles: Vec::new(),
            include_questions: true,
            include_cards: false,
            limit: 20,
            offset: 0,
            adapter_id: None,
            source_id: None,
            project_path: None,
        })
        .expect("search partial metadata ngram");
    assert_eq!(partial_title.total_count, 1);

    let filtered = index
        .search_cards(&ConversationCardQuery {
            query: "pipeline".to_string(),
            record_kind: "web".to_string(),
            card_kinds: Vec::new(),
            semantic_roles: Vec::new(),
            include_questions: true,
            include_cards: false,
            limit: 20,
            offset: 0,
            adapter_id: None,
            source_id: None,
            project_path: None,
        })
        .expect("filter web cards");
    assert_eq!(filtered.total_count, 0);
}

#[test]
fn in_memory_index_searches_cards_by_related_id_fragments() {
    let index = InMemoryConversationIndex::new().expect("create conversation index");
    let session_id = format!("conversation-session-{}", "1".repeat(64));
    let question_id = format!("conversation-question-{}", "2".repeat(64));
    let turn_id = format!("conversation-turn-{}", "3".repeat(64));
    let part_id = format!("conversation-part-{}", "4".repeat(64));
    let block_id = format!("{part_id}-answer");
    index
        .replace_documents(&[ConversationSearchDocument::scoped_card(
            "session",
            &session_id,
            &question_id,
            &block_id,
            "answer",
            "Unrelated title",
            "Content without hexadecimal identifiers",
            "codex",
            "codex-live",
            "/tmp/project",
            &turn_id,
            &part_id,
        )])
        .expect("index conversation card");

    for fragment in ["11111111", "22222222", "33333333", "44444444"] {
        let matches = index
            .search_cards(&ConversationCardQuery {
                query: fragment.to_string(),
                record_kind: "session".to_string(),
                card_kinds: Vec::new(),
                semantic_roles: Vec::new(),
                include_questions: true,
                include_cards: true,
                limit: 20,
                offset: 0,
                adapter_id: None,
                source_id: None,
                project_path: None,
            })
            .expect("search card by id fragment");
        assert_eq!(matches.total_count, 1, "fragment {fragment}");
        assert_eq!(matches.hits[0].document_id, block_id);
    }
}

#[test]
fn in_memory_index_filters_dynamic_card_kinds_and_semantic_roles() {
    let index = InMemoryConversationIndex::new().expect("create conversation index");
    index
        .replace_documents(&[
            ConversationSearchDocument::scoped_document(
                "card",
                "session",
                "session-1",
                "question-1",
                "part-1",
                "claude-code.reasoning",
                "reasoning",
                "Claude",
                "shared reasoning evidence",
                "claude-code",
                "source-1",
                "/tmp/project",
                "turn-1",
                "part-1",
            ),
            ConversationSearchDocument::scoped_document(
                "card",
                "session",
                "session-2",
                "question-2",
                "part-2",
                "codex.analysis",
                "reasoning",
                "Codex",
                "shared reasoning evidence",
                "codex",
                "source-2",
                "/tmp/project",
                "turn-2",
                "part-2",
            ),
        ])
        .expect("index dynamic cards");

    let exact = index
        .search_cards(&ConversationCardQuery {
            query: "reasoning evidence".to_string(),
            record_kind: "session".to_string(),
            card_kinds: vec!["claude-code.reasoning".to_string()],
            semantic_roles: Vec::new(),
            include_questions: false,
            include_cards: true,
            limit: 20,
            offset: 0,
            adapter_id: None,
            source_id: None,
            project_path: None,
        })
        .expect("search exact custom kind");
    assert_eq!(exact.total_count, 1);
    assert_eq!(exact.hits[0].card_type, "claude-code.reasoning");

    let semantic = index
        .search_cards(&ConversationCardQuery {
            query: "reasoning evidence".to_string(),
            record_kind: "session".to_string(),
            card_kinds: Vec::new(),
            semantic_roles: vec!["reasoning".to_string()],
            include_questions: false,
            include_cards: true,
            limit: 20,
            offset: 0,
            adapter_id: None,
            source_id: None,
            project_path: None,
        })
        .expect("search semantic role across adapters");
    assert_eq!(semantic.total_count, 2);
    assert_eq!(
        semantic.content_type_counts.get("claude-code.reasoning"),
        Some(&1)
    );
    assert_eq!(semantic.content_type_counts.get("codex.analysis"), Some(&1));
    assert_eq!(semantic.semantic_role_counts.get("reasoning"), Some(&2));
}
