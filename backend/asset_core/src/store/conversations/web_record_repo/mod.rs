use crate::backend::domain::conversations::{
    ConversationRecordKind, ConversationSessionDetail, ConversationSessionListItem,
};
use crate::backend::domain::{
    conversation_turn_fingerprint, ConversationGroupingOrigin, ConversationPart,
    ConversationSession, ConversationSource, ConversationSyncRun, ConversationSyncStatus,
    ConversationTurn, NormalizedConversationSession,
};
use crate::backend::store::{StoreError, StoreResult};
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use std::collections::BTreeMap;

use super::{
    codec::{decode_json_app, encode_enum_app, encode_json_app},
    conversation_repo::{
        build_question_details_from_dynamic_groups, conversation_part_index_ready_sqlx,
        delete_session_part_index_sqlx_tx, delete_source_part_index_sqlx_tx,
        insert_conversation_sync_delta_sqlx_tx, map_sqlx_conversation_part,
        map_sqlx_conversation_session, replace_turn_part_index_sqlx_tx, ConversationImportResult,
        ConversationTurnRow, CONVERSATION_IMPORT_BATCH_SIZE,
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
