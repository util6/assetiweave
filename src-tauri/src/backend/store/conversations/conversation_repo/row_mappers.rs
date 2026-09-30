use super::*;

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

#[derive(Debug, FromRow)]
pub(super) struct ConversationAdapterPackageRow {
    package_id: String,
    adapter_id: String,
    name: String,
    version: String,
    record_kind: String,
    install_dir: String,
    manifest_path: String,
    adapter_manifest_path: String,
    runtime_protocol: String,
    runtime_ready: i64,
    origin: String,
    source_url: Option<String>,
    git_ref: Option<String>,
    git_commit: Option<String>,
    catalog_url: Option<String>,
    update_policy: String,
    latest_version: Option<String>,
    last_checked_at: Option<String>,
    runtime_gate_status: String,
    runtime_validated_at: Option<String>,
    installed_content_hash: Option<String>,
    trusted_package_hash: Option<String>,
    error_message: Option<String>,
    created_at: String,
    updated_at: String,
}

impl ConversationAdapterPackageRow {
    fn into_domain(self) -> StoreResult<ConversationAdapterPackage> {
        Ok(ConversationAdapterPackage {
            package_id: self.package_id,
            adapter_id: self.adapter_id,
            name: self.name,
            version: self.version,
            record_kind: decode_enum(self.record_kind)?,
            install_dir: self.install_dir,
            manifest_path: self.manifest_path,
            adapter_manifest_path: self.adapter_manifest_path,
            runtime_protocol: self.runtime_protocol,
            runtime_ready: self.runtime_ready == 1,
            origin: decode_enum(self.origin)?,
            source_url: self.source_url,
            git_ref: self.git_ref,
            git_commit: self.git_commit,
            catalog_url: self.catalog_url,
            update_policy: decode_enum(self.update_policy)?,
            latest_version: self.latest_version,
            last_checked_at: self.last_checked_at,
            runtime_gate_status: decode_enum(self.runtime_gate_status)?,
            runtime_validated_at: self.runtime_validated_at,
            installed_content_hash: self.installed_content_hash,
            trusted_package_hash: self.trusted_package_hash,
            error_message: self.error_message,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

pub(super) fn map_sqlx_conversation_adapter_package(
    row: &SqliteRow,
) -> StoreResult<ConversationAdapterPackage> {
    ConversationAdapterPackageRow::from_row(row)
        .map_err(StoreError::external)?
        .into_domain()
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
        }
    }
}

pub(crate) fn map_sqlx_conversation_session(row: &SqliteRow) -> StoreResult<ConversationSession> {
    Ok(ConversationSessionRow::from_row(row)
        .map_err(StoreError::external)?
        .into_domain())
}

#[derive(Debug, FromRow)]
pub(super) struct ConversationTurnRow {
    id: String,
    session_id: String,
    external_id: String,
    turn_index: i64,
    user_text: String,
    title: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    fingerprint: String,
    missing: i64,
    imported_at: String,
    model: Option<String>,
}

impl ConversationTurnRow {
    fn into_domain(self) -> ConversationTurn {
        ConversationTurn {
            id: self.id,
            session_id: self.session_id,
            external_id: self.external_id,
            turn_index: self.turn_index,
            user_text: self.user_text,
            title: self.title,
            started_at: self.started_at,
            ended_at: self.ended_at,
            fingerprint: self.fingerprint,
            missing: self.missing == 1,
            imported_at: self.imported_at,
            model: self.model,
        }
    }
}

pub(crate) fn map_sqlx_conversation_turn(row: &SqliteRow) -> StoreResult<ConversationTurn> {
    Ok(ConversationTurnRow::from_row(row)
        .map_err(StoreError::external)?
        .into_domain())
}

#[derive(Debug, FromRow)]
pub(super) struct ConversationPartRow {
    id: String,
    turn_id: String,
    part_index: i64,
    role: String,
    kind: String,
    text: Option<String>,
    language: Option<String>,
    command: Option<String>,
    cwd: Option<String>,
    status: Option<String>,
    exit_code: Option<i64>,
    metadata_json: Option<String>,
    command_label: Option<String>,
    source_execution_id: Option<String>,
    content_card_json: Option<String>,
    translated_text: Option<String>,
}

impl ConversationPartRow {
    fn into_domain(self) -> StoreResult<ConversationPart> {
        Ok(ConversationPart {
            id: self.id,
            turn_id: self.turn_id,
            part_index: self.part_index,
            role: decode_enum(self.role)?,
            kind: decode_enum(self.kind)?,
            text: self.text,
            language: self.language,
            command: self.command,
            cwd: self.cwd,
            status: self.status,
            exit_code: self.exit_code.map(|v| v as i32),
            command_label: self.command_label,
            source_execution_id: self.source_execution_id,
            content_card: self.content_card_json.map(decode_json).transpose()?,
            metadata_json: self.metadata_json,
            translated_text: self.translated_text,
        })
    }
}

pub(crate) fn map_sqlx_conversation_part(row: &SqliteRow) -> StoreResult<ConversationPart> {
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

pub(crate) fn map_sqlx_conversation_question(row: &SqliteRow) -> StoreResult<ConversationQuestion> {
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

pub(crate) fn map_sqlx_conversation_question_turn(
    row: &SqliteRow,
) -> StoreResult<ConversationQuestionTurn> {
    ConversationQuestionTurnRow::from_row(row)
        .map_err(StoreError::external)?
        .into_domain()
}
