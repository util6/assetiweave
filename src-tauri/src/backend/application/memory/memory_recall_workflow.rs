use crate::backend::application::prelude::*;
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
use sha2::{Digest, Sha256};

pub(crate) const RECALL_SOURCE_ID: &str = "assetiweave-memory-recall";
pub(crate) const RECALL_ADAPTER_ID: &str = "assetiweave-memory-recall";
pub(crate) const MAX_RECALL_QUERY_CHARS: usize = 4_000;
pub(crate) const MAX_RECALL_ANSWER_CHARS: usize = 100_000;
pub(crate) const MAX_RECALL_REFERENCES: usize = 64;

pub(crate) use super::memory_recall_executor::*;
pub(crate) use super::memory_recall_prompt::*;

impl AppService {
    pub(crate) async fn load_memory_recall_block(
        &self,
        scope: &MemoryScope,
        reference: &MemoryRecallContentReference,
    ) -> AppResult<crate::backend::domain::conversations::ConversationBlockDetail> {
        let tenant_id = self.tenant_id();
        if !self
            .recall_content_reference_exists_for_scope(tenant_id, scope, reference)
            .await?
        {
            return Err(AppError::Validation(
                "Recall locator is not readable in this session scope".to_string(),
            ));
        }
        let record_kind = match reference.record_kind {
            MemoryRecordKind::Session => {
                crate::backend::domain::conversations::ConversationRecordKind::Session
            }
            MemoryRecordKind::Web => {
                crate::backend::domain::conversations::ConversationRecordKind::Web
            }
        };
        let locators = crate::backend::store::list_conversation_block_locators_sqlx(
            self.db.pool(),
            tenant_id,
            record_kind,
            &reference.question_id,
        )
        .await?;
        let Some(locator) = locators.into_iter().find(|locator| {
            locator.session_id == reference.session_id
                && locator.block_id == reference.block_id
                && reference
                    .turn_id
                    .as_deref()
                    .is_none_or(|id| locator.turn_id == id)
                && reference
                    .part_id
                    .as_deref()
                    .is_none_or(|id| locator.part_id.as_deref() == Some(id))
        }) else {
            return Err(AppError::NotFound(
                "Recall locator is not readable in this tenant".to_string(),
            ));
        };
        Ok(crate::backend::store::load_conversation_block_detail_sqlx(
            self.db.pool(),
            tenant_id,
            record_kind,
            &locator.block_id,
        )
        .await?)
    }

    pub(crate) async fn create_memory_recall_session(
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
        crate::backend::store::create_memory_recall_session_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &session,
        )
        .await?;
        Ok(session)
    }

    pub(crate) async fn get_memory_recall_session(
        &self,
        params: MemoryRecallSessionGetParams,
    ) -> AppResult<MemoryRecallSession> {
        let session_id = normalize_recall_id(&params.session_id, "session")?;
        crate::backend::store::load_memory_recall_session_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &session_id,
        )
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Recall session not found: {session_id}")))
    }

    pub(crate) async fn send_memory_recall_turn(
        &self,
        params: MemoryRecallTurnSendParams,
    ) -> AppResult<MemoryRecallSession> {
        let session_id = normalize_recall_id(&params.session_id, "session")?;
        let query = redact_recall_query(&params.query)?;
        let session = self
            .get_memory_recall_session(MemoryRecallSessionGetParams {
                session_id: session_id.clone(),
            })
            .await?;
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
        crate::backend::store::create_memory_recall_turn_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &turn,
        )
        .await?;

        let mut history = session.turns.clone();
        history.push(turn.clone());
        if let Err(error) = self
            .persist_recall_conversation(self.tenant_id(), &session, &history)
            .await
        {
            let _ = crate::backend::store::fail_memory_recall_turn_sqlx(
                self.db.pool(),
                self.tenant_id(),
                &turn.id,
                MemoryRecallTurnStatus::Failed,
                &error.to_string(),
            )
            .await;
            return Err(error);
        }

        self.schedule_memory_recall_turn_for_tenant(self.tenant_id(), &turn.id)
            .await?;
        self.get_memory_recall_session(MemoryRecallSessionGetParams { session_id })
            .await
    }

    pub(crate) async fn cancel_memory_recall_turn(
        &self,
        params: MemoryRecallTurnCancelParams,
    ) -> AppResult<MemoryRecallSession> {
        let turn_id = normalize_recall_id(&params.turn_id, "turn")?;
        let turn = crate::backend::store::load_memory_recall_turn_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &turn_id,
        )
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Recall turn not found: {turn_id}")))?;
        let task_id = format!("memory-recall:{turn_id}");
        crate::backend::store::fail_memory_recall_turn_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &turn.id,
            MemoryRecallTurnStatus::Cancelled,
            "Recall turn cancelled by user",
        )
        .await?;
        let _ = self.runtime.task_runtime().cancel(&task_id);
        self.get_memory_recall_session(MemoryRecallSessionGetParams {
            session_id: turn.session_id,
        })
        .await
    }

    pub(crate) async fn recover_memory_recall_turns_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<usize> {
        let turns = crate::backend::store::list_memory_recall_turns_for_recovery_sqlx(
            self.db.pool(),
            tenant_id,
        )
        .await?;
        let mut scheduled = 0;
        for (turn_id, status) in turns {
            if status == MemoryRecallTurnStatus::Running {
                crate::backend::store::fail_memory_recall_turn_sqlx(
                    self.db.pool(),
                    tenant_id,
                    &turn_id,
                    MemoryRecallTurnStatus::ResumeUnavailable,
                    "Recall provider execution was interrupted before restart",
                )
                .await?;
                continue;
            }
            self.schedule_memory_recall_turn_for_tenant(tenant_id, &turn_id)
                .await?;
            scheduled += 1;
        }
        Ok(scheduled)
    }
}

