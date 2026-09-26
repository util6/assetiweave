use crate::backend::domain::conversations::{
    ConversationQuestionDetail, ConversationSessionDetail, ConversationSessionListItem,
};
use crate::backend::domain::{
    conversation_turn_fingerprint, group_turn_ids_by_question, ConversationCardKindDefinition,
    ConversationPart, ConversationQuestionTurn, ConversationSession, ConversationSource,
    ConversationSyncRun, ConversationSyncStatus, ConversationTurn, NormalizedConversationSession,
};
use crate::backend::store::{StoreError, StoreResult};
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use std::collections::BTreeMap;

use super::{
    codec::{decode_json_app, encode_enum_app, encode_json_app},
    conversation_repo::{
        append_projected_cards_to_question_aggregate, insert_conversation_sync_delta_sqlx_tx,
        map_sqlx_conversation_part, map_sqlx_conversation_question,
        map_sqlx_conversation_question_turn, map_sqlx_conversation_session,
        project_question_content_nodes, project_question_title, ConversationImportResult,
        CONVERSATION_IMPORT_BATCH_SIZE,
    },
};

mod cleanup;
mod import;
mod list;
mod parts_and_questions;
mod types_and_helpers;

pub(crate) use cleanup::*;
pub(crate) use import::*;
pub(crate) use list::*;
pub(crate) use parts_and_questions::*;
pub(crate) use types_and_helpers::*;

#[cfg(test)]
#[path = "../web_record_repo_tests.rs"]
mod tests;
