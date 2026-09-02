use super::prelude::*;
use crate::backend::{
    agents::types::AgentId,
    ai_execution::{
        execute_agent_blocking, AgentSessionMode, AiExecutionCancellation, AiExecutionLimits,
        AiExecutionPurpose, AiExecutionRequest,
    },
    models::{ConversationPartKind, ConversationPartRole, ConversationSourceKind},
    runtime::{AppError, AppResult},
};
use sha2::{Digest, Sha256};

const RECALL_SOURCE_ID: &str = "assetiweave-memory-recall";
const RECALL_ADAPTER_ID: &str = "assetiweave-memory-recall";
const MAX_RECALL_QUERY_CHARS: usize = 4_000;
const MAX_RECALL_ANSWER_CHARS: usize = 100_000;
const MAX_RECALL_REFERENCES: usize = 64;

impl AppService {
    pub(crate) fn memory_recall_pool(&self) -> &sqlx::SqlitePool {
        self.db.pool()
    }

    pub(crate) fn memory_recall_run_sync<F: std::future::Future>(&self, future: F) -> F::Output {
        self.runtime.run_sync(future)
    }

    pub(crate) fn create_memory_recall_session(
        &self,
        params: MemoryRecallSessionCreateParams,
    ) -> AppResult<MemoryRecallSession> {
        let now = Utc::now().to_rfc3339();
        let (agent_id, model) = self.resolve_recall_assignment()?;
        let id = Uuid::new_v4().to_string();
        let session = MemoryRecallSession {
            id: id.clone(),
            status: MemoryRecallSessionStatus::Active,
            scope: params.scope,
            execution_context_key: format!("memory-recall:{}", id),
            agent_id: agent_id.to_string(),
            model,
            turn_count: 0,
            active_turn_id: None,
            last_error: None,
            created_at: now.clone(),
            updated_at: now,
            turns: Vec::new(),
        };
        self.runtime
            .run_sync(crate::backend::store::create_memory_recall_session_sqlx(
                self.db.pool(),
                self.tenant_id(),
                &session,
            ))?;
        Ok(session)
    }

    pub(crate) fn get_memory_recall_session(
        &self,
        params: MemoryRecallSessionGetParams,
    ) -> AppResult<MemoryRecallSession> {
        let session_id = normalize_recall_id(&params.session_id, "session")?;
        self.runtime
            .run_sync(crate::backend::store::load_memory_recall_session_sqlx(
                self.db.pool(),
                self.tenant_id(),
                &session_id,
            ))?
            .ok_or_else(|| AppError::NotFound(format!("Recall session not found: {session_id}")))
    }

