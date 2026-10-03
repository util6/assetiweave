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
    conversation_turn_fingerprint, group_turn_ids_by_question, ConversationAdapter,
    ConversationAdapterCatalogRelease, ConversationAdapterKind, ConversationAdapterPackage,
    ConversationAdapterPackageOrigin, ConversationAdapterPackageVersion,
    ConversationAdapterRuntimeGateStatus, ConversationAdapterTrustState,
    ConversationCardKindDefinition, ConversationGroupingOrigin, ConversationPart,
    ConversationQuestion, ConversationQuestionTurn, ConversationSession, ConversationSource,
    ConversationSourceKind, ConversationSyncRun, ConversationSyncStatus, ConversationTurn,
    NormalizedConversationSession,
};
use crate::backend::store::{StoreError, StoreResult};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqliteRow, AssertSqlSafe, Executor, FromRow, Sqlite, SqlitePool, Transaction};
use std::collections::{BTreeMap, BTreeSet};

use crate::backend::store::codec::{decode_enum, decode_json, encode_enum, encode_json};

pub(crate) use super::search_index_repo::bump_conversation_search_source_revision_sqlx_tx;

pub(crate) mod adapters;
pub(crate) mod common;
pub(crate) mod content_details;
pub(crate) mod import;
pub(crate) mod import_advanced;
pub(crate) mod import_groups;
pub(crate) mod import_groups_aggregates;
pub(crate) mod import_prune;
pub(crate) mod import_records;
pub(crate) mod merge_split;
pub(crate) mod observations;
pub(crate) mod package_releases;
pub(crate) mod packages;
pub(crate) mod part_links;
pub(crate) mod prefixes;
pub(crate) mod projection;
pub(crate) mod reproject;
pub(crate) mod row_mappers;
pub(crate) mod search_cards;
pub(crate) mod search_records;
pub(crate) mod search_snippets;
pub(crate) mod sessions;
pub(crate) mod sources;
pub(crate) mod sql_constants;

pub(crate) use adapters::*;
pub(crate) use common::*;
pub(crate) use content_details::*;
pub(crate) use import::*;
pub(crate) use import_advanced::*;
pub(crate) use import_groups::*;
pub(crate) use import_groups_aggregates::*;
pub(crate) use import_prune::*;
pub(crate) use import_records::*;
pub(crate) use merge_split::*;
pub(crate) use observations::*;
pub(crate) use package_releases::*;
pub(crate) use packages::*;
pub(crate) use part_links::*;
pub(crate) use prefixes::*;
pub(crate) use projection::*;
pub(crate) use reproject::*;
pub(crate) use row_mappers::*;
pub(crate) use search_cards::*;
pub(crate) use search_records::*;
pub(crate) use search_snippets::*;
pub(crate) use sessions::*;
pub(crate) use sources::*;
pub(crate) use sql_constants::*;

#[cfg(test)]
#[path = "../conversation_repo_tests.rs"]
mod tests;
