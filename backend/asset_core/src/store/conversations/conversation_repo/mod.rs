use crate::backend::domain::conversations::projection::{
    project_conversation_content_nodes, ConversationCard, ConversationContentNodeCandidate,
};
use crate::backend::domain::conversations::DomainEvent;
use crate::backend::domain::conversations::{
    ConversationBlockDetail, ConversationBlockLocator, ConversationCardRenderer,
    ConversationContentNode, ConversationMutationResult, ConversationQuestionDetail,
    ConversationRecordKind, ConversationSearchCardType, ConversationSearchHit,
    ConversationSearchPage, ConversationSessionDetail, ConversationSessionListItem,
};
use crate::backend::domain::{
    conversation_turn_fingerprint, ConversationAdapter, ConversationAdapterCatalogRelease,
    ConversationAdapterKind, ConversationAdapterPackage, ConversationAdapterPackageOrigin,
    ConversationAdapterPackageVersion, ConversationAdapterRuntimeGateStatus,
    ConversationAdapterTrustState, ConversationCardKindDefinition, ConversationGroupingOrigin,
    ConversationPart, ConversationQuestion, ConversationQuestionTurn, ConversationSession,
    ConversationSource, ConversationSourceKind, ConversationSyncRun, ConversationSyncStatus,
    ConversationTurn, NormalizedConversationSession,
};
use crate::backend::store::{StoreError, StoreResult};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqliteRow, AssertSqlSafe, Executor, FromRow, Sqlite, SqlitePool, Transaction};
use std::collections::{BTreeMap, BTreeSet};

use crate::backend::store::codec::{decode_enum, decode_json, encode_enum, encode_json};

pub use super::search_index_repo::bump_conversation_search_source_revision_sqlx_tx;

pub mod adapters;
pub mod common;
pub mod content_details;
pub mod import;
pub mod import_advanced;
pub mod import_groups;
pub mod import_groups_aggregates;
pub mod import_parts;
pub mod import_prune;
pub mod import_records;
pub mod merge_split;
pub mod observations;
pub mod outline;
pub mod package_releases;
pub mod packages;
pub mod part_index;
pub mod prefixes;
pub mod prefixes_any;
pub mod projection;
pub mod reproject;
pub mod row_mappers;
pub mod search_cards;
pub mod search_records;
pub mod search_snippets;
pub mod sessions;
pub mod sources;
pub mod sql_constants;

pub use adapters::*;
pub use common::*;
pub use content_details::*;
pub use import::*;
pub use import_advanced::*;
pub use import_groups::*;
pub use import_groups_aggregates::*;
pub use import_parts::*;
pub use import_prune::*;
pub use import_records::*;
pub use merge_split::*;
pub use observations::*;
pub use outline::*;
pub use package_releases::*;
pub use packages::*;
pub use part_index::*;
pub use prefixes::*;
pub use prefixes_any::*;
pub use projection::*;
pub use reproject::*;
pub use row_mappers::*;
pub use search_cards::*;
pub use search_records::*;
pub use search_snippets::*;
pub use sessions::*;
pub use sources::*;
pub use sql_constants::*;

#[cfg(test)]
#[path = "../conversation_repo_tests.rs"]
mod tests;

#[cfg(test)]
mod adversarial_tests;
