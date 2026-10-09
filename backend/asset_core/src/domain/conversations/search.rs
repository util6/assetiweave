use std::collections::BTreeMap;

/// Stable identity and ranking facts for hits in a conversation search index.
///
/// Infrastructure owns the index implementation, while this value crosses
/// into Store so persisted conversation content can be hydrated without Store
/// depending on the search engine module.
#[derive(Debug, Clone)]
pub struct ConversationSearchMatch {
    pub document_id: String,
    pub session_id: String,
    pub question_id: String,
    pub card_type: String,
    pub score: usize,
    pub turn_id: String,
    pub part_id: String,
}

#[derive(Debug, Clone)]
pub struct ConversationSearchMatches {
    pub total_count: usize,
    pub hits: Vec<ConversationSearchMatch>,
    pub content_type_counts: BTreeMap<String, usize>,
    pub semantic_role_counts: BTreeMap<String, usize>,
}
