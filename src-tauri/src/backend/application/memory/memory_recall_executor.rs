use super::memory_recall_prompt::*;
use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::{
    application::memory::memory_agent_session::{
        ActiveMemoryAgentSession, MemoryAgentSessionParams,
    },
    domain::agents::AgentId,
    domain::agents::AgentSessionRef,
    domain::{ConversationPartKind, ConversationPartRole, ConversationSourceKind},
    infrastructure::agent_execution::{
        execute_agent, AgentSessionMode, AiExecutionCancellation, AiExecutionLimits,
        AiExecutionPurpose, AiExecutionRequest,
    },
    infrastructure::tasks::tasks::{StageStatus, TaskContext, TaskOutcome, TaskStage},
};

const RECALL_SOURCE_ID: &str = "assetiweave-memory-recall";
const RECALL_ADAPTER_ID: &str = "assetiweave-memory-recall";
const MAX_RECALL_QUERY_CHARS: usize = 4_000;
const MAX_RECALL_ANSWER_CHARS: usize = 100_000;
const MAX_RECALL_REFERENCES: usize = 64;

impl AppService {
    pub(crate) async fn schedule_memory_recall_turn_for_tenant(
        &self,
        tenant_id: &str,
        turn_id: &str,
    ) -> AppResult<()> {
        let runtime = self.runtime.clone();
        let tenant_id_for_task = tenant_id.to_string();
        let turn_id_for_task = turn_id.to_string();
        let task_id = format!("memory-recall:{turn_id}");
        let task_runtime = runtime.task_runtime().clone();
        let mut spec = crate::backend::infrastructure::tasks::TaskSpec::new(
            crate::backend::infrastructure::tasks::TaskKind::Memory,
            Some(task_id),
        )
        .with_tenant_id(tenant_id.to_string())
        .with_conflict_key(format!("memory-recall-session:{tenant_id}:{turn_id}"));
        spec.detail = serde_json::json!({
            "domain": "memory_recall",
            "job_id": turn_id,
        });
        let spawn = task_runtime.spawn_async(spec, move |context| async move {
            let service = AppService::from_runtime(&runtime)
                .for_tenant(&tenant_id_for_task)
                .await?;
            if context.is_cancelled() {
                let _ = crate::backend::store::fail_memory_recall_turn_sqlx(
                    service.db.pool(),
                    &tenant_id_for_task,
                    &turn_id_for_task,
                    MemoryRecallTurnStatus::Cancelled,
                    "Recall task cancelled before execution",
                )
                .await;
                return Err(AppError::Cancelled("Recall task cancelled".to_string()));
            }
            service
                .run_memory_recall_turn_for_tenant(&tenant_id_for_task, &turn_id_for_task, context)
                .await
        });
        match spawn {
            Ok(crate::backend::infrastructure::tasks::SpawnOutcome::Started)
            | Ok(crate::backend::infrastructure::tasks::SpawnOutcome::Existing) => Ok(()),
            Err(error) => {
                let _ = crate::backend::store::fail_memory_recall_turn_sqlx(
                    self.db.pool(),
                    tenant_id,
                    turn_id,
                    MemoryRecallTurnStatus::Failed,
                    &error.to_string(),
                )
                .await;
                Err(error.into())
            }
        }
    }