impl AppService {
    pub(crate) async fn recall_session_reference_exists_for_scope(
        &self,
        tenant_id: &str,
        scope: &MemoryScope,
        reference: &MemoryRecallSessionReference,
    ) -> AppResult<bool> {
        let record_kind = match reference.record_kind {
            MemoryRecordKind::Session => {
                crate::backend::domain::conversations::ConversationRecordKind::Session
            }
            MemoryRecordKind::Web => {
                crate::backend::domain::conversations::ConversationRecordKind::Web
            }
        };
        let session_exists = match record_kind {
            crate::backend::domain::conversations::ConversationRecordKind::Session => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT EXISTS(SELECT 1 FROM conversation_sessions s JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE s.tenant_id=?1 AND s.id=?2 AND s.missing=0 AND source.enabled=1 AND source.adapter_id <> 'assetiweave-memory-recall' AND (?3 IS NULL OR s.adapter_id=?3) AND (?4 IS NULL OR s.source_id=?4) AND (?5 IS NULL OR s.project_path=?5) AND (?6 IS NULL OR s.id=?6))",
                )
                .bind(tenant_id)
                .bind(&reference.session_id)
                .bind(&scope.app_id)
                .bind(&scope.source_id)
                .bind(&scope.project_path)
                .bind(&scope.session_id)
                .fetch_one(self.db.pool())
                .await
                .map_err(AppError::external)?
            }
            crate::backend::domain::conversations::ConversationRecordKind::Web => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT EXISTS(SELECT 1 FROM web_record_sessions s JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE s.tenant_id=?1 AND s.id=?2 AND s.missing=0 AND source.enabled=1 AND source.adapter_id <> 'assetiweave-memory-recall' AND (?3 IS NULL OR s.adapter_id=?3) AND (?4 IS NULL OR s.source_id=?4) AND (?5 IS NULL OR s.id=?5))",
                )
                .bind(tenant_id)
                .bind(&reference.session_id)
                .bind(&scope.app_id)
                .bind(&scope.source_id)
                .bind(&scope.session_id)
                .fetch_one(self.db.pool())
                .await
                .map_err(AppError::external)?
            }
        };
        if session_exists == 0 {
            return Ok(false);
        }
        let Some(ref question_id) = reference.question_id else {
            return Ok(true);
        };
        let exists = match record_kind {
            crate::backend::domain::conversations::ConversationRecordKind::Session => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT EXISTS(SELECT 1 FROM conversation_questions WHERE tenant_id=?1 AND id=?2 AND session_id=?3)",
                )
            }
            crate::backend::domain::conversations::ConversationRecordKind::Web => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT EXISTS(SELECT 1 FROM web_record_questions WHERE tenant_id=?1 AND id=?2 AND session_id=?3)",
                )
            }
        }
        .bind(tenant_id)
        .bind(question_id)
        .bind(&reference.session_id)
        .fetch_one(self.db.pool())
        .await
        .map_err(AppError::external)?;
        Ok(exists != 0)
    }

    pub(crate) async fn recall_content_reference_exists_for_scope(
        &self,
        tenant_id: &str,
        scope: &MemoryScope,
        reference: &MemoryRecallContentReference,
    ) -> AppResult<bool> {
        let record_kind = match reference.record_kind {
            MemoryRecordKind::Session => {
                crate::backend::domain::conversations::ConversationRecordKind::Session
            }
            MemoryRecordKind::Web => {
                crate::backend::domain::conversations::ConversationRecordKind::Web
            }
        };
        let parent_exists = match record_kind {
            crate::backend::domain::conversations::ConversationRecordKind::Session => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT EXISTS(SELECT 1 FROM conversation_questions q JOIN conversation_sessions s ON s.tenant_id=q.tenant_id AND s.id=q.session_id JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE q.tenant_id=?1 AND q.id=?2 AND q.session_id=?3 AND s.missing=0 AND source.enabled=1 AND source.adapter_id <> 'assetiweave-memory-recall' AND (?4 IS NULL OR s.adapter_id=?4) AND (?5 IS NULL OR s.source_id=?5) AND (?6 IS NULL OR s.project_path=?6) AND (?7 IS NULL OR s.id=?7))",
                )
            }
            crate::backend::domain::conversations::ConversationRecordKind::Web => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT EXISTS(SELECT 1 FROM web_record_questions q JOIN web_record_sessions s ON s.tenant_id=q.tenant_id AND s.id=q.session_id JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE q.tenant_id=?1 AND q.id=?2 AND q.session_id=?3 AND s.missing=0 AND source.enabled=1 AND source.adapter_id <> 'assetiweave-memory-recall' AND (?4 IS NULL OR s.adapter_id=?4) AND (?5 IS NULL OR s.source_id=?5) AND (?7 IS NULL OR s.id=?7))",
                )
            }
        }
        .bind(tenant_id)
        .bind(&reference.question_id)
        .bind(&reference.session_id)
        .bind(&scope.app_id)
        .bind(&scope.source_id)
        .bind(&scope.project_path)
        .bind(&scope.session_id)
        .fetch_one(self.db.pool())
        .await
        .map_err(AppError::external)?;
        if parent_exists == 0 {
            return Ok(false);
        }
        let locators = crate::backend::store::list_conversation_block_locators_sqlx(
            self.db.pool(),
            tenant_id,
            record_kind,
            &reference.question_id,
        )
        .await?;
        Ok(locators.iter().any(|locator| {
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
        }))
    }
}

#[cfg(test)]
#[path = "memory_recall_workflow_tests.rs"]
mod tests;
