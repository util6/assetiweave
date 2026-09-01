use super::schema::{
    build_conversation_schema, register_conversation_tokenizers, ConversationSearchSchema,
    JIEBA_TOKENIZER,
};
use crate::backend::models::{conversation_id_fragment, conversation_id_search_term};
use crate::backend::runtime::{AppError, AppResult};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use tantivy::{
    collector::{Count, TopDocs},
    doc,
    query::{BooleanQuery, BoostQuery, Occur, Query, TermQuery},
    schema::{IndexRecordOption, TantivyDocument, Value},
    tokenizer::TokenStream,
    Index, Term,
};

pub(super) struct ConversationSearchDocument {
    document_kind: String,
    document_id: String,
    record_kind: String,
    session_id: String,
    question_id: String,
    card_kind: String,
    semantic_role: String,
    question_title: String,
    content: String,
    adapter_id: String,
    source_id: String,
    project_path: String,
    turn_id: String,
    part_id: String,
    id_fragments: BTreeSet<String>,
}

impl ConversationSearchDocument {
    #[allow(clippy::too_many_arguments)]
    #[cfg(test)]
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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn scoped_document(
        document_kind: &str,
        record_kind: &str,
        session_id: &str,
        question_id: &str,
        document_id: &str,
        card_kind: &str,
        semantic_role: &str,
        question_title: &str,
        content: &str,
        adapter_id: &str,
        source_id: &str,
        project_path: &str,
        turn_id: &str,
        part_id: &str,
    ) -> Self {
        let id_fragments = [session_id, question_id, turn_id, part_id, document_id]
            .into_iter()
            .map(conversation_id_fragment)
            .filter(|fragment| conversation_id_search_term(fragment).is_some())
            .collect();
        Self {
            document_kind: document_kind.to_string(),
            document_id: document_id.to_string(),
            record_kind: record_kind.to_string(),
            session_id: session_id.to_string(),
            question_id: question_id.to_string(),
            card_kind: card_kind.to_string(),
            semantic_role: semantic_role.to_string(),
            question_title: question_title.to_string(),
            content: content.to_string(),
            adapter_id: adapter_id.to_string(),
            source_id: source_id.to_string(),
            project_path: project_path.to_string(),
            turn_id: turn_id.to_string(),
            part_id: part_id.to_string(),
            id_fragments,
        }
    }
}

#[derive(Clone)]
pub(super) struct ConversationCardQuery {
    pub(super) query: String,
    pub(super) record_kind: String,
    pub(super) card_kinds: Vec<String>,
    pub(super) semantic_roles: Vec<String>,
    pub(super) include_questions: bool,
    pub(super) include_cards: bool,
    pub(super) limit: usize,
    pub(super) offset: usize,
    pub(super) adapter_id: Option<String>,
    pub(super) source_id: Option<String>,
    pub(super) project_path: Option<String>,
}

pub(crate) struct ConversationSearchMatch {
    pub(crate) document_id: String,
    pub(crate) session_id: String,
    pub(crate) question_id: String,
    pub(crate) card_type: String,
    pub(crate) score: usize,
    pub(crate) turn_id: String,
    pub(crate) part_id: String,
}

pub(crate) struct ConversationSearchMatches {
    pub(crate) total_count: usize,
    pub(crate) hits: Vec<ConversationSearchMatch>,
    pub(crate) content_type_counts: BTreeMap<String, usize>,
    pub(crate) semantic_role_counts: BTreeMap<String, usize>,
}

#[cfg(test)]
pub(super) struct InMemoryConversationIndex {
    index: Index,
    fields: ConversationSearchSchema,
}

#[cfg(test)]
impl InMemoryConversationIndex {
    pub(super) fn new() -> AppResult<Self> {
        let fields = build_conversation_schema();
        let index = Index::create_in_ram(fields.schema.clone());
        register_conversation_tokenizers(&index);
        Ok(Self { index, fields })
    }

    pub(super) fn replace_documents(
        &self,
        documents: &[ConversationSearchDocument],
    ) -> AppResult<()> {
        replace_documents_with_checkpoint(&self.index, &self.fields, documents, &mut || Ok(()))
    }

