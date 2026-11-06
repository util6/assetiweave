use crate::backend::domain::conversations::{
    ConversationRecordKind, ConversationSessionDetail, ConversationSessionListItem,
};
use crate::backend::domain::{
    ConversationGroupingOrigin, ConversationPart, ConversationSession, ConversationSource,
    ConversationSyncStatus, ConversationTurn, NormalizedConversationSession,
};
use crate::backend::store::{StoreError, StoreResult};
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use std::collections::BTreeMap;

use super::snapshot::{prepare_session_snapshot, replace_session_snapshot_sqlx_tx};
use super::{
    codec::{decode_json_app, encode_enum_app, encode_json_app},
    conversation_repo::{
        build_question_details_from_dynamic_groups, conversation_part_index_ready_sqlx,
        map_sqlx_conversation_part, map_sqlx_conversation_session, ConversationImportResult,
        ConversationTurnRow, CONVERSATION_IMPORT_BATCH_SIZE,
    },
};

mod cleanup;
mod import;
mod list;
mod types_and_helpers;

pub use cleanup::*;
pub use import::*;
pub use list::*;
pub use types_and_helpers::*;

#[cfg(test)]
#[path = "../web_record_repo_tests.rs"]
mod tests;
