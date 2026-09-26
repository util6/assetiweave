use std::collections::BTreeSet;

use tantivy::{
    doc,
    query::{BooleanQuery, BoostQuery, Occur, Query, TermQuery},
    schema::{IndexRecordOption, TantivyDocument, Value},
    tokenizer::TokenStream,
    Index, Term,
};

use super::engine::ConversationCardQuery;
use super::schema::{ConversationSearchSchema, JIEBA_TOKENIZER};
use crate::backend::domain::{conversation_id_fragment, conversation_id_search_term};
use crate::backend::infrastructure::{InfraError, InfraResult};

pub(super) fn build_card_query(
    index: &Index,
    fields: &ConversationSearchSchema,
    query: &str,
    request: &ConversationCardQuery,
) -> InfraResult<BooleanQuery> {
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
        return Err(InfraError::Validation(
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

pub(crate) fn tokens_for(
    index: &Index,
    tokenizer_name: &str,
    query: &str,
) -> InfraResult<Vec<String>> {
    let mut tokens = BTreeSet::new();
    let mut analyzer = index.tokenizers().get(tokenizer_name).ok_or_else(|| {
        InfraError::External(format!("missing conversation tokenizer: {tokenizer_name}"))
    })?;
    let mut stream = analyzer.token_stream(query);
    while stream.advance() {
        let text = stream.token().text.trim().to_lowercase();
        if !text.is_empty() && text.chars().any(|c| c.is_alphanumeric()) {
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

pub(super) fn stored_text(
    document: &TantivyDocument,
    field: tantivy::schema::Field,
) -> InfraResult<String> {
    document
        .get_first(field)
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            InfraError::External(
                "conversation search index document is missing a stored field".to_string(),
            )
        })
}

pub(super) fn score_to_integer(score: f32) -> usize {
    (score.max(0.0) * 1_000.0).round().max(1.0) as usize
}