    pub(super) fn replace_documents_with_checkpoint<F>(
        &self,
        documents: &[ConversationSearchDocument],
        checkpoint: &mut F,
    ) -> AppResult<()>
    where
        F: FnMut() -> AppResult<()>,
    {
        replace_documents_with_checkpoint(&self.index, &self.fields, documents, checkpoint)
    }

    pub(super) fn search_cards(
        &self,
        request: &ConversationCardQuery,
    ) -> AppResult<ConversationSearchMatches> {
        search_cards(&self.index, &self.fields, request)
    }
}

pub(super) struct DiskConversationIndex {
    index: Index,
    fields: ConversationSearchSchema,
}

impl DiskConversationIndex {
    pub(super) fn create(path: &Path) -> AppResult<Self> {
        std::fs::create_dir_all(path).map_err(AppError::external)?;
        let fields = build_conversation_schema();
        let index =
            Index::create_in_dir(path, fields.schema.clone()).map_err(AppError::external)?;
        register_conversation_tokenizers(&index);
        Ok(Self { index, fields })
    }

    pub(super) fn replace_documents(
        &self,
        documents: &[ConversationSearchDocument],
    ) -> AppResult<()> {
        replace_documents_with_checkpoint(&self.index, &self.fields, documents, &mut || Ok(()))
    }

    pub(super) fn replace_documents_with_checkpoint<F>(
        &self,
        documents: &[ConversationSearchDocument],
        checkpoint: &mut F,
    ) -> AppResult<()>
    where
        F: FnMut() -> AppResult<()>,
    {
        replace_documents_with_checkpoint(&self.index, &self.fields, documents, checkpoint)
    }

    pub(super) fn open(path: &Path) -> AppResult<Self> {
        let fields = build_conversation_schema();
        let index = Index::open_in_dir(path).map_err(AppError::external)?;
        register_conversation_tokenizers(&index);
        Ok(Self { index, fields })
    }

    pub(super) fn search_cards(
        &self,
        request: &ConversationCardQuery,
    ) -> AppResult<ConversationSearchMatches> {
        search_cards(&self.index, &self.fields, request)
    }
}

fn replace_documents_with_checkpoint<F>(
    index: &Index,
    fields: &ConversationSearchSchema,
    documents: &[ConversationSearchDocument],
    checkpoint: &mut F,
) -> AppResult<()>
where
    F: FnMut() -> AppResult<()>,
{
    let mut writer = index.writer(50_000_000).map_err(AppError::external)?;
    checkpoint()?;
    writer.delete_all_documents().map_err(AppError::external)?;
    for item in documents {
        checkpoint()?;
        let mut document = doc!(
                fields.document_kind => item.document_kind.as_str(),
                fields.document_id => item.document_id.as_str(),
                fields.record_kind => item.record_kind.as_str(),
                fields.session_id => item.session_id.as_str(),
                fields.question_id => item.question_id.as_str(),
                fields.turn_id => item.turn_id.as_str(),
                fields.part_id => item.part_id.as_str(),
                fields.block_id => item.document_id.as_str(),
                fields.card_kind => item.card_kind.as_str(),
                fields.semantic_role => item.semantic_role.as_str(),
                fields.adapter_id => item.adapter_id.as_str(),
                fields.source_id => item.source_id.as_str(),
                fields.project_path => item.project_path.as_str(),
                fields.question_title_zh => item.question_title.as_str(),
                fields.question_title_en => item.question_title.as_str(),
                fields.question_title_ngram => item.question_title.as_str(),
                fields.content_zh => item.content.as_str(),
                fields.content_en => item.content.as_str(),
        );
        for fragment in &item.id_fragments {
            document.add_text(fields.id_fragment, fragment);
        }
        writer.add_document(document).map_err(AppError::external)?;
    }
    checkpoint()?;
    writer.commit().map_err(AppError::external)?;
    Ok(())
}

