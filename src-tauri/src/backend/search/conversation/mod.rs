mod engine;
mod lifecycle;
mod schema;

pub(crate) use engine::ConversationSearchMatches;
pub(crate) use lifecycle::{rebuild_conversation_search_index, search_ready_conversation_index};
