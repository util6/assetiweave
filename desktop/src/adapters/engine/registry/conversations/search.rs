//! Engine 命令注册表：Conversations / Search

use super::super::dispatch::*;
use super::super::types::*;
use crate::backend::application::AppService;
use crate::{command, param};
use serde_json::{json, Value};

pub(super) const COMMANDS: &[CommandSpec] = &[
    command!(
        "conversation.search",
        "conversation.search",
        "Search conversation content cards",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationSearchParams,
        ServiceAsync => |service, params| service.search_conversation_records(params).await,
        &[
            param!("record_kind", "Conversation record kind", ["recordKind"]),
            param!("adapter_id", "Optional adapter filter", ["adapterId"]),
            param!("source_id", "Optional source filter", ["sourceId"]),
            param!("project_path", "Optional project path filter", ["projectPath"]),
            param!("query", "Search query"),
            param!("content_types", "Content card types", ["contentTypes"]),
            param!("since", "Only include sessions on or after this RFC3339 timestamp or YYYY-MM-DD date"),
            param!("until", "Only include sessions on or before this RFC3339 timestamp or YYYY-MM-DD date"),
            param!("timeline", "Return hits in chronological session order"),
            param!("limit", "Maximum number of hits"),
            param!("offset", "Pagination offset"),
            param!("search_options", "Optional retrieval settings", ["searchOptions"]),
        ],
        Some("assetiweave-cli conversation search --query <query>")
    ),
    command!(
        "conversation.search.incremental",
        "conversation.search.incremental",
        "Search content cards changed by the most recent incremental conversation sync runs",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationIncrementalSearchParams,
        ServiceAsync => |service, params| service.search_recent_incremental_conversation_records(params).await,
        &[
            param!("record_kind", "Conversation record kind", ["recordKind"]),
            param!("adapter_id", "Optional adapter filter", ["adapterId"]),
            param!("source_id", "Optional source filter", ["sourceId"]),
            param!("project_path", "Optional project path filter", ["projectPath"]),
            param!("query", "Search query"),
            param!("content_types", "Content card types", ["contentTypes"]),
            param!("recent_runs", "Most recent delta-bearing sync runs to inspect", ["recentRuns"]),
            param!("limit", "Maximum number of hits"),
            param!("offset", "Pagination offset"),
            param!("search_options", "Optional retrieval settings", ["searchOptions"]),
        ],
        Some("assetiweave-cli conversation search incremental --query <query>")
    ),
    command!(
        "conversation.search.index.status",
        "conversation.search.index.status",
        "Get the local conversation search index status",
        Read,
        Friendly,
        false,
        NoParams,
        ServiceAsync => |service, _params| service.get_conversation_search_index_status().await,
        &[],
        Some("assetiweave-cli conversation search index status")
    ),
    command!(
        "conversation.search.index.rebuild",
        "conversation.search.index.rebuild",
        "Rebuild the derived local conversation search index",
        Write,
        Friendly,
        false,
        NoParams,
        ServiceAsync => |service, _params| service.rebuild_conversation_search_index().await,
        &[],
        Some("assetiweave-cli conversation search index rebuild")
    ),
    command!(
        "conversation.part.translation.update",
        "conversation.part.translation.update",
        "Overwrite the stored translation for a conversation content part",
        Write,
        Friendly,
        false,
        crate::backend::application::ConversationPartTranslationUpdateParams,
        ServiceAsync => |service, params| service.update_conversation_part_translation(params).await,
        &[
            param!("record_kind", "Conversation record table family", ["recordKind"]),
            param!("part_id", "Conversation part identifier", ["partId"]),
            param!("translated_text", "Translated content to store", ["translatedText"]),
        ],
        Some("assetiweave-cli conversation part translation update <part-id> --text <text>")
    ),
    command!(
        "conversation.card.translation.run",
        "conversation.card.translation.run",
        "Translate a conversation content card with the configured translation provider",
        Write,
        Friendly,
        false,
        crate::backend::application::conversations::card_translation::ConversationTranslationRequest,
        ServiceAsync => |service, params| service.translate_conversation_card(params).await,
        &[
            param!("provider", "Translation provider family"),
            param!("cli", "CLI translator when provider is cli"),
            param!("model", "Optional model identifier"),
            param!("prompt", "Rendered translation prompt")
        ],
        None
    ),
    command!(
        "check_opencode_translation_availability",
        "conversation.card.translation.opencode-status",
        "Check whether opencode is available for content card translation",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.check_opencode_translation_availability(),
        &[],
        None
    ),
    command!(
        "translate_conversation_card_with_opencode",
        "conversation.card.translation.opencode-run",
        "Translate a conversation content card with opencode",
        Write,
        App,
        false,
        crate::backend::application::conversations::card_translation::OpencodeTranslationRequest,
        ServiceAsync => |service, params| service.translate_conversation_card_with_opencode(params).await,
        &[param!("prompt", "Rendered translation prompt passed to the OpenCode ACP agent")],
        None
    ),
    command!(
        "test_conversation_translation_connection",
        "conversation.card.translation.connection-test",
        "Run a lightweight translation prompt to test provider connectivity",
        Read,
        App,
        false,
        crate::backend::application::conversations::card_translation::ConversationTranslationConnectionRequest,
        ServiceAsync => |service, params| service.test_conversation_translation_connection(params).await,
        &[
            param!("provider", "Translation provider family"),
            param!("cli", "CLI translator when provider is cli"),
            param!("model", "Optional model identifier"),
            param!("prompt", "Connection test prompt")
        ],
        None
    ),
    command!(
        "list_conversation_translation_models",
        "conversation.card.translation.model-list",
        "List available translation models for the selected provider",
        Read,
        App,
        false,
        crate::backend::application::conversations::card_translation::ConversationTranslationModelsRequest,
        Service => |service, params| service.list_conversation_translation_models(params),
        &[
            param!("provider", "Translation provider family"),
            param!("cli", "CLI translator when provider is cli")
        ],
        None
    ),
    command!(
        "search_conversation_records",
        "conversation.search",
        "Search conversation content cards",
        Read,
        App,
        false,
        crate::backend::application::ConversationSearchParams,
        ServiceAsync => |service, params| service.search_conversation_records(params).await,
        &[
            param!("record_kind", "Conversation record kind", ["recordKind"]),
            param!("adapter_id", "Optional adapter filter", ["adapterId"]),
            param!("source_id", "Optional source filter", ["sourceId"]),
            param!("project_path", "Optional project path filter", ["projectPath"]),
            param!("query", "Search query"),
            param!("content_types", "Content card types", ["contentTypes"]),
            param!("since", "Only include sessions on or after this RFC3339 timestamp or YYYY-MM-DD date"),
            param!("until", "Only include sessions on or before this RFC3339 timestamp or YYYY-MM-DD date"),
            param!("timeline", "Return hits in chronological session order"),
            param!("limit", "Maximum number of hits"),
            param!("offset", "Pagination offset"),
            param!("search_options", "Optional retrieval settings", ["searchOptions"]),
        ],
        None
    ),
    command!(
        "search_recent_incremental_conversation_records",
        "conversation.search.incremental",
        "Search content cards changed by the most recent incremental conversation sync runs",
        Read,
        App,
        false,
        crate::backend::application::ConversationIncrementalSearchParams,
        ServiceAsync => |service, params| service.search_recent_incremental_conversation_records(params).await,
        &[
            param!("record_kind", "Conversation record kind", ["recordKind"]),
            param!("adapter_id", "Optional adapter filter", ["adapterId"]),
            param!("source_id", "Optional source filter", ["sourceId"]),
            param!("project_path", "Optional project path filter", ["projectPath"]),
            param!("query", "Search query"),
            param!("content_types", "Content card types", ["contentTypes"]),
            param!("recent_runs", "Most recent delta-bearing sync runs to inspect", ["recentRuns"]),
            param!("limit", "Maximum number of hits"),
            param!("offset", "Pagination offset"),
            param!("search_options", "Optional retrieval settings", ["searchOptions"]),
        ],
        None
    ),
    command!(
        "get_conversation_search_index_status",
        "conversation.search.index.status",
        "Get the local conversation search index status",
        Read,
        App,
        false,
        NoParams,
        ServiceAsync => |service, _params| service.get_conversation_search_index_status().await,
        &[],
        None
    ),
    command!(
        "start_conversation_search_index_rebuild",
        "start_conversation_search_index_rebuild",
        "Rebuild the conversation search index",
        Write,
        App,
        false,
        NoParams,
        ServiceAsync => |service, _params| service.rebuild_conversation_search_index().await,
        &[],
        None
    ),
    command!(
        "update_conversation_part_translation",
        "conversation.part.translation.update",
        "Overwrite the stored translation for a conversation content part",
        Write,
        App,
        false,
        crate::backend::application::ConversationPartTranslationUpdateParams,
        ServiceAsync => |service, params| service.update_conversation_part_translation(params).await,
        &[
            param!("record_kind", "Conversation record table family", ["recordKind"]),
            param!("part_id", "Conversation part identifier", ["partId"]),
            param!("translated_text", "Translated content to store", ["translatedText"]),
        ],
        None
    ),
];