fn search_cards(
    index: &Index,
    fields: &ConversationSearchSchema,
    request: &ConversationCardQuery,
) -> AppResult<ConversationSearchMatches> {
    let query = request.query.trim();
    if query.is_empty() {
        return Err(AppError::Validation(
            "conversation search query is required".to_string(),
        ));
    }
    if query.chars().count() > 512 {
        return Err(AppError::Validation(
            "conversation search query must not exceed 512 characters".to_string(),
        ));
    }
    let reader = index.reader().map_err(AppError::external)?;
    let searcher = reader.searcher();
    let mut content_type_counts = BTreeMap::new();
    let mut semantic_role_counts = BTreeMap::new();
    let mut facet_request = request.clone();
    facet_request.card_kinds.clear();
    facet_request.semantic_roles.clear();
    facet_request.include_questions = true;
    facet_request.include_cards = true;
    let facet_query = build_card_query(index, fields, query, &facet_request)?;
    let facet_docs = searcher
        .search(
            &facet_query,
            &TopDocs::with_limit(searcher.num_docs() as usize).order_by_score(),
        )
        .map_err(AppError::external)?;
    for (_, address) in facet_docs {
        let document = searcher
            .doc::<TantivyDocument>(address)
            .map_err(AppError::external)?;
        let document_kind = stored_text(&document, fields.document_kind)?;
        if document_kind == "question" {
            *content_type_counts
                .entry("question".to_string())
                .or_default() += 1;
            continue;
        }
        let card_kind = stored_text(&document, fields.card_kind)?;
        *content_type_counts.entry(card_kind).or_default() += 1;
        let semantic_role = stored_text(&document, fields.semantic_role)?;
        if !semantic_role.is_empty() {
            *semantic_role_counts.entry(semantic_role).or_default() += 1;
        }
    }
    let query = build_card_query(index, fields, query, request)?;
    let total_count = searcher
        .search(&query, &Count)
        .map_err(AppError::external)?;
    let top_docs = searcher
        .search(
            &query,
            &TopDocs::with_limit(request.limit)
                .and_offset(request.offset)
                .order_by_score(),
        )
        .map_err(AppError::external)?;
    let mut hits = Vec::with_capacity(top_docs.len());
    for (score, address) in top_docs {
        let document = searcher
            .doc::<TantivyDocument>(address)
            .map_err(AppError::external)?;
        hits.push(ConversationSearchMatch {
            document_id: stored_text(&document, fields.document_id)?,
            session_id: stored_text(&document, fields.session_id)?,
            question_id: stored_text(&document, fields.question_id)?,
            card_type: if stored_text(&document, fields.document_kind)? == "question" {
                "question".to_string()
            } else {
                stored_text(&document, fields.card_kind)?
            },
            score: score_to_integer(score),
            turn_id: stored_text(&document, fields.turn_id)?,
            part_id: stored_text(&document, fields.part_id)?,
        });
    }
    Ok(ConversationSearchMatches {
        total_count,
        hits,
        content_type_counts,
        semantic_role_counts,
    })
}