    pub(crate) fn send_memory_recall_turn(
        &self,
        params: MemoryRecallTurnSendParams,
    ) -> AppResult<MemoryRecallSession> {
        let session_id = normalize_recall_id(&params.session_id, "session")?;
        let query = redact_recall_query(&params.query)?;
        let session = self.get_memory_recall_session(MemoryRecallSessionGetParams {
            session_id: session_id.clone(),
        })?;
        if session.active_turn_id.is_some() {
            return Err(AppError::Conflict(
                "Recall session already has an active turn".to_string(),
            ));
        }
        if session.status != MemoryRecallSessionStatus::Active {
            return Err(AppError::Conflict(format!(
                "Recall session is not active: {}",
                session.status.as_str()
            )));
        }

        let turn_id = Uuid::new_v4().to_string();
        let conversation_session_id = recall_conversation_session_id(&session.id);
        let conversation_turn_id = recall_conversation_turn_id(&conversation_session_id, &turn_id);
        let now = Utc::now().to_rfc3339();
        let turn = MemoryRecallTurn {
            id: turn_id.clone(),
            session_id: session.id.clone(),
            sequence: session.turn_count,
            conversation_session_id,
            conversation_turn_id,
            status: MemoryRecallTurnStatus::Queued,
            user_text: query.clone(),
            structured_output: None,
            last_error: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.runtime
            .run_sync(crate::backend::store::create_memory_recall_turn_sqlx(
                self.db.pool(),
                self.tenant_id(),
                &turn,
            ))?;

        let mut history = session.turns.clone();
        history.push(turn.clone());
        if let Err(error) = self.persist_recall_conversation(self.tenant_id(), &session, &history) {
            let _ = self
                .runtime
                .run_sync(crate::backend::store::fail_memory_recall_turn_sqlx(
                    self.db.pool(),
                    self.tenant_id(),
                    &turn.id,
                    MemoryRecallTurnStatus::Failed,
                    &error.to_string(),
                ));
            return Err(error);
        }

        self.schedule_memory_recall_turn_for_tenant(self.tenant_id(), &turn.id)?;
        self.get_memory_recall_session(MemoryRecallSessionGetParams { session_id })
    }

    pub(crate) fn cancel_memory_recall_turn(
        &self,
        params: MemoryRecallTurnCancelParams,
    ) -> AppResult<MemoryRecallSession> {
        let turn_id = normalize_recall_id(&params.turn_id, "turn")?;
        let turn = self
            .runtime
            .run_sync(crate::backend::store::load_memory_recall_turn_sqlx(
                self.db.pool(),
                self.tenant_id(),
                &turn_id,
            ))?
            .ok_or_else(|| AppError::NotFound(format!("Recall turn not found: {turn_id}")))?;
        let task_id = format!("memory-recall:{turn_id}");
        self.runtime
            .run_sync(crate::backend::store::fail_memory_recall_turn_sqlx(
                self.db.pool(),
                self.tenant_id(),
                &turn.id,
                MemoryRecallTurnStatus::Cancelled,
                "Recall turn cancelled by user",
            ))?;
        let _ = self.runtime.task_runtime().cancel(&task_id);
        self.get_memory_recall_session(MemoryRecallSessionGetParams {
            session_id: turn.session_id,
        })
    }

    pub(crate) fn recover_memory_recall_turns_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<usize> {
        let turns = self.runtime.run_sync(
            crate::backend::store::list_memory_recall_turns_for_recovery_sqlx(
                self.db.pool(),
                tenant_id,
            ),
        )?;
        let mut scheduled = 0;
        for (turn_id, status) in turns {
            if status == MemoryRecallTurnStatus::Running {
                self.runtime
                    .run_sync(crate::backend::store::fail_memory_recall_turn_sqlx(
                        self.db.pool(),
                        tenant_id,
                        &turn_id,
                        MemoryRecallTurnStatus::ResumeUnavailable,
                        "Recall provider execution was interrupted before restart",
                    ))?;
                continue;
            }
            self.schedule_memory_recall_turn_for_tenant(tenant_id, &turn_id)?;
            scheduled += 1;
        }
        Ok(scheduled)
    }

    pub(crate) fn schedule_memory_recall_turn_for_tenant(
        &self,
        tenant_id: &str,
        turn_id: &str,
    ) -> AppResult<()> {
        let runtime = self.runtime.clone();
        let tenant_id_for_task = tenant_id.to_string();
        let turn_id_for_task = turn_id.to_string();
        let task_id = format!("memory-recall:{turn_id}");
        let task_runtime = runtime.task_runtime().clone();
        let mut spec = crate::backend::runtime::tasks::TaskSpec::new(
            crate::backend::runtime::tasks::TaskKind::Memory,
            Some(task_id),
        )
        .with_tenant_id(tenant_id.to_string())
        .with_conflict_key(format!("memory-recall-session:{tenant_id}:{turn_id}"));
        spec.detail = serde_json::json!({
            "domain": "memory_recall",
            "job_id": turn_id,
        });
        let spawn = task_runtime.spawn(
            spec,
            Box::new(move |context| {
                let service = AppService::from_runtime(&runtime);
                if context.is_cancelled() {
                    let _ = service.runtime.run_sync(
                        crate::backend::store::fail_memory_recall_turn_sqlx(
                            service.db.pool(),
                            &tenant_id_for_task,
                            &turn_id_for_task,
                            MemoryRecallTurnStatus::Cancelled,
                            "Recall task cancelled before execution",
                        ),
                    );
                    return Err(AppError::Canceled("Recall task cancelled".to_string()));
                }
                service.run_memory_recall_turn_for_tenant(
                    &tenant_id_for_task,
                    &turn_id_for_task,
                    AiExecutionCancellation::from_token(context.cancellation()),
                )
            }),
        );
        match spawn {
            Ok(crate::backend::runtime::tasks::SpawnOutcome::Started)
            | Ok(crate::backend::runtime::tasks::SpawnOutcome::Existing) => Ok(()),
            Err(error) => {
                let _ = self
                    .runtime
                    .run_sync(crate::backend::store::fail_memory_recall_turn_sqlx(
                        self.db.pool(),
                        tenant_id,
                        turn_id,
                        MemoryRecallTurnStatus::Failed,
                        &error.to_string(),
                    ));
                Err(error)
            }
        }
    }

    fn run_memory_recall_turn_for_tenant(
        &self,
        tenant_id: &str,
        turn_id: &str,
        cancellation: AiExecutionCancellation,
    ) -> AppResult<Value> {
        let turn = self
            .runtime
            .run_sync(crate::backend::store::load_memory_recall_turn_sqlx(
                self.db.pool(),
                tenant_id,
                turn_id,
            ))?
            .ok_or_else(|| AppError::NotFound(format!("Recall turn not found: {turn_id}")))?;
        if cancellation.is_cancelled() {
            let _ = self
                .runtime
                .run_sync(crate::backend::store::fail_memory_recall_turn_sqlx(
                    self.db.pool(),
                    tenant_id,
                    turn_id,
                    MemoryRecallTurnStatus::Cancelled,
                    "Recall task cancelled before execution",
                ));
            return Err(AppError::Canceled("Recall task cancelled".to_string()));
        }
        self.runtime
            .run_sync(crate::backend::store::mark_memory_recall_turn_running_sqlx(
                self.db.pool(),
                tenant_id,
                turn_id,
            ))?;
        let session = self
            .runtime
            .run_sync(crate::backend::store::load_memory_recall_session_sqlx(
                self.db.pool(),
                tenant_id,
                &turn.session_id,
            ))?
            .ok_or_else(|| {
                AppError::NotFound(format!("Recall session not found: {}", turn.session_id))
            })?;
        let prompt = build_recall_prompt(&session, &turn);
        let request = AiExecutionRequest {
            execution_id: format!("memory-recall-{turn_id}"),
            agent_id: AgentId::parse(&session.agent_id)
                .map_err(|error| AppError::Validation(error.to_string()))?,
            purpose: AiExecutionPurpose::Recall,
            session_mode: AgentSessionMode::Persistent,
            prompt,
            model: session.model.clone(),
            limits: AiExecutionLimits::default(),
            cancellation: cancellation.clone(),
            progress: None,
            tenant_id: Some(tenant_id.to_string()),
            execution_context_key: Some(session.execution_context_key.clone()),
            binding: None,
            replay: false,
            restore_only: false,
            team_tools: None,
            recall_tools: Some(crate::backend::ai_execution::AiRecallTools {
                tenant_id: tenant_id.to_string(),
                recall_session_id: session.id.clone(),
                database_path: self.db_path.to_string_lossy().into_owned(),
            }),
        };
        let result = match execute_agent_blocking(self.agent_runtime.clone(), request) {
            Ok(result) => result,
            Err(error) => {
                let view = error.to_view();
                let status = if view.code == "resume_unavailable" {
                    MemoryRecallTurnStatus::ResumeUnavailable
                } else if view.code == "cancelled" || view.code == "canceled" {
                    MemoryRecallTurnStatus::Cancelled
                } else {
                    MemoryRecallTurnStatus::Failed
                };
                let _ = self
                    .runtime
                    .run_sync(crate::backend::store::fail_memory_recall_turn_sqlx(
                        self.db.pool(),
                        tenant_id,
                        turn_id,
                        status,
                        &view.message,
                    ));
                return Err(AppError::Domain {
                    code: view.code,
                    message: view.message,
                    retryable: view.retryable,
                    details: None,
                });
            }
        };
        if cancellation.is_cancelled() {
            let _ = self
                .runtime
                .run_sync(crate::backend::store::fail_memory_recall_turn_sqlx(
                    self.db.pool(),
                    tenant_id,
                    turn_id,
                    MemoryRecallTurnStatus::Cancelled,
                    "Recall task cancelled during execution",
                ));
            return Err(AppError::Canceled("Recall task cancelled".to_string()));
        }
        let output =
            match parse_and_validate_recall_output(self, tenant_id, &session.scope, &result.text) {
                Ok(output) => output,
                Err(error) => {
                    let _ =
                        self.runtime
                            .run_sync(crate::backend::store::fail_memory_recall_turn_sqlx(
                                self.db.pool(),
                                tenant_id,
                                turn_id,
                                MemoryRecallTurnStatus::Failed,
                                &error.to_string(),
                            ));
                    return Err(error);
                }
            };
        let current_status = self
            .runtime
            .run_sync(crate::backend::store::load_memory_recall_turn_sqlx(
                self.db.pool(),
                tenant_id,
                turn_id,
            ))?
            .map(|turn| turn.status);
        if current_status != Some(MemoryRecallTurnStatus::Running) {
            return Err(AppError::Canceled(
                "Recall turn is no longer active".to_string(),
            ));
        }
        let session_after = self
            .runtime
            .run_sync(crate::backend::store::load_memory_recall_session_sqlx(
                self.db.pool(),
                tenant_id,
                &session.id,
            ))?
            .ok_or_else(|| {
                AppError::NotFound(format!("Recall session not found: {}", session.id))
            })?;
        let mut history = session_after.turns.clone();
        let current = history
            .iter_mut()
            .find(|item| item.id == turn_id)
            .ok_or_else(|| AppError::NotFound(format!("Recall turn not found: {turn_id}")))?;
        current.structured_output = Some(output.clone());
        current.status = MemoryRecallTurnStatus::Completed;
        self.persist_recall_conversation(tenant_id, &session_after, &history)?;
        self.record_recall_usage(tenant_id, turn_id, &output)?;
        self.runtime
            .run_sync(crate::backend::store::complete_memory_recall_turn_sqlx(
                self.db.pool(),
                tenant_id,
                turn_id,
                &output,
            ))?;
        Ok(serde_json::json!({
            "turnId": turn_id,
            "status": "completed"
        }))
    }

    fn record_recall_usage(
        &self,
        tenant_id: &str,
        turn_id: &str,
        output: &MemoryRecallStructuredOutput,
    ) -> AppResult<()> {
        if !crate::backend::app_settings::memory_usage_enabled_for_database(&self.db)? {
            return Ok(());
        }
        let used_at = Utc::now().to_rfc3339();
        self.runtime.run_sync(async {
            for reference in &output.session_references {
                crate::backend::store::record_memory_usage_event_sqlx(
                    self.db.pool(),
                    tenant_id,
                    "recall_session",
                    &reference.session_id,
                    "recall_turn",
                    turn_id,
                    &used_at,
                )
                .await?;
            }
            for reference in &output.content_references {
                crate::backend::store::record_memory_usage_event_sqlx(
                    self.db.pool(),
                    tenant_id,
                    "recall_content",
                    &reference.block_id,
                    "recall_turn",
                    turn_id,
                    &used_at,
                )
                .await?;
            }
            Ok::<_, AppError>(())
        })
    }

    fn persist_recall_conversation(
        &self,
        tenant_id: &str,
        session: &MemoryRecallSession,
        turns: &[MemoryRecallTurn],
    ) -> AppResult<()> {
        let now = Utc::now().to_rfc3339();
        let source = crate::backend::models::ConversationSource {
            id: RECALL_SOURCE_ID.to_string(),
            adapter_id: RECALL_ADAPTER_ID.to_string(),
            name: "AssetIWeave Recall sessions".to_string(),
            kind: ConversationSourceKind::Custom,
            location: "app://memory-recall".to_string(),
            config_json: Some(r#"{"readOnly":true,"owner":"assetiweave"}"#.to_string()),
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let normalized_turns = turns
            .iter()
            .map(|turn| {
                let mut parts = vec![crate::backend::models::NormalizedConversationPart {
                    role: ConversationPartRole::User,
                    kind: ConversationPartKind::Text,
                    text: Some(turn.user_text.clone()),
                    language: None,
                    command: None,
                    cwd: session.scope.project_path.clone(),
                    status: None,
                    exit_code: None,
                    command_label: None,
                    source_execution_id: None,
                    content_card: None,
                    metadata_json: None,
                }];
                if let Some(output) = turn.structured_output.as_ref() {
                    parts.push(crate::backend::models::NormalizedConversationPart {
                        role: ConversationPartRole::Assistant,
                        kind: ConversationPartKind::Text,
                        text: Some(output.answer.clone()),
                        language: None,
                        command: None,
                        cwd: None,
                        status: None,
                        exit_code: None,
                        command_label: None,
                        source_execution_id: None,
                        content_card: Some(
                            crate::backend::models::ConversationContentCardDescriptor {
                                schema_version: 1,
                                kind: "answer".to_string(),
                                renderer: Some("markdown".to_string()),
                            },
                        ),
                        metadata_json: None,
                    });
                }
                crate::backend::models::NormalizedConversationTurn {
                    external_id: turn.id.clone(),
                    turn_index: turn.sequence,
                    user_text: turn.user_text.clone(),
                    title: None,
                    started_at: Some(turn.created_at.clone()),
                    ended_at: Some(turn.updated_at.clone()),
                    parts,
                }
            })
            .collect::<Vec<_>>();
        let fingerprint = fingerprint_turns(&normalized_turns);
        let normalized = crate::backend::models::NormalizedConversationSession {
            external_id: session.id.clone(),
            title: Some("Recall".to_string()),
            project_path: session.scope.project_path.clone(),
            started_at: Some(session.created_at.clone()),
            updated_at: Some(now.clone()),
            source_locator: Some(format!("memory-recall://{}", session.id)),
            source_fingerprint: Some(fingerprint),
            turns: normalized_turns,
        };
        self.runtime.run_sync(async {
            crate::backend::store::upsert_conversation_source_sqlx(
                self.db.pool(),
                tenant_id,
                &source,
            )
            .await?;
            crate::backend::store::import_conversation_sessions_sqlx(
                self.db.pool(),
                tenant_id,
                &source,
                &[normalized],
                false,
            )
            .await
            .map(|_| ())
        })
    }

    fn resolve_recall_assignment(&self) -> AppResult<(AgentId, Option<String>)> {
        let settings =
            crate::backend::app_settings::read_app_settings_value_for_database(&self.db)?;
        crate::backend::ai_execution::composition::resolve_agent_for(
            &crate::backend::ai_execution::composition::ActionId::new("memory.recall"),
            &settings,
        )
    }
}

fn build_recall_prompt(session: &MemoryRecallSession, turn: &MemoryRecallTurn) -> String {
    let history = session
        .turns
        .iter()
        .filter(|item| item.id != turn.id)
        .map(|item| {
            let answer = item
                .structured_output
                .as_ref()
                .map(|output| output.answer.as_str())
                .unwrap_or("");
            format!("prior clue: {}\nprior answer: {answer}", item.user_text)
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "You are AssetIWeave's Recall Agent. Use only the read-only Recall tools exposed by the host. Never mutate data. Return JSON only with exactly these fields: answer (string), sessionReferences (array), contentReferences (array), followUpSuggestions (array of strings). Do not put internal IDs in answer. Current clue: {}\nScope: {}\n{}",
        turn.user_text,
        serde_json::to_string(&session.scope).unwrap_or_else(|_| "{}".to_string()),
        history
    )
}

fn parse_and_validate_recall_output(
    service: &AppService,
    tenant_id: &str,
    scope: &MemoryScope,
    raw: &str,
) -> AppResult<MemoryRecallStructuredOutput> {
    let redacted = crate::backend::memory_redaction::redact_memory_text(raw).text;
    let value = strip_json_fence(&redacted);
    let mut output: MemoryRecallStructuredOutput =
        serde_json::from_str(value).map_err(|error| {
            AppError::Validation(format!("Recall output schema is invalid: {error}"))
        })?;
    output.answer = output.answer.trim().to_string();
    if output.answer.is_empty() || output.answer.chars().count() > MAX_RECALL_ANSWER_CHARS {
        return Err(AppError::Validation(
            "Recall answer is empty or too long".to_string(),
        ));
    }
    if output.session_references.len() > MAX_RECALL_REFERENCES
        || output.content_references.len() > MAX_RECALL_REFERENCES
        || output.follow_up_suggestions.len() > MAX_RECALL_REFERENCES
    {
        return Err(AppError::Validation(
            "Recall output contains too many references".to_string(),
        ));
    }
    let mut valid_sessions = Vec::new();
    let mut session_keys = BTreeSet::new();
    for reference in output.session_references {
        let key = format!(
            "{}:{}:{}",
            reference.record_kind.as_str(),
            reference.session_id,
            reference.question_id.as_deref().unwrap_or_default()
        );
        if !session_keys.insert(key) {
            continue;
        }
        if !service.recall_session_reference_exists_for_scope(tenant_id, scope, &reference)? {
            return Err(AppError::Validation(
                "Recall output contains an invalid or out-of-scope session reference".to_string(),
            ));
        }
        valid_sessions.push(reference);
    }
    let mut valid_content = Vec::new();
    let mut content_keys = BTreeSet::new();
    for reference in output.content_references {
        let key = format!(
            "{}:{}:{}:{}",
            reference.record_kind.as_str(),
            reference.question_id,
            reference.turn_id.as_deref().unwrap_or_default(),
            reference.block_id
        );
        if !content_keys.insert(key) {
            continue;
        }
        if !service.recall_content_reference_exists_for_scope(tenant_id, scope, &reference)? {
            return Err(AppError::Validation(
                "Recall output contains an invalid or out-of-scope content reference".to_string(),
            ));
        }
        valid_content.push(reference);
    }
    let mut suggestions = output
        .follow_up_suggestions
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .take(MAX_RECALL_REFERENCES)
        .collect::<Vec<_>>();
    suggestions.dedup();
    let referenced_ids = valid_sessions
        .iter()
        .flat_map(|reference| {
            std::iter::once(reference.session_id.as_str()).chain(reference.question_id.as_deref())
        })
        .chain(valid_content.iter().flat_map(|reference| {
            std::iter::once(reference.session_id.as_str())
                .chain(std::iter::once(reference.question_id.as_str()))
                .chain(reference.turn_id.as_deref())
                .chain(std::iter::once(reference.block_id.as_str()))
        }));
    for id in referenced_ids {
        if !id.is_empty() && output.answer.contains(id) {
            return Err(AppError::Validation(
                "Recall answer must not contain internal locator IDs".to_string(),
            ));
        }
    }
    output.session_references = valid_sessions;
    output.content_references = valid_content;
    output.follow_up_suggestions = suggestions;
    Ok(output)
}

impl AppService {
    pub(crate) fn recall_session_reference_exists_for_scope(
        &self,
        tenant_id: &str,
        scope: &MemoryScope,
        reference: &MemoryRecallSessionReference,
    ) -> AppResult<bool> {
        let record_kind = match reference.record_kind {
            MemoryRecordKind::Session => crate::backend::dto::ConversationRecordKind::Session,
            MemoryRecordKind::Web => crate::backend::dto::ConversationRecordKind::Web,
        };
        let pool = self.db.pool().clone();
        let tenant_id = tenant_id.to_string();
        let session_id = reference.session_id.clone();
        let question_id = reference.question_id.clone();
        let app_id = scope.app_id.clone();
        let source_id = scope.source_id.clone();
        let project_path = scope.project_path.clone();
        let scoped_session_id = scope.session_id.clone();
        self.runtime.run_sync(async move {
            let session_exists = match record_kind {
                crate::backend::dto::ConversationRecordKind::Session => (
                    sqlx::query_scalar::<_, i64>(
                        "SELECT EXISTS(SELECT 1 FROM conversation_sessions s JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE s.tenant_id=?1 AND s.id=?2 AND s.missing=0 AND source.enabled=1 AND source.adapter_id <> 'assetiweave-memory-recall' AND (?3 IS NULL OR s.adapter_id=?3) AND (?4 IS NULL OR s.source_id=?4) AND (?5 IS NULL OR s.project_path=?5) AND (?6 IS NULL OR s.id=?6))",
                    )
                    .bind(&tenant_id)
                    .bind(&session_id)
                    .bind(&app_id)
                    .bind(&source_id)
                    .bind(&project_path)
                    .bind(&scoped_session_id)
                    .fetch_one(&pool)
                    .await
                    .map_err(AppError::external)?,
                    "conversation_questions",
                ),
                crate::backend::dto::ConversationRecordKind::Web => (
                    sqlx::query_scalar::<_, i64>(
                        "SELECT EXISTS(SELECT 1 FROM web_record_sessions s JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE s.tenant_id=?1 AND s.id=?2 AND s.missing=0 AND source.enabled=1 AND source.adapter_id <> 'assetiweave-memory-recall' AND (?3 IS NULL OR s.adapter_id=?3) AND (?4 IS NULL OR s.source_id=?4) AND (?5 IS NULL OR s.id=?5))",
                    )
                    .bind(&tenant_id)
                    .bind(&session_id)
                    .bind(&app_id)
                    .bind(&source_id)
                    .bind(&scoped_session_id)
                    .fetch_one(&pool)
                    .await
                    .map_err(AppError::external)?,
                    "web_record_questions",
                ),
            };
            if session_exists.0 == 0 {
                return Ok(false);
            }
            let Some(question_id) = question_id else {
                return Ok(true);
            };
            let exists = match record_kind {
                crate::backend::dto::ConversationRecordKind::Session => {
                    sqlx::query_scalar::<_, i64>(
                        "SELECT EXISTS(SELECT 1 FROM conversation_questions WHERE tenant_id=?1 AND id=?2 AND session_id=?3)",
                    )
                }
                crate::backend::dto::ConversationRecordKind::Web => {
                    sqlx::query_scalar::<_, i64>(
                        "SELECT EXISTS(SELECT 1 FROM web_record_questions WHERE tenant_id=?1 AND id=?2 AND session_id=?3)",
                    )
                }
            }
            .bind(&tenant_id)
            .bind(question_id)
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .map_err(AppError::external)?;
            Ok(exists != 0)
        })
    }

    pub(crate) fn recall_content_reference_exists_for_scope(
        &self,
        tenant_id: &str,
        scope: &MemoryScope,
        reference: &MemoryRecallContentReference,
    ) -> AppResult<bool> {
        let record_kind = match reference.record_kind {
            MemoryRecordKind::Session => crate::backend::dto::ConversationRecordKind::Session,
            MemoryRecordKind::Web => crate::backend::dto::ConversationRecordKind::Web,
        };
        let app_id = scope.app_id.clone();
        let source_id = scope.source_id.clone();
        let project_path = scope.project_path.clone();
        let scoped_session_id = scope.session_id.clone();
        let parent_exists = self.runtime.run_sync(async {
            let exists = match record_kind {
                crate::backend::dto::ConversationRecordKind::Session => {
                    sqlx::query_scalar::<_, i64>(
                        "SELECT EXISTS(SELECT 1 FROM conversation_questions q JOIN conversation_sessions s ON s.tenant_id=q.tenant_id AND s.id=q.session_id JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE q.tenant_id=?1 AND q.id=?2 AND q.session_id=?3 AND s.missing=0 AND source.enabled=1 AND source.adapter_id <> 'assetiweave-memory-recall' AND (?4 IS NULL OR s.adapter_id=?4) AND (?5 IS NULL OR s.source_id=?5) AND (?6 IS NULL OR s.project_path=?6) AND (?7 IS NULL OR s.id=?7))",
                    )
                }
                crate::backend::dto::ConversationRecordKind::Web => {
                    sqlx::query_scalar::<_, i64>(
                        "SELECT EXISTS(SELECT 1 FROM web_record_questions q JOIN web_record_sessions s ON s.tenant_id=q.tenant_id AND s.id=q.session_id JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE q.tenant_id=?1 AND q.id=?2 AND q.session_id=?3 AND s.missing=0 AND source.enabled=1 AND source.adapter_id <> 'assetiweave-memory-recall' AND (?4 IS NULL OR s.adapter_id=?4) AND (?5 IS NULL OR s.source_id=?5) AND (?7 IS NULL OR s.id=?7))",
                    )
                }
            }
            .bind(tenant_id)
            .bind(&reference.question_id)
            .bind(&reference.session_id)
            .bind(&app_id)
            .bind(&source_id)
            .bind(&project_path)
            .bind(&scoped_session_id)
            .fetch_one(self.db.pool())
            .await
            .map_err(AppError::external)?;
            Ok::<bool, AppError>(exists != 0)
        })?;
        if !parent_exists {
            return Ok(false);
        }
        let locators = self.runtime.run_sync(
            crate::backend::store::list_conversation_block_locators_sqlx(
                self.db.pool(),
                tenant_id,
                record_kind,
                &reference.question_id,
            ),
        );
        Ok(locators.map_or(false, |items| {
            items.iter().any(|locator| {
                locator.session_id == reference.session_id
                    && locator.question_id == reference.question_id
                    && locator.block_id == reference.block_id
                    && reference
                        .turn_id
                        .as_deref()
                        .is_none_or(|id| locator.turn_id == id)
                    && reference
                        .part_id
                        .as_deref()
                        .is_none_or(|id| locator.part_id.as_deref() == Some(id))
            })
        }))
    }
}

fn redact_recall_query(query: &str) -> AppResult<String> {
    let query = crate::backend::memory_redaction::redact_memory_text(query).text;
    let query = query.trim();
    if query.is_empty() {
        return Err(AppError::Validation("Recall clue is required".to_string()));
    }
    if query.chars().count() > MAX_RECALL_QUERY_CHARS {
        return Err(AppError::Validation("Recall clue is too long".to_string()));
    }
    Ok(query.to_string())
}

fn normalize_recall_id(value: &str, kind: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 160 || value.contains(['\n', '\r', '\0']) {
        return Err(AppError::Validation(format!("Recall {kind} id is invalid")));
    }
    Ok(value.to_string())
}

fn recall_conversation_session_id(session_id: &str) -> String {
    stable_recall_id("conversation-session", &[RECALL_SOURCE_ID, session_id])
}

fn recall_conversation_turn_id(session_id: &str, turn_id: &str) -> String {
    stable_recall_id("conversation-turn", &[session_id, turn_id])
}

fn stable_recall_id(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(b"\0");
    }
    format!("{prefix}-{:x}", hasher.finalize())
}

fn fingerprint_turns(turns: &[crate::backend::models::NormalizedConversationTurn]) -> String {
    let mut hasher = Sha256::new();
    for turn in turns {
        hasher.update(turn.external_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(turn.user_text.as_bytes());
        for part in &turn.parts {
            hasher.update(format!("{:?}:{:?}", part.role, part.kind).as_bytes());
            hasher.update(b"\0");
            hasher.update(part.text.as_deref().unwrap_or_default().as_bytes());
        }
    }
    format!("{:x}", hasher.finalize())
}

fn strip_json_fence(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.starts_with("```") && trimmed.ends_with("```") {
        let body = trimmed
            .trim_start_matches('`')
            .strip_prefix("json")
            .unwrap_or_else(|| trimmed.trim_start_matches('`'));
        return body.trim_end_matches('`').trim();
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::agents::types::AgentProtocol;
    use crate::backend::ai_execution::{
        executor::BackendFuture, AgentExecutionRuntime, AiExecutionRequest, AiExecutionResult,
    };
    use std::sync::Arc;

    #[test]
    fn recall_query_is_redacted_and_bounded() {
        let result = redact_recall_query(" Bearer abcdefghijklmnopqrstuvwxyz1234567890 ");
        assert!(result.is_ok());
        assert!(!result
            .unwrap()
            .contains("abcdefghijklmnopqrstuvwxyz1234567890"));
    }

    #[test]
    fn recall_json_fence_is_removed_without_changing_payload() {
        assert_eq!(strip_json_fence("```json\n{}\n```"), "{}");
    }

    struct FakeRecallRuntime;

    impl AgentExecutionRuntime for FakeRecallRuntime {
        fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
            Box::pin(async move {
                Ok(AiExecutionResult {
                    text: r#"{
                        "answer": "找到了一条相关线索。",
                        "sessionReferences": [],
                        "contentReferences": [],
                        "followUpSuggestions": ["继续缩小时间范围"]
                    }"#
                    .to_string(),
                    agent_id: request.agent_id,
                    protocol: AgentProtocol::Acp,
                    requested_model: request.model,
                    elapsed_ms: 1,
                    persistent_binding: None,
                    replay_text: None,
                })
            })
        }
    }

    #[test]
    fn recall_one_turn_returns_quickly_and_reopens_from_conversation_and_workflow_tables() {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-memory-recall-workflow-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create Recall fixture root");
        let db_path = root.join("app.db");
        let runtime: Arc<dyn AgentExecutionRuntime> = Arc::new(FakeRecallRuntime);
        let service = AppService::open_with_db_path_and_runtime(db_path.clone(), runtime.clone())
            .expect("open Recall fixture service");
        let invalid = parse_and_validate_recall_output(
            &service,
            "default",
            &MemoryScope::default(),
            r#"{
                "answer": "引用不存在的记录",
                "sessionReferences": [{"recordKind":"session","sessionId":"missing"}],
                "contentReferences": [],
                "followUpSuggestions": []
            }"#,
        );
        assert!(invalid
            .expect_err("invalid Recall reference should be rejected")
            .to_string()
            .contains("out-of-scope"));
        let session = service
            .create_memory_recall_session(MemoryRecallSessionCreateParams {
                scope: MemoryScope {
                    project_path: Some(root.to_string_lossy().into_owned()),
                    ..MemoryScope::default()
                },
            })
            .expect("create Recall session");
        let queued = service
            .send_memory_recall_turn(MemoryRecallTurnSendParams {
                session_id: session.id.clone(),
                query: "请找出上次关于发布的讨论".to_string(),
            })
            .expect("queue Recall turn");
        assert_eq!(queued.id, session.id);

        let completed = (0..100)
            .find_map(|_| {
                let current = service
                    .get_memory_recall_session(MemoryRecallSessionGetParams {
                        session_id: session.id.clone(),
                    })
                    .expect("read Recall session");
                if current
                    .turns
                    .first()
                    .is_some_and(|turn| turn.status == MemoryRecallTurnStatus::Completed)
                {
                    Some(current)
                } else {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    None
                }
            })
            .expect("Recall turn completion");
        let turn = completed.turns.first().expect("Recall turn");
        let output = turn
            .structured_output
            .as_ref()
            .expect("structured Recall output");
        assert_eq!(output.answer, "找到了一条相关线索。");
        assert!(output.session_references.is_empty());
        assert!(output.content_references.is_empty());
        assert_eq!(output.follow_up_suggestions, vec!["继续缩小时间范围"]);

        let counts = service.runtime.run_sync(async {
            let source_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM conversation_sources WHERE tenant_id='default' AND id='assetiweave-memory-recall'",
            )
            .fetch_one(service.db.pool())
            .await
            .map_err(AppError::external)?;
            let turn_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM conversation_turns WHERE tenant_id='default' AND session_id=?1",
            )
            .bind(&turn.conversation_session_id)
            .fetch_one(service.db.pool())
            .await
            .map_err(AppError::external)?;
            Ok::<_, AppError>((source_count, turn_count))
        });
        assert_eq!(counts.expect("count Recall Conversation rows"), (1, 1));
        assert_eq!(completed.status, MemoryRecallSessionStatus::Active);

        drop(service);
        let reopened = AppService::open_with_db_path_and_runtime(db_path.clone(), runtime)
            .expect("reopen Recall fixture service");
        let restored = reopened
            .get_memory_recall_session(MemoryRecallSessionGetParams {
                session_id: session.id,
            })
            .expect("read restored Recall session");
        assert_eq!(restored.turns.len(), 1);
        assert_eq!(
            restored.turns[0]
                .structured_output
                .as_ref()
                .expect("restored Recall output")
                .answer,
            "找到了一条相关线索。"
        );
        drop(reopened);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn recall_session_supports_sequential_turns_without_replaying_completed_turns() {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-memory-recall-multiturn-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create multi-turn fixture root");
        let db_path = root.join("app.db");
        let runtime: Arc<dyn AgentExecutionRuntime> = Arc::new(FakeRecallRuntime);
        let service = AppService::open_with_db_path_and_runtime(db_path.clone(), runtime)
            .expect("open multi-turn service");
        let session = service
            .create_memory_recall_session(MemoryRecallSessionCreateParams::default())
            .expect("create multi-turn session");

        for (index, query) in ["先找发布讨论", "再找更早的那次"].into_iter().enumerate()
        {
            service
                .send_memory_recall_turn(MemoryRecallTurnSendParams {
                    session_id: session.id.clone(),
                    query: query.to_string(),
                })
                .expect("send sequential Recall turn");
            wait_for_recall_turn(&service, &session.id, index + 1);
        }

        let restored = service
            .get_memory_recall_session(MemoryRecallSessionGetParams {
                session_id: session.id,
            })
            .expect("read multi-turn session");
        assert_eq!(restored.turns.len(), 2);
        assert_eq!(restored.turns[0].user_text, "先找发布讨论");
        assert_eq!(restored.turns[1].user_text, "再找更早的那次");
        assert!(restored
            .turns
            .iter()
            .all(|turn| turn.status == MemoryRecallTurnStatus::Completed));

        drop(service);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn recall_cancel_is_durable_and_does_not_allow_late_agent_output() {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-memory-recall-cancel-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create cancellation fixture root");
        let db_path = root.join("app.db");
        let runtime: Arc<dyn AgentExecutionRuntime> = Arc::new(BlockingRecallRuntime);
        let service = AppService::open_with_db_path_and_runtime(db_path, runtime)
            .expect("open cancellation service");
        let session = service
            .create_memory_recall_session(MemoryRecallSessionCreateParams::default())
            .expect("create cancellation session");
        let queued = service
            .send_memory_recall_turn(MemoryRecallTurnSendParams {
                session_id: session.id.clone(),
                query: "等待取消".to_string(),
            })
            .expect("send cancellable Recall turn");
        let turn_id = queued.active_turn_id.expect("active turn");
        service
            .cancel_memory_recall_turn(MemoryRecallTurnCancelParams { turn_id })
            .expect("cancel Recall turn");

        let cancelled = wait_for_recall_turn(&service, &session.id, 1);
        assert_eq!(cancelled.turns[0].status, MemoryRecallTurnStatus::Cancelled);
        assert!(cancelled.turns[0].structured_output.is_none());
        assert!(cancelled.active_turn_id.is_none());
        service
            .runtime
            .task_runtime()
            .shutdown_with_grace(std::time::Duration::from_secs(2));
        drop(service);
        std::fs::remove_dir_all(root).ok();
    }

    struct BlockingRecallRuntime;

    impl AgentExecutionRuntime for BlockingRecallRuntime {
        fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
            Box::pin(async move {
                request.cancellation.cancelled().await;
                Err(crate::backend::ai_execution::AiExecutionError::Cancelled {
                    program: std::path::PathBuf::from("recall-fixture"),
                })
            })
        }
    }

    fn wait_for_recall_turn(
        service: &AppService,
        session_id: &str,
        expected_turn_count: usize,
    ) -> MemoryRecallSession {
        (0..200)
            .find_map(|_| {
                let current = service
                    .get_memory_recall_session(MemoryRecallSessionGetParams {
                        session_id: session_id.to_string(),
                    })
                    .expect("read Recall session while waiting");
                if current.turns.len() >= expected_turn_count
                    && current
                        .turns
                        .last()
                        .is_some_and(|turn| turn.status.is_terminal())
                {
                    Some(current)
                } else {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    None
                }
            })
            .expect("Recall turn reached terminal state")
    }
}
