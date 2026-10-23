use crate::backend::domain::conversations::{
    ConversationQuestionDetail, ConversationSessionDetail, ConversationSessionListItem,
};
use crate::backend::domain::{
    conversation_turn_fingerprint, ConversationCardKindDefinition, ConversationGroupingOrigin,
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
        append_projected_cards_to_question_aggregate, decode_turn_question_origin,
        insert_conversation_sync_delta_sqlx_tx, map_sqlx_conversation_part,
        map_sqlx_conversation_question, map_sqlx_conversation_question_turn,
        map_sqlx_conversation_session, plan_turn_question_assignments,
        project_question_content_nodes, project_question_title, ConversationImportResult,
        TurnQuestionInput, CONVERSATION_IMPORT_BATCH_SIZE,
    },
};

mod cleanup;
mod import;
mod list;
mod parts_and_questions;
mod types_and_helpers;

pub use cleanup::*;
pub use import::*;
pub use list::*;
pub use parts_and_questions::*;
pub use types_and_helpers::*;

#[cfg(test)]
#[path = "../web_record_repo_tests.rs"]
mod tests;