fn build_card_query(
    index: &Index,
    fields: &ConversationSearchSchema,
    query: &str,
    request: &ConversationCardQuery,
) -> AppResult<BooleanQuery> {
    let mut clauses: Vec<(Occur, Box<dyn Query>)> =
        vec![exact_clause(fields.record_kind, &request.record_kind)];
    let has_card_filters = !request.card_kinds.is_empty() || !request.semantic_roles.is_empty();
    if !request.include_cards {
        clauses.push(exact_clause(fields.document_kind, "question"));
    } else if has_card_filters {
        let mut card_clauses = vec![exact_clause(fields.document_kind, "card")];
        if !request.card_kinds.is_empty() {
            card_clauses.push((
                Occur::Must,
                Box::new(BooleanQuery::new(
                    request
                        .card_kinds
                        .iter()
                        .map(|kind| exact_should_clause(fields.card_kind, kind))
                        .collect(),
                )),
            ));
        }
        if !request.semantic_roles.is_empty() {
            card_clauses.push((
                Occur::Must,
                Box::new(BooleanQuery::new(
                    request
                        .semantic_roles
                        .iter()
                        .map(|role| exact_should_clause(fields.semantic_role, role))
                        .collect(),
                )),
            ));
        }
        let card_query = Box::new(BooleanQuery::new(card_clauses)) as Box<dyn Query>;
        if request.include_questions {
            clauses.push((
                Occur::Must,
                Box::new(BooleanQuery::new(vec![
                    exact_should_clause(fields.document_kind, "question"),
                    (Occur::Should, card_query),
                ])),
            ));
        } else {
            clauses.push((Occur::Must, card_query));
        }
    } else if !request.include_questions {
        clauses.push(exact_clause(fields.document_kind, "card"));
    }
    if let Some(adapter_id) = request.adapter_id.as_deref() {
        clauses.push(exact_clause(fields.adapter_id, adapter_id));
    }
    if let Some(source_id) = request.source_id.as_deref() {
        clauses.push(exact_clause(fields.source_id, source_id));
    }
    if let Some(project_path) = request.project_path.as_deref() {
        clauses.push(exact_clause(fields.project_path, project_path));
    }

    if let Some(id_fragment) =
        conversation_id_search_term(query).map(|value| conversation_id_fragment(&value))
    {
        clauses.push((
            Occur::Must,
            Box::new(TermQuery::new(
                Term::from_field_text(fields.id_fragment, &id_fragment),
                IndexRecordOption::Basic,
            )),
        ));
        return Ok(BooleanQuery::new(clauses));
    }

    let jieba_tokens = tokens_for(index, JIEBA_TOKENIZER, query)?;
    let default_tokens = tokens_for(index, "default", query)?;
    let mut lexical_branches = Vec::new();
    if !jieba_tokens.is_empty() {
        lexical_branches.push((
            Occur::Should,
            Box::new(text_branch(
                &jieba_tokens,
                fields.content_zh,
                fields.question_title_zh,
            )) as Box<dyn Query>,
        ));
    }
    if !default_tokens.is_empty() {
        lexical_branches.push((
            Occur::Should,
            Box::new(text_branch(
                &default_tokens,
                fields.content_en,
                fields.question_title_en,
            )) as Box<dyn Query>,
        ));
    }
    if lexical_branches.is_empty() {
        return Err(AppError::Validation(
            "conversation search query has no searchable terms".to_string(),
        ));
    }
    let normalized = query.trim().to_lowercase();
    if (2..=15).contains(&normalized.chars().count()) {
        lexical_branches.push(text_should_clause(
            fields.question_title_ngram,
            &normalized,
            0.6,
        ));
    }
    clauses.push((Occur::Must, Box::new(BooleanQuery::new(lexical_branches))));
    Ok(BooleanQuery::new(clauses))
}

fn tokens_for(index: &Index, tokenizer_name: &str, query: &str) -> AppResult<Vec<String>> {
    let mut tokens = BTreeSet::new();
    let mut analyzer = index.tokenizers().get(tokenizer_name).ok_or_else(|| {
        AppError::External(format!("missing conversation tokenizer: {tokenizer_name}"))
    })?;
    let mut stream = analyzer.token_stream(query);
    while stream.advance() {
        let text = stream.token().text.trim().to_lowercase();
        if !text.is_empty() {
            tokens.insert(text);
        }
    }
    Ok(tokens.into_iter().take(32).collect())
}

fn exact_clause(field: tantivy::schema::Field, value: &str) -> (Occur, Box<dyn Query>) {
    (
        Occur::Must,
        Box::new(TermQuery::new(
            Term::from_field_text(field, value),
            IndexRecordOption::Basic,
        )),
    )
}

fn exact_should_clause(field: tantivy::schema::Field, value: &str) -> (Occur, Box<dyn Query>) {
    let (_, query) = exact_clause(field, value);
    (Occur::Should, query)
}

fn text_should_clause(
    field: tantivy::schema::Field,
    value: &str,
    boost: f32,
) -> (Occur, Box<dyn Query>) {
    let query = TermQuery::new(
        Term::from_field_text(field, value),
        IndexRecordOption::WithFreqsAndPositions,
    );
    (
        Occur::Should,
        Box::new(BoostQuery::new(Box::new(query), boost)),
    )
}

fn text_branch(
    tokens: &[String],
    content_field: tantivy::schema::Field,
    title_field: tantivy::schema::Field,
) -> BooleanQuery {
    BooleanQuery::new(
        tokens
            .iter()
            .map(|token| {
                (
                    Occur::Must,
                    Box::new(BooleanQuery::new(vec![
                        text_should_clause(content_field, token, 3.0),
                        text_should_clause(title_field, token, 3.5),
                    ])) as Box<dyn Query>,
                )
            })
            .collect(),
    )
}

fn stored_text(document: &TantivyDocument, field: tantivy::schema::Field) -> AppResult<String> {
    document
        .get_first(field)
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            AppError::External(
                "conversation search index document is missing a stored field".to_string(),
            )
        })
}

fn score_to_integer(score: f32) -> usize {
    (score.max(0.0) * 1_000.0).round().max(1.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use tantivy::query::AllQuery;

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
}
