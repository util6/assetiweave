use super::schema::{
    build_conversation_schema, register_conversation_tokenizers, ConversationSearchSchema,
    JIEBA_TOKENIZER,
};
use crate::backend::dto::AppResult;
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
    document_type: String,
    document_id: String,
    record_kind: String,
    session_id: String,
    question_id: String,
    card_type: String,
    question_title: String,
    content: String,
    adapter_id: String,
    source_id: String,
    project_path: String,
    turn_id: String,
    part_id: String,
}

impl ConversationSearchDocument {
    #[cfg(test)]
    pub(super) fn card(
        record_kind: &str,
        session_id: &str,
        question_id: &str,
        document_id: &str,
        card_type: &str,
        question_title: &str,
        content: &str,
    ) -> Self {
        Self::scoped_card(
            record_kind,
            session_id,
            question_id,
            document_id,
            card_type,
            question_title,
            content,
            "",
            "",
            "",
            "",
            "",
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn scoped_card(
        record_kind: &str,
        session_id: &str,
        question_id: &str,
        document_id: &str,
        card_type: &str,
        question_title: &str,
        content: &str,
        adapter_id: &str,
        source_id: &str,
        project_path: &str,
        turn_id: &str,
        part_id: &str,
    ) -> Self {
        Self {
            document_type: "card".to_string(),
            document_id: document_id.to_string(),
            record_kind: record_kind.to_string(),
            session_id: session_id.to_string(),
            question_id: question_id.to_string(),
            card_type: card_type.to_string(),
            question_title: question_title.to_string(),
            content: content.to_string(),
            adapter_id: adapter_id.to_string(),
            source_id: source_id.to_string(),
            project_path: project_path.to_string(),
            turn_id: turn_id.to_string(),
            part_id: part_id.to_string(),
        }
    }
}

pub(super) struct ConversationCardQuery {
    pub(super) query: String,
    pub(super) record_kind: String,
    pub(super) card_types: Vec<String>,
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
        replace_documents(&self.index, &self.fields, documents)
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
        std::fs::create_dir_all(path).map_err(|error| error.to_string())?;
        let fields = build_conversation_schema();
        let index =
            Index::create_in_dir(path, fields.schema.clone()).map_err(|error| error.to_string())?;
        register_conversation_tokenizers(&index);
        Ok(Self { index, fields })
    }

    pub(super) fn replace_documents(
        &self,
        documents: &[ConversationSearchDocument],
    ) -> AppResult<()> {
        replace_documents(&self.index, &self.fields, documents)
    }

    pub(super) fn open(path: &Path) -> AppResult<Self> {
        let fields = build_conversation_schema();
        let index = Index::open_in_dir(path).map_err(|error| error.to_string())?;
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

fn replace_documents(
    index: &Index,
    fields: &ConversationSearchSchema,
    documents: &[ConversationSearchDocument],
) -> AppResult<()> {
    let mut writer = index
        .writer(50_000_000)
        .map_err(|error| error.to_string())?;
    writer
        .delete_all_documents()
        .map_err(|error| error.to_string())?;
    for item in documents {
        writer
            .add_document(doc!(
                    fields.document_type => item.document_type.as_str(),
                    fields.document_id => item.document_id.as_str(),
                    fields.record_kind => item.record_kind.as_str(),
                    fields.session_id => item.session_id.as_str(),
                    fields.question_id => item.question_id.as_str(),
                    fields.turn_id => item.turn_id.as_str(),
                    fields.part_id => item.part_id.as_str(),
                    fields.block_id => item.document_id.as_str(),
                    fields.card_type => item.card_type.as_str(),
                    fields.adapter_id => item.adapter_id.as_str(),
                    fields.source_id => item.source_id.as_str(),
                    fields.project_path => item.project_path.as_str(),
                    fields.question_title_zh => item.question_title.as_str(),
                    fields.question_title_en => item.question_title.as_str(),
                    fields.question_title_ngram => item.question_title.as_str(),
                    fields.content_zh => item.content.as_str(),
                    fields.content_en => item.content.as_str(),
            ))
            .map_err(|error| error.to_string())?;
    }
    writer.commit().map_err(|error| error.to_string())?;
    Ok(())
}

fn search_cards(
    index: &Index,
    fields: &ConversationSearchSchema,
    request: &ConversationCardQuery,
) -> AppResult<ConversationSearchMatches> {
    let query = request.query.trim();
    if query.is_empty() {
        return Err("conversation search query is required".to_string());
    }
    let reader = index.reader().map_err(|error| error.to_string())?;
    let searcher = reader.searcher();
    let query = build_card_query(index, fields, query, request)?;
    let total_count = searcher
        .search(&query, &Count)
        .map_err(|error| error.to_string())?;
    let top_docs = searcher
        .search(
            &query,
            &TopDocs::with_limit(request.limit)
                .and_offset(request.offset)
                .order_by_score(),
        )
        .map_err(|error| error.to_string())?;
    let mut hits = Vec::with_capacity(top_docs.len());
    for (score, address) in top_docs {
        let document = searcher
            .doc::<TantivyDocument>(address)
            .map_err(|error| error.to_string())?;
        hits.push(ConversationSearchMatch {
            document_id: stored_text(&document, fields.document_id)?,
            session_id: stored_text(&document, fields.session_id)?,
            question_id: stored_text(&document, fields.question_id)?,
            card_type: stored_text(&document, fields.card_type)?,
            score: score_to_integer(score),
            turn_id: stored_text(&document, fields.turn_id)?,
            part_id: stored_text(&document, fields.part_id)?,
        });
    }
    Ok(ConversationSearchMatches { total_count, hits })
}

fn build_card_query(
    index: &Index,
    fields: &ConversationSearchSchema,
    query: &str,
    request: &ConversationCardQuery,
) -> AppResult<BooleanQuery> {
    let mut clauses: Vec<(Occur, Box<dyn Query>)> = vec![
        exact_clause(fields.document_type, "card"),
        exact_clause(fields.record_kind, &request.record_kind),
    ];
    if !request.card_types.is_empty() {
        clauses.push((
            Occur::Must,
            Box::new(BooleanQuery::new(
                request
                    .card_types
                    .iter()
                    .map(|card_type| exact_should_clause(fields.card_type, card_type))
                    .collect(),
            )),
        ));
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
        return Err("conversation search query has no searchable terms".to_string());
    }
    clauses.push((Occur::Must, Box::new(BooleanQuery::new(lexical_branches))));
    let normalized = query.trim().to_lowercase();
    if (2..=15).contains(&normalized.chars().count()) {
        clauses.push(text_should_clause(
            fields.question_title_ngram,
            &normalized,
            0.6,
        ));
    }
    Ok(BooleanQuery::new(clauses))
}

fn tokens_for(index: &Index, tokenizer_name: &str, query: &str) -> AppResult<Vec<String>> {
    let mut tokens = BTreeSet::new();
    let mut analyzer = index
        .tokenizers()
        .get(tokenizer_name)
        .ok_or_else(|| format!("missing conversation tokenizer: {tokenizer_name}"))?;
    let mut stream = analyzer.token_stream(query);
    while stream.advance() {
        let text = stream.token().text.trim().to_lowercase();
        if !text.is_empty() {
            tokens.insert(text);
        }
    }
    Ok(tokens.into_iter().collect())
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
        .ok_or_else(|| "conversation search index document is missing a stored field".to_string())
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
                ConversationSearchDocument::card(
                    "session",
                    "session-1",
                    "question-1",
                    "card-1",
                    "question",
                    "Tantivy 本地搜索",
                    "如何实现中文全文搜索",
                ),
                ConversationSearchDocument::card(
                    "web",
                    "session-2",
                    "question-2",
                    "card-2",
                    "answer",
                    "Deploy pipeline",
                    "Use a release pipeline with rollback support",
                ),
            ])
            .expect("index conversation cards");

        let reader = index.index.reader().expect("open diagnostic reader");
        let searcher = reader.searcher();
        assert_eq!(searcher.search(&AllQuery, &Count).expect("count cards"), 2);
        let card_filter = TermQuery::new(
            Term::from_field_text(index.fields.document_type, "card"),
            IndexRecordOption::Basic,
        );
        assert_eq!(
            searcher
                .search(&card_filter, &Count)
                .expect("count card documents"),
            2
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
                card_types: vec!["question".to_string()],
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

        let filtered = index
            .search_cards(&ConversationCardQuery {
                query: "pipeline".to_string(),
                record_kind: "web".to_string(),
                card_types: vec!["question".to_string()],
                limit: 20,
                offset: 0,
                adapter_id: None,
                source_id: None,
                project_path: None,
            })
            .expect("filter web cards");
        assert_eq!(filtered.total_count, 0);
    }
}
