use super::query_builder::*;
use super::schema::{
    build_conversation_schema, register_conversation_tokenizers, ConversationSearchSchema,
    JIEBA_TOKENIZER,
};
use crate::backend::domain::{
    conversation_id_fragment, conversation_id_search_term, ConversationSearchMatch,
    ConversationSearchMatches,
};
use crate::backend::infrastructure::{InfraError, InfraResult};
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

pub(crate) struct ConversationSearchDocument {
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
    pub(crate) fn scoped_document(
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
pub(crate) struct ConversationCardQuery {
    pub(crate) query: String,
    pub(crate) record_kind: String,
    pub(crate) card_kinds: Vec<String>,
    pub(crate) semantic_roles: Vec<String>,
    pub(crate) include_questions: bool,
    pub(crate) include_cards: bool,
    pub(crate) limit: usize,
    pub(crate) offset: usize,
    pub(crate) adapter_id: Option<String>,
    pub(crate) source_id: Option<String>,
    pub(crate) project_path: Option<String>,
}

#[cfg(test)]
pub(super) struct InMemoryConversationIndex {
    index: Index,
    fields: ConversationSearchSchema,
}

#[cfg(test)]
impl InMemoryConversationIndex {
    pub(super) fn new() -> InfraResult<Self> {
        let fields = build_conversation_schema();
        let index = Index::create_in_ram(fields.schema.clone());
        register_conversation_tokenizers(&index);
        Ok(Self { index, fields })
    }

    pub(super) fn replace_documents(
        &self,
        documents: &[ConversationSearchDocument],
    ) -> InfraResult<()> {
        replace_documents_with_checkpoint(&self.index, &self.fields, documents, &mut || Ok(()))
    }

    pub(super) fn replace_documents_with_checkpoint<F>(
        &self,
        documents: &[ConversationSearchDocument],
        checkpoint: &mut F,
    ) -> InfraResult<()>
    where
        F: FnMut() -> InfraResult<()>,
    {
        replace_documents_with_checkpoint(&self.index, &self.fields, documents, checkpoint)
    }

    pub(super) fn search_cards(
        &self,
        request: &ConversationCardQuery,
    ) -> InfraResult<ConversationSearchMatches> {
        search_cards(&self.index, &self.fields, request)
    }
}

pub(super) struct DiskConversationIndex {
    index: Index,
    fields: ConversationSearchSchema,
}

impl DiskConversationIndex {
    pub(super) fn create(path: &Path) -> InfraResult<Self> {
        std::fs::create_dir_all(path).map_err(InfraError::external)?;
        let fields = build_conversation_schema();
        let index =
            Index::create_in_dir(path, fields.schema.clone()).map_err(InfraError::external)?;
        register_conversation_tokenizers(&index);
        Ok(Self { index, fields })
    }

    pub(super) fn replace_documents(
        &self,
        documents: &[ConversationSearchDocument],
    ) -> InfraResult<()> {
        replace_documents_with_checkpoint(&self.index, &self.fields, documents, &mut || Ok(()))
    }

    pub(super) fn replace_documents_with_checkpoint<F>(
        &self,
        documents: &[ConversationSearchDocument],
        checkpoint: &mut F,
    ) -> InfraResult<()>
    where
        F: FnMut() -> InfraResult<()>,
    {
        replace_documents_with_checkpoint(&self.index, &self.fields, documents, checkpoint)
    }

    pub(super) fn open(path: &Path) -> InfraResult<Self> {
        let fields = build_conversation_schema();
        let index = Index::open_in_dir(path).map_err(InfraError::external)?;
        register_conversation_tokenizers(&index);
        Ok(Self { index, fields })
    }

    pub(super) fn search_cards(
        &self,
        request: &ConversationCardQuery,
    ) -> InfraResult<ConversationSearchMatches> {
        search_cards(&self.index, &self.fields, request)
    }
}

fn replace_documents_with_checkpoint<F>(
    index: &Index,
    fields: &ConversationSearchSchema,
    documents: &[ConversationSearchDocument],
    checkpoint: &mut F,
) -> InfraResult<()>
where
    F: FnMut() -> InfraResult<()>,
{
    let mut writer = index.writer(50_000_000).map_err(InfraError::external)?;
    checkpoint()?;
    writer
        .delete_all_documents()
        .map_err(InfraError::external)?;
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
        writer
            .add_document(document)
            .map_err(InfraError::external)?;
    }
    checkpoint()?;
    writer.commit().map_err(InfraError::external)?;
    Ok(())
}

fn search_cards(
    index: &Index,
    fields: &ConversationSearchSchema,
    request: &ConversationCardQuery,
) -> InfraResult<ConversationSearchMatches> {
    let query = request.query.trim();
    if query.is_empty() {
        return Err(InfraError::Validation(
            "conversation search query is required".to_string(),
        ));
    }
    if query.chars().count() > 512 {
        return Err(InfraError::Validation(
            "conversation search query must not exceed 512 characters".to_string(),
        ));
    }
    let query_clause = build_card_query(index, fields, query, request)?;
    let reader = index.reader().map_err(InfraError::external)?;
    let searcher = reader.searcher();
    if searcher.num_docs() == 0 {
        return Ok(ConversationSearchMatches {
            total_count: 0,
            hits: Vec::new(),
            content_type_counts: BTreeMap::new(),
            semantic_role_counts: BTreeMap::new(),
        });
    }
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
        .map_err(InfraError::external)?;
    for (_, address) in facet_docs {
        let document = searcher
            .doc::<TantivyDocument>(address)
            .map_err(InfraError::external)?;
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
    let total_count = searcher
        .search(&query_clause, &Count)
        .map_err(InfraError::external)?;
    let top_docs = searcher
        .search(
            &query_clause,
            &TopDocs::with_limit(request.limit)
                .and_offset(request.offset)
                .order_by_score(),
        )
        .map_err(InfraError::external)?;
    let mut hits = Vec::with_capacity(top_docs.len());
    for (score, address) in top_docs {
        let document = searcher
            .doc::<TantivyDocument>(address)
            .map_err(InfraError::external)?;
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

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