    async fn run_memory_recall_turn_for_tenant(
        &self,
        tenant_id: &str,
        turn_id: &str,
        context: TaskContext,
    ) -> AppResult<Value> {
        let cancellation = AiExecutionCancellation::from_token(context.cancellation());
        let turn =
            crate::backend::store::load_memory_recall_turn_sqlx(self.db.pool(), tenant_id, turn_id)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("Recall turn not found: {turn_id}")))?;
        if cancellation.is_cancelled() {
            let _ = crate::backend::store::fail_memory_recall_turn_sqlx(
                self.db.pool(),
                tenant_id,
                turn_id,
                MemoryRecallTurnStatus::Cancelled,
                "Recall task cancelled before execution",
            )
            .await;
            return Err(AppError::Cancelled("Recall task cancelled".to_string()));
        }
        crate::backend::store::mark_memory_recall_turn_running_sqlx(
            self.db.pool(),
            tenant_id,
            turn_id,
        )
        .await?;
        let session = crate::backend::store::load_memory_recall_session_sqlx(
            self.db.pool(),
            tenant_id,
            &turn.session_id,
        )
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!("Recall session not found: {}", turn.session_id))
        })?;

        let progress = context.progress();
        let stages = vec![TaskStage {
            id: "agent_execution".to_string(),
            name: "执行记忆召回 Agent".to_string(),
            status: StageStatus::Pending,
            started_at: None,
            finished_at: None,
            duration_ms: None,
            progress: None,
            current_activities: Vec::new(),
            metrics: Vec::new(),
            failures: Vec::new(),
            skipped: Vec::new(),
            agent_session_ref: None,
            steps: Vec::new(),
        }];
        progress.set_stages(stages);

        let session_ref = AgentSessionRef {
            schema_version: 1,
            value: format!("agent-session://recall/{}", session.id),
        };
        let memory_session = ActiveMemoryAgentSession::start(
            &self.runtime,
            MemoryAgentSessionParams {
                tenant_id,
                scope: "recall",
                job_id: &session.id,
                task_id: Some(context.task_id()),
                agent_id: &session.agent_id,
                display_name: Some("Memory Recall Agent".to_string()),
                model: session.model.clone(),
                prompt_summary: &turn.user_text,
                custom_session_ref: Some(session_ref.clone()),
                persistent: true,
            },
        );
        progress.set_stage_agent_session_ref("agent_execution", Some(session_ref.clone()));
        progress.update_stage_status("agent_execution", StageStatus::Running);

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
            progress: Some(memory_session.sink.clone()),
            tenant_id: Some(tenant_id.to_string()),
            execution_context_key: Some(session.execution_context_key.clone()),
            binding: None,
            replay: false,
            restore_only: false,
            recall_tools: Some(
                crate::backend::infrastructure::agent_execution::AiRecallTools {
                    tenant_id: tenant_id.to_string(),
                    recall_session_id: session.id.clone(),
                    database_path: self.db_path.to_string_lossy().into_owned(),
                },
            ),
            memory_generation_tools: None,
        };
        let agent_runtime = self.agent_runtime.clone();
        let execution_result = execute_agent(agent_runtime, request).await;
        let result = match execution_result {
            Ok(result) => {
                memory_session.finish_succeeded(&self.runtime);
                progress.finish_stage(
                    "agent_execution",
                    StageStatus::Succeeded,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                );
                result
            }
            Err(error) => {
                let view = error.to_view();
                let status = if view.code == "resume_unavailable" {
                    MemoryRecallTurnStatus::ResumeUnavailable
                } else if view.code == "cancelled" || view.code == "canceled" {
                    MemoryRecallTurnStatus::Cancelled
                } else {
                    MemoryRecallTurnStatus::Failed
                };
                let _ = crate::backend::store::fail_memory_recall_turn_sqlx(
                    self.db.pool(),
                    tenant_id,
                    turn_id,
                    status,
                    &view.message,
                )
                .await;
                if status == MemoryRecallTurnStatus::Cancelled {
                    memory_session.finish_cancelled(&self.runtime);
                    progress.finish_stage(
                        "agent_execution",
                        StageStatus::Canceled,
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                    );
                    progress.set_outcome(
                        TaskOutcome::Canceled,
                        None,
                        Some("任务已被取消".to_string()),
                    );
                } else {
                    memory_session.finish_failed(&self.runtime, &view.code, &view.message, false);
                    progress.finish_stage(
                        "agent_execution",
                        StageStatus::Failed,
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                    );
                    progress.set_outcome(
                        TaskOutcome::Failure,
                        Some(view.code.clone()),
                        Some(view.message.clone()),
                    );
                }
                return Err(AppError::Domain {
                    code: view.code,
                    message: view.message,
                    retryable: view.retryable,
                    details: None,
                });
            }
        };
        if cancellation.is_cancelled() {
            memory_session.finish_cancelled(&self.runtime);
            progress.finish_stage(
                "agent_execution",
                StageStatus::Canceled,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            progress.set_outcome(
                TaskOutcome::Canceled,
                None,
                Some("任务已被取消".to_string()),
            );
            let _ = crate::backend::store::fail_memory_recall_turn_sqlx(
                self.db.pool(),
                tenant_id,
                turn_id,
                MemoryRecallTurnStatus::Cancelled,
                "Recall task cancelled during execution",
            )
            .await;
            return Err(AppError::Cancelled("Recall task cancelled".to_string()));
        }
        let output =
            match parse_and_validate_recall_output(self, tenant_id, &session.scope, &result.text)
                .await
            {
                Ok(output) => output,
                Err(error) => {
                    let _ = crate::backend::store::fail_memory_recall_turn_sqlx(
                        self.db.pool(),
                        tenant_id,
                        turn_id,
                        MemoryRecallTurnStatus::Failed,
                        &error.to_string(),
                    )
                    .await;
                    return Err(error);
                }
            };
        let current_status =
            crate::backend::store::load_memory_recall_turn_sqlx(self.db.pool(), tenant_id, turn_id)
                .await?
                .map(|turn| turn.status);
        if current_status != Some(MemoryRecallTurnStatus::Running) {
            return Err(AppError::Cancelled(
                "Recall turn is no longer active".to_string(),
            ));
        }
        let session_after = crate::backend::store::load_memory_recall_session_sqlx(
            self.db.pool(),
            tenant_id,
            &session.id,
        )
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Recall session not found: {}", session.id)))?;
        let mut history = session_after.turns.clone();
        let current = history
            .iter_mut()
            .find(|item| item.id == turn_id)
            .ok_or_else(|| AppError::NotFound(format!("Recall turn not found: {turn_id}")))?;
        current.structured_output = Some(output.clone());
        current.status = MemoryRecallTurnStatus::Completed;
        self.persist_recall_conversation(tenant_id, &session_after, &history)
            .await?;
        self.record_recall_usage(tenant_id, turn_id, &output)
            .await?;
        crate::backend::store::complete_memory_recall_turn_sqlx(
            self.db.pool(),
            tenant_id,
            turn_id,
            &output,
        )
        .await?;
        progress.set_outcome(TaskOutcome::Success, None, None);
        Ok(serde_json::json!({
            "turnId": turn_id,
            "status": "completed"
        }))
    }

    async fn record_recall_usage(
        &self,
        tenant_id: &str,
        turn_id: &str,
        output: &MemoryRecallStructuredOutput,
    ) -> AppResult<()> {
        if !self.backend_settings()?.is_memory_usage_enabled() {
            return Ok(());
        }
        let used_at = Utc::now().to_rfc3339();
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
        Ok(())
    }

    pub(crate) async fn persist_recall_conversation(
        &self,
        tenant_id: &str,
        session: &MemoryRecallSession,
        turns: &[MemoryRecallTurn],
    ) -> AppResult<()> {
        let now = Utc::now().to_rfc3339();
        let source = crate::backend::domain::ConversationSource {
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
                let mut parts = vec![crate::backend::domain::NormalizedConversationPart {
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
                    parts.push(crate::backend::domain::NormalizedConversationPart {
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
                            crate::backend::domain::ConversationContentCardDescriptor {
                                schema_version: 1,
                                kind: "answer".to_string(),
                                renderer: Some("markdown".to_string()),
                            },
                        ),
                        metadata_json: None,
                    });
                }
                crate::backend::domain::NormalizedConversationTurn {
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
        let normalized = crate::backend::domain::NormalizedConversationSession {
            external_id: session.id.clone(),
            title: Some("Recall".to_string()),
            project_path: session.scope.project_path.clone(),
            started_at: Some(session.created_at.clone()),
            updated_at: Some(now.clone()),
            source_locator: Some(format!("memory-recall://{}", session.id)),
            source_fingerprint: Some(fingerprint),
            execution_origin: Some("internal_memory".to_string()),
            execution_purpose: Some("recall".to_string()),
            user_visible: Some(false),
            turns: normalized_turns,
        };
        crate::backend::application::conversations::conversation_sources::save_source(
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
        .map_err(Into::into)
    }

    pub(crate) fn resolve_recall_assignment(&self) -> AppResult<(AgentId, Option<String>)> {
        let settings = self.app_settings_value();
        crate::backend::application::agents::composition::resolve_agent_for(
            &crate::backend::domain::agents::ActionId::new("memory.recall"),
            &settings,
        )
    }
}
