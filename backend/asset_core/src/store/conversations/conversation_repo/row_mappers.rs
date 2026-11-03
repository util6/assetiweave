use super::*;
use crate::domain::conversations::{
    derive_part_id, ConversationAdapterPackageOrigin, ConversationAdapterPackageRecordKind,
    ConversationAdapterRuntimeGateStatus, ConversationContentCardDescriptor,
    ConversationPackageUpdatePolicy, ConversationPartKind, ConversationPartRole,
};
use serde::Deserialize;
use sqlx::Row;

#[derive(Debug, FromRow)]
pub(super) struct ConversationAdapterRow {
    id: String,
    name: String,
    kind: String,
    version: String,
    enabled: i64,
    manifest_path: Option<String>,
    executable_path: Option<String>,
    content_hash: Option<String>,
    trusted_hash: Option<String>,
    trust_state: String,
    protocol_version: Option<i64>,
    capabilities: String,
    input_kinds: String,
    card_contract_version: Option<i64>,
    card_kinds_json: String,
    projection_version: Option<i64>,
    created_at: String,
    updated_at: String,
}

impl ConversationAdapterRow {
    fn into_domain(self) -> StoreResult<ConversationAdapter> {
        let protocol_version = self
            .protocol_version
            .map(|value| {
                u32::try_from(value)
                    .map_err(|_| StoreError::external(format!("invalid protocol_version: {value}")))
            })
            .transpose()?;
        let card_contract_version = self
            .card_contract_version
            .map(|value| {
                u32::try_from(value).map_err(|_| {
                    StoreError::external(format!("invalid card_contract_version: {value}"))
                })
            })
            .transpose()?;
        let projection_version = self
            .projection_version
            .map(|value| {
                u32::try_from(value).map_err(|_| {
                    StoreError::external(format!("invalid projection_version: {value}"))
                })
            })
            .transpose()?;
        Ok(ConversationAdapter {
            id: self.id,
            name: self.name,
            kind: decode_enum(self.kind)?,
            version: self.version,
            enabled: self.enabled == 1,
            manifest_path: self.manifest_path,
            executable_path: self.executable_path,
            content_hash: self.content_hash,
            trusted_hash: self.trusted_hash,
            trust_state: decode_enum(self.trust_state)?,
            protocol_version,
            capabilities: decode_json(self.capabilities)?,
            input_kinds: decode_json(self.input_kinds)?,
            card_contract_version,
            card_kinds: decode_json(self.card_kinds_json)?,
            projection_version,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

pub(super) fn map_sqlx_conversation_adapter(row: &SqliteRow) -> StoreResult<ConversationAdapter> {
    ConversationAdapterRow::from_row(row)
        .map_err(StoreError::external)?
        .into_domain()
}

pub(super) fn deserialize_bool_flexible<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct BoolVisitor;
    impl<'de> serde::de::Visitor<'de> for BoolVisitor {
        type Value = bool;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a boolean or an integer 0/1")
        }
        fn visit_bool<E>(self, v: bool) -> Result<bool, E> {
            Ok(v)
        }
        fn visit_i64<E>(self, v: i64) -> Result<bool, E> {
            Ok(v != 0)
        }
        fn visit_u64<E>(self, v: u64) -> Result<bool, E> {
            Ok(v != 0)
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<bool, E> {
            match v {
                "true" | "1" => Ok(true),
                "false" | "0" => Ok(false),
                _ => Err(E::custom(format!("invalid boolean string: {v}"))),
            }
        }
    }
    deserializer.deserialize_any(BoolVisitor)
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct ConversationAdapterCapabilityMeta {
    pub record_kind: ConversationAdapterPackageRecordKind,
    pub manifest_path: String,
    pub adapter_manifest_path: String,
    pub runtime_protocol: String,
    #[serde(deserialize_with = "deserialize_bool_flexible")]
    pub runtime_ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted_package_hash: Option<String>,
    pub origin: ConversationAdapterPackageOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_url: Option<String>,
    pub update_policy: ConversationPackageUpdatePolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<String>,
    pub runtime_gate_status: ConversationAdapterRuntimeGateStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_validated_at: Option<String>,
}

pub(super) fn map_sqlx_conversation_adapter_package(
    row: &SqliteRow,
) -> StoreResult<ConversationAdapterPackage> {
    let package_id: String = row.get("extension_id");
    let adapter_id: String = row.get("target_id");
    let name: String = row.get("display_name");
    let version: String = row.get("active_version");
    let install_dir: String = row
        .get::<Option<String>, _>("install_dir")
        .unwrap_or_default();
    let error_message: Option<String> = row.get("error_message");
    let created_at: String = row.get("installed_at");
    let updated_at: String = row.get("updated_at");
    let meta_json: String = row.get("capability_meta_json");
    let meta: ConversationAdapterCapabilityMeta =
        serde_json::from_str(&meta_json).map_err(|e| {
            StoreError::external(format!(
                "failed to parse conversation adapter capability meta: {e}"
            ))
        })?;

    Ok(ConversationAdapterPackage {
        package_id,
        adapter_id,
        name,
        version,
        record_kind: meta.record_kind,
        install_dir,
        manifest_path: meta.manifest_path,
        adapter_manifest_path: meta.adapter_manifest_path,
        runtime_protocol: meta.runtime_protocol,
        runtime_ready: meta.runtime_ready,
        origin: meta.origin,
        source_url: meta.source_url,
        git_ref: meta.git_ref,
        git_commit: meta.git_commit,
        catalog_url: meta.catalog_url,
        update_policy: meta.update_policy,
        latest_version: meta.latest_version,
        last_checked_at: meta.last_checked_at,
        runtime_gate_status: meta.runtime_gate_status,
        runtime_validated_at: meta.runtime_validated_at,
        installed_content_hash: meta.installed_content_hash,
        trusted_package_hash: meta.trusted_package_hash,
        error_message,
        created_at,
        updated_at,
    })
}

#[derive(Debug, FromRow)]
pub(super) struct ConversationSourceRow {
    id: String,
    adapter_id: String,
    name: String,
    kind: String,
    location: String,
    config_json: Option<String>,
    enabled: i64,
    last_synced_at: Option<String>,
    last_sync_status: Option<String>,
    created_at: String,
    updated_at: String,
    synced_adapter_hash: Option<String>,
    synced_card_contract_version: Option<i64>,
    synced_payload_policy_version: Option<i64>,
    synced_projection_version: Option<i64>,
}

impl ConversationSourceRow {
    fn into_domain(self) -> StoreResult<ConversationSource> {
        Ok(ConversationSource {
            id: self.id,
            adapter_id: self.adapter_id,
            name: self.name,
            kind: decode_enum(self.kind)?,
            location: self.location,
            config_json: self.config_json,
            enabled: self.enabled == 1,
            last_synced_at: self.last_synced_at,
            last_sync_status: self.last_sync_status,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

pub(super) fn map_sqlx_conversation_source(row: &SqliteRow) -> StoreResult<ConversationSource> {
    ConversationSourceRow::from_row(row)
        .map_err(StoreError::external)?
        .into_domain()
}

#[derive(Debug, FromRow)]
pub(super) struct ConversationSessionRow {
    id: String,
    source_id: String,
    adapter_id: String,
    external_id: String,
    title: String,
    project_path: Option<String>,
    started_at: Option<String>,
    updated_at: Option<String>,
    source_locator: Option<String>,
    source_fingerprint: Option<String>,
    missing: i64,
    created_at: String,
    imported_at: String,
    #[sqlx(default)]
    execution_origin: Option<String>,
    #[sqlx(default)]
    execution_purpose: Option<String>,
    #[sqlx(default)]
    user_visible: Option<i64>,
    #[sqlx(default)]
    last_cwd: Option<String>,
}

impl ConversationSessionRow {
    fn into_domain(self) -> ConversationSession {
        ConversationSession {
            id: self.id,
            source_id: self.source_id,
            adapter_id: self.adapter_id,
            external_id: self.external_id,
            title: self.title,
            project_path: self.project_path,
            started_at: self.started_at,
            updated_at: self.updated_at,
            source_locator: self.source_locator,
            source_fingerprint: self.source_fingerprint,
            missing: self.missing == 1,
            created_at: self.created_at,
            imported_at: self.imported_at,
            execution_origin: self.execution_origin.unwrap_or_else(|| "user".to_string()),
            execution_purpose: self.execution_purpose,
            user_visible: self.user_visible.map(|v| v != 0).unwrap_or(true),
            last_cwd: self.last_cwd,
        }
    }
}

pub fn map_sqlx_conversation_session(row: &SqliteRow) -> StoreResult<ConversationSession> {
    Ok(ConversationSessionRow::from_row(row)
        .map_err(StoreError::external)?
        .into_domain())
}

#[derive(Debug, Default, Deserialize)]
struct TurnProcessSparseMeta {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    turn_kind: Option<String>,
    #[serde(default)]
    agent_role: Option<String>,
    #[serde(default)]
    parent_turn_id: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub(crate) struct ConversationTurnRow {
    id: String,
    session_id: String,
    external_id: String,
    turn_index: i64,
    prompt: String,
    process: Option<String>,
    response: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    fingerprint: String,
    missing: i64,
    imported_at: String,
    model: Option<String>,
}

impl ConversationTurnRow {
    pub(crate) fn into_turn(self) -> ConversationTurn {
        self.into_domain()
    }

    fn into_domain(self) -> ConversationTurn {
        let (title, turn_kind, agent_role, parent_turn_id) = self
            .process
            .as_deref()
            .and_then(|s| serde_json::from_str::<TurnProcessSparseMeta>(s).ok())
            .map(|m| (m.title, m.turn_kind, m.agent_role, m.parent_turn_id))
            .unwrap_or_default();

        ConversationTurn {
            id: self.id,
            session_id: self.session_id,
            external_id: self.external_id,
            turn_index: self.turn_index,
            prompt: self.prompt,
            title,
            started_at: self.started_at,
            ended_at: self.ended_at,
            fingerprint: self.fingerprint,
            missing: self.missing == 1,
            imported_at: self.imported_at,
            model: self.model,
            process: self.process,
            response: self.response,
            turn_kind,
            agent_role,
            parent_turn_id,
        }
    }
}

pub fn map_sqlx_conversation_turn(row: &SqliteRow) -> StoreResult<ConversationTurn> {
    Ok(ConversationTurnRow::from_row(row)
        .map_err(StoreError::external)?
        .into_domain())
}

#[derive(Debug, FromRow)]
pub(super) struct ConversationPartRow {
    pub(super) turn_id: String,
    pub(super) part_index: i64,
    pub(super) kind: String,
    pub(super) content: Option<String>,
    pub(super) attrs_json: Option<String>,
    pub(super) translated_text: Option<String>,
}

impl ConversationPartRow {
    pub(super) fn into_domain(self) -> StoreResult<ConversationPart> {
        let triad_part = crate::backend::domain::conversations::triad::TriadPart {
            part_index: self.part_index,
            kind: self.kind,
            content: self.content,
            attrs_json: self.attrs_json,
            translated_text: self.translated_text,
        };
        let prefix = if self.turn_id.starts_with("web-record-") {
            "web-record-part"
        } else {
            "conversation-part"
        };
        let restored = crate::backend::domain::conversations::triad::merge_turn_triad(
            "codex",
            prefix,
            &self.turn_id,
            "",
            None,
            None,
            &[triad_part],
        );
        restored
            .into_iter()
            .next()
            .ok_or_else(|| StoreError::external("failed to restore conversation part from row"))
    }
}

pub fn map_sqlx_conversation_part(row: &SqliteRow) -> StoreResult<ConversationPart> {
    ConversationPartRow::from_row(row)
        .map_err(StoreError::external)?
        .into_domain()
}

#[derive(Debug, FromRow)]
pub(super) struct ConversationQuestionRow {
    id: String,
    session_id: String,
    title: Option<String>,
    created_at: String,
    updated_at: String,
}

impl ConversationQuestionRow {
    fn into_domain(self) -> ConversationQuestion {
        ConversationQuestion {
            id: self.id,
            session_id: self.session_id,
            title: self.title,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

pub fn map_sqlx_conversation_question(row: &SqliteRow) -> StoreResult<ConversationQuestion> {
    Ok(ConversationQuestionRow::from_row(row)
        .map_err(StoreError::external)?
        .into_domain())
}

#[derive(Debug, FromRow)]
pub(super) struct ConversationQuestionTurnRow {
    question_id: String,
    turn_id: String,
    turn_order: i64,
    assignment_origin: String,
    assigned_at: String,
    updated_at: String,
}

impl ConversationQuestionTurnRow {
    fn into_domain(self) -> StoreResult<ConversationQuestionTurn> {
        Ok(ConversationQuestionTurn {
            question_id: self.question_id,
            turn_id: self.turn_id,
            turn_order: self.turn_order,
            assignment_origin: decode_enum(self.assignment_origin)?,
            assigned_at: self.assigned_at,
            updated_at: self.updated_at,
        })
    }
}

pub fn map_sqlx_conversation_question_turn(
    row: &SqliteRow,
) -> StoreResult<ConversationQuestionTurn> {
    ConversationQuestionTurnRow::from_row(row)
        .map_err(StoreError::external)?
        .into_domain()
}
