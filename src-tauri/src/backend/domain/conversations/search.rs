use std::collections::BTreeMap;

/// Stable identity and ranking facts for hits in a conversation search index.
///
/// Infrastructure owns the index implementation, while this value crosses
/// into Store so persisted conversation content can be hydrated without Store
/// depending on the search engine module.
#[derive(Debug, Clone)]
pub(crate) struct ConversationSearchMatch {
    pub(crate) document_id: String,
    pub(crate) session_id: String,
    pub(crate) question_id: String,
    pub(crate) card_type: String,
    pub(crate) score: usize,
    pub(crate) turn_id: String,
    pub(crate) part_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ConversationSearchMatches {
    pub(crate) total_count: usize,
    pub(crate) hits: Vec<ConversationSearchMatch>,
    pub(crate) content_type_counts: BTreeMap<String, usize>,
    pub(crate) semantic_role_counts: BTreeMap<String, usize>,
}
