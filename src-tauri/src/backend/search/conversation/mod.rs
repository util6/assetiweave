mod engine;
mod lifecycle;
mod schema;

pub(crate) use engine::ConversationSearchMatches;
pub(crate) use lifecycle::{
    rebuild_conversation_search_index, rebuild_conversation_search_index_with_cancellation,
    rebuild_conversation_search_index_with_offset, search_ready_conversation_index,
};
