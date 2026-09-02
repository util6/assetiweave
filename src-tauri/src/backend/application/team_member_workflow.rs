use crate::backend::{
    agents::types::AgentId,
    ai_execution::{
        execute_agent_blocking, AgentSessionMode, AiExecutionCancellation, AiExecutionError,
        AiExecutionPhase, AiExecutionProgressSink, AiExecutionPurpose, AiExecutionRequest,
        SessionEvent, SessionEventDelivery, SessionEventIdentity, SessionEventKind,
        SessionEventProjection, SessionItemKind, SessionSnapshot,
    },
    application::AppService,
    models::{TeamMember, TeamMemberTurnInput},
    runtime::{
        session_streams::SessionStreamKey,
        tasks::{ExternalRegistrationOutcome, TaskKind, TaskSnapshot, TaskSpec},
        AppError, AppResult,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use tokio::sync::broadcast;
use uuid::Uuid;

const MEMBER_TURN_WORKFLOW: &str = "team_member_turn";
const MAX_CONTEXT_KEY_BYTES: usize = 256;
const MAX_PROVIDER_ANCHOR_BYTES: usize = 256;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct TeamMemberStreamSnapshot {
    pub(crate) team_id: String,
    pub(crate) member_id: String,
    pub(crate) execution_id: String,
    /// Monotonic projection revision used by transport consumers for
    /// reconnect/dedup decisions. It is deliberately separate from task
    /// lifecycle state, which may change without adding a Session item.
    pub(crate) sequence: u64,
    pub(crate) replay: bool,
    pub(crate) task: TaskSnapshot,
    pub(crate) stream: SessionSnapshot,
}

impl AppService {
    /// Start either a live turn or a history replay through the same
    /// member-scoped workflow. The returned snapshot is intentionally the
    /// pre-completion view; provider work is owned by TaskRuntime.
    pub(crate) fn start_member_turn(
        &self,
        input: TeamMemberTurnInput,
    ) -> AppResult<TeamMemberStreamSnapshot> {
        let member = self.validate_member_turn_input(&input)?;
        let binding = self.load_member_binding(&member, input.replay)?;
        let agent_id = AgentId::parse(member.agent_id.clone())
            .map_err(|error| AppError::Validation(error.to_string()))?;
        let execution_id = format!("team-member-exec-{}", Uuid::new_v4().simple());
        let task_id = format!("team-member-turn-{execution_id}");
        let tenant_id = self.tenant_id().to_string();
        let spec = TaskSpec::new(
            TaskKind::TeamRun,
            Some(format!(
                "{MEMBER_TURN_WORKFLOW}:{}:{}:{}",
                input.team_id, member.id, input.replay
            )),
        )
        .with_task_id(task_id.clone())
        .with_tenant_id(tenant_id.clone())
        .with_conflict_key(format!(
            "{MEMBER_TURN_WORKFLOW}:{}:{}",
            input.team_id, member.id
        ));
        let registration = self.runtime.task_runtime().register_external(spec)?;
        match registration {
            ExternalRegistrationOutcome::Started(_) => {}
            ExternalRegistrationOutcome::Existing(snapshot) => {
                let existing_key =
                    self.member_stream_key_from_task(&input.team_id, &member, &snapshot)?;
                return self.member_stream_snapshot(&existing_key, snapshot);
            }
            ExternalRegistrationOutcome::Conflict(snapshot) => {
                return Err(AppError::Conflict(format!(
                    "An active Team member turn already exists for member {} ({})",
                    member.id, snapshot.task_id
                )));
            }
        }
        let key = SessionStreamKey {
            tenant_id: tenant_id.clone(),
            team_id: input.team_id.clone(),
            member_id: member.id.clone(),
            execution_id: execution_id.clone(),
        };
        let projection = self.runtime.session_streams().register(key.clone());
        let workflow = MemberTurnProgressSink::new(
            self.runtime.task_runtime().clone(),
            task_id.clone(),
            projection,
            key.clone(),
            input.replay,
        );
        if !input.replay {
            workflow.emit_control(
                "user",
                SessionEventKind::UserMessageAcknowledged { accepted: true },
            );
        }

        let agent_runtime = self.agent_runtime.clone();
        let runtime_for_worker = self.runtime.clone();
        let prompt = input.message.trim().to_string();
        let team_id = input.team_id.clone();
        let member_id = member.id.clone();
        let context_key = member.execution_context_key.clone();
        let model = member.model.clone();
        let replay = input.replay;
        let execution_id_for_worker = execution_id.clone();
        let key_for_worker = key.clone();
        let sink = workflow.clone();
        let started = self.runtime.task_runtime().start_external_with(
            &task_id,
            member_task_detail(
                &self.tenant_id().to_string(),
                &team_id,
                &member_id,
                &execution_id,
                replay,
                "running",
            ),
            Box::new(move |context| {
                if context.is_cancelled() {
                    sink.finish_cancelled();
                    runtime_for_worker
                        .session_streams()
                        .mark_terminal(&key_for_worker);
                    return Err(AppError::Canceled(
                        "Team member turn was cancelled before execution".to_string(),
                    ));
                }

                sink.set_phase(AiExecutionPhase::Queued);
                let request = AiExecutionRequest {
                    execution_id: execution_id_for_worker.clone(),
                    agent_id,
                    purpose: AiExecutionPurpose::TeamMemberTurn,
                    session_mode: AgentSessionMode::Persistent,
                    prompt,
                    model,
                    limits: Default::default(),
                    cancellation: AiExecutionCancellation::from_token(context.cancellation()),
                    progress: Some(Arc::new(sink.clone())),
                    tenant_id: Some(tenant_id),
                    execution_context_key: Some(context_key),
                    binding,
                    replay,
                    restore_only: false,
                    team_tools: None,
                    recall_tools: None,
                };
                let result = execute_agent_blocking(agent_runtime, request);
                match result {
                    Ok(_result) => {
                        if context.is_cancelled() {
                            sink.finish_cancelled();
                            runtime_for_worker
                                .session_streams()
                                .mark_terminal(&key_for_worker);
                            return Err(AppError::Canceled(
                                "Team member turn was cancelled".to_string(),
                            ));
                        }
                        sink.finish_succeeded();
                        runtime_for_worker
                            .session_streams()
                            .mark_terminal(&key_for_worker);
                        Ok(member_task_result(
                            &team_id,
                            &member_id,
                            &execution_id_for_worker,
                            replay,
                        ))
                    }
                    Err(error) => {
                        let view = error.to_view();
                        if context.is_cancelled() || view.code == "cancelled" {
                            sink.finish_cancelled();
                            runtime_for_worker
                                .session_streams()
                                .mark_terminal(&key_for_worker);
                            Err(AppError::Canceled(
                                "Team member turn was cancelled".to_string(),
                            ))
                        } else {
                            sink.finish_failed(&view.code, view.retryable);
                            runtime_for_worker
                                .session_streams()
                                .mark_terminal(&key_for_worker);
                            Err(ai_error(error))
                        }
                    }
                }
            }),
        );
        let started = match started {
            Ok(snapshot) => snapshot,
            Err(error) => {
                workflow.finish_failed(&error.code(), error.retryable());
                self.runtime.session_streams().mark_terminal(&key);
                return Err(error);
            }
        };
        if started.state.is_terminal() {
            if started.state == crate::backend::runtime::tasks::TaskState::Canceled {
                workflow.finish_cancelled();
            } else if let Some(error) = started.error.as_ref() {
                workflow.finish_failed(&error.code, error.retryable);
            }
            self.runtime.session_streams().mark_terminal(&key);
        }
        self.member_stream_snapshot(&key, started)
    }

    /// Explicit alias used by application callers that prefer the Team
    /// aggregate name. Both Leader and Teammate calls reach `start_member_turn`.
    pub(crate) fn start_team_member_turn(
        &self,
        input: TeamMemberTurnInput,
    ) -> AppResult<TeamMemberStreamSnapshot> {
        self.start_member_turn(input)
    }

    pub(crate) fn start_member_replay(
        &self,
        team_id: &str,
        member_id: &str,
    ) -> AppResult<TeamMemberStreamSnapshot> {
        self.start_member_turn(TeamMemberTurnInput {
            team_id: team_id.to_string(),
            member_id: member_id.to_string(),
            message: String::new(),
            replay: true,
        })
    }

    pub(crate) fn get_member_stream(
        &self,
        team_id: &str,
        member_id: &str,
        execution_id: &str,
    ) -> AppResult<Option<TeamMemberStreamSnapshot>> {
        let member = self.validate_member_scope(team_id, member_id)?;
        let key = self.member_stream_key(team_id, &member, execution_id)?;
        let Some(task) = self.get_member_turn_task_by_execution(&key)? else {
            return Ok(None);
        };
        self.member_stream_snapshot(&key, task).map(Some)
    }

    /// Subscribe to the process-local projection for a validated member
    /// scope. Transport adapters use this wrapper instead of reaching into
    /// `SessionStreamRegistry` or reproducing its key validation.
    pub(crate) fn subscribe_member_stream(
        &self,
        team_id: &str,
        member_id: &str,
        execution_id: &str,
    ) -> AppResult<Option<broadcast::Receiver<SessionSnapshot>>> {
        let member = self.validate_member_scope(team_id, member_id)?;
        let key = self.member_stream_key(team_id, &member, execution_id)?;
        Ok(self.runtime.session_streams().subscribe(&key))
    }

    /// Project a TaskRuntime update into the same safe member stream payload
    /// used by start and polling calls. TaskRuntime owns lifecycle state;
    /// this method only reads the transient stream projection for event
    /// emission and never persists Session content.
    pub(crate) fn member_stream_event_for_task(
        &self,
        task: &TaskSnapshot,
    ) -> Option<TeamMemberStreamSnapshot> {
        if !is_member_turn_task(task)
            || task
                .tenant_id
                .as_deref()
                .is_some_and(|tenant| tenant != self.tenant_id())
        {
            return None;
        }
        let team_id = task.detail.get("team_id").and_then(Value::as_str)?;
        let member_id = task.detail.get("member_id").and_then(Value::as_str)?;
        let execution_id = task.detail.get("execution_id").and_then(Value::as_str)?;
        let key = SessionStreamKey {
            tenant_id: self.tenant_id().to_string(),
            team_id: team_id.to_string(),
            member_id: member_id.to_string(),
            execution_id: execution_id.to_string(),
        };
        self.member_stream_snapshot(&key, task.clone()).ok()
    }

    pub(crate) fn get_member_turn_task(&self, task_id: &str) -> AppResult<Option<TaskSnapshot>> {
        Ok(self
            .runtime
            .task_runtime()
            .get_for_tenant(self.tenant_id(), task_id)
            .filter(|snapshot| is_member_turn_task(snapshot)))
    }

    pub(crate) fn list_member_turn_tasks(&self) -> AppResult<Vec<TaskSnapshot>> {
        Ok(self
            .runtime
            .task_runtime()
            .list_for_tenant(
                self.tenant_id(),
                crate::backend::runtime::tasks::TaskFilter {
                    kind: Some(TaskKind::TeamRun),
                    active_only: false,
                },
            )
            .into_iter()
            .filter(is_member_turn_task)
            .collect())
    }

    pub(crate) fn cancel_member_turn(
        &self,
        team_id: &str,
        member_id: &str,
        execution_id: &str,
    ) -> AppResult<TeamMemberStreamSnapshot> {
        let member = self.validate_member_scope(team_id, member_id)?;
        let key = self.member_stream_key(team_id, &member, execution_id)?;
        let task = self
            .get_member_turn_task_by_execution(&key)?
            .ok_or_else(|| AppError::NotFound(format!("Member turn not found: {execution_id}")))?;
        let task = match self
            .runtime
            .task_runtime()
            .cancel_for_tenant(self.tenant_id(), &task.task_id)
        {
            crate::backend::runtime::tasks::CancelOutcome::Requested(snapshot)
            | crate::backend::runtime::tasks::CancelOutcome::AlreadyFinished(snapshot) => snapshot,
            crate::backend::runtime::tasks::CancelOutcome::NotFound => {
                return Err(AppError::NotFound(format!(
                    "Member turn not found: {execution_id}"
                )))
            }
        };
        self.member_stream_snapshot(&key, task)
    }

    fn validate_member_turn_input(&self, input: &TeamMemberTurnInput) -> AppResult<TeamMember> {
        let member = self.validate_member_scope(&input.team_id, &input.member_id)?;
        let agent_id = AgentId::parse(member.agent_id.clone())
            .map_err(|error| AppError::Validation(error.to_string()))?;
        let capabilities = self
            .agent_runtime
            .agent_capabilities(&agent_id)
            .ok_or_else(|| member_capability_error(&member, &["capabilities_unavailable"]))?;
        let mut missing = Vec::new();
        if !capabilities.resume {
            missing.push("resume");
        }
        if input.replay {
            if !capabilities.history_replay {
                missing.push("history_replay");
            }
        } else {
            if !capabilities.text_prompt {
                missing.push("text_prompt");
            }
            if !capabilities.live_events {
                missing.push("live_events");
            }
            if input.message.trim().is_empty() {
                return Err(AppError::Validation(
                    "Team member message is required".to_string(),
                ));
            }
        }
        if !missing.is_empty() {
            return Err(member_capability_error(&member, &missing));
        }
        validate_context_key(&member.execution_context_key)?;
        Ok(member)
    }

    fn validate_member_scope(&self, team_id: &str, member_id: &str) -> AppResult<TeamMember> {
        let team = self
            .get_team(team_id)?
            .ok_or_else(|| AppError::NotFound(format!("Team not found: {team_id}")))?;
        team.members
            .into_iter()
            .find(|member| member.id == member_id && member.team_id == team_id)
            .ok_or_else(|| AppError::NotFound(format!("Team member not found: {member_id}")))
    }

    fn load_member_binding(
        &self,
        member: &TeamMember,
        replay: bool,
    ) -> AppResult<Option<crate::backend::ai_execution::PersistentExecutionBinding>> {
        let store =
            crate::backend::ai_execution::PersistentBindingStore::new(self.db.pool().clone());
        let binding = self
            .runtime
            .run_sync(store.load(self.tenant_id(), &member.execution_context_key))?;
        let Some(binding) = binding else {
            if replay {
                return Err(AppError::Domain {
                    code: "team_member_anchor_unavailable".to_string(),
                    message: "The Team member provider session is unavailable for replay."
                        .to_string(),
                    retryable: false,
                    details: Some(json!({ "memberId": member.id })),
                });
            }
            return Ok(None);
        };
        validate_binding(member, self.tenant_id(), &binding)?;
        Ok(Some(binding))
    }

    fn member_stream_key(
        &self,
        team_id: &str,
        member: &TeamMember,
        execution_id: &str,
    ) -> AppResult<SessionStreamKey> {
        if execution_id.trim().is_empty() || execution_id.len() > MAX_CONTEXT_KEY_BYTES {
            return Err(AppError::Validation(
                "Member execution id is invalid".to_string(),
            ));
        }
        Ok(SessionStreamKey {
            tenant_id: self.tenant_id().to_string(),
            team_id: team_id.to_string(),
            member_id: member.id.clone(),
            execution_id: execution_id.to_string(),
        })
    }

    fn member_stream_key_from_task(
        &self,
        team_id: &str,
        member: &TeamMember,
        task: &TaskSnapshot,
    ) -> AppResult<SessionStreamKey> {
        let execution_id = task
            .detail
            .get("execution_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AppError::Conflict("Member turn task identity is invalid".to_string())
            })?;
        self.member_stream_key(team_id, member, execution_id)
    }

    fn get_member_turn_task_by_execution(
        &self,
        key: &SessionStreamKey,
    ) -> AppResult<Option<TaskSnapshot>> {
        Ok(self.list_member_turn_tasks()?.into_iter().find(|snapshot| {
            snapshot.detail.get("tenant_id").and_then(Value::as_str) == Some(key.tenant_id.as_str())
                && snapshot.detail.get("team_id").and_then(Value::as_str)
                    == Some(key.team_id.as_str())
                && snapshot.detail.get("member_id").and_then(Value::as_str)
                    == Some(key.member_id.as_str())
                && snapshot.detail.get("execution_id").and_then(Value::as_str)
                    == Some(key.execution_id.as_str())
        }))
    }

    fn member_stream_snapshot(
        &self,
        key: &SessionStreamKey,
        task: TaskSnapshot,
    ) -> AppResult<TeamMemberStreamSnapshot> {
        let stream = self
            .runtime
            .session_streams()
            .snapshot(key)
            .unwrap_or_else(|| SessionSnapshot {
                revision: 0,
                event_count: 0,
                items: Vec::new(),
            });
        let stream = public_session_snapshot(stream);
        Ok(TeamMemberStreamSnapshot {
            team_id: key.team_id.clone(),
            member_id: key.member_id.clone(),
            execution_id: key.execution_id.clone(),
            sequence: stream.revision,
            replay: task
                .detail
                .get("replay")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            task,
            stream,
        })
    }
}

/// Strip provider/tool detail before a member projection crosses a public
/// transport boundary. Assistant and thinking text remain available to the
/// chat workspace; tool items retain lifecycle state but never carry raw
/// provider payloads.
fn public_session_snapshot(mut snapshot: SessionSnapshot) -> SessionSnapshot {
    for item in &mut snapshot.items {
        if item.kind == SessionItemKind::Tool {
            item.text = None;
        }
    }
    snapshot
}

#[derive(Clone)]
struct MemberTurnProgressSink {
    task_runtime: crate::backend::runtime::tasks::TaskRuntime,
    task_id: String,
    projection: Arc<SessionEventProjection>,
    key: SessionStreamKey,
    replay: bool,
    sequence: Arc<AtomicU64>,
    terminal_emitted: Arc<AtomicBool>,
    phase: Arc<Mutex<Option<AiExecutionPhase>>>,
}

impl MemberTurnProgressSink {
    fn new(
        task_runtime: crate::backend::runtime::tasks::TaskRuntime,
        task_id: String,
        projection: Arc<SessionEventProjection>,
        key: SessionStreamKey,
        replay: bool,
    ) -> Self {
        Self {
            task_runtime,
            task_id,
            projection,
            key,
            replay,
            sequence: Arc::new(AtomicU64::new(0)),
            terminal_emitted: Arc::new(AtomicBool::new(false)),
            phase: Arc::new(Mutex::new(None)),
        }
    }

    fn emit_control(&self, suffix: &str, kind: SessionEventKind) {
        self.emit_session_event(SessionEvent {
            identity: SessionEventIdentity {
                session_id: self.key.member_id.clone(),
                member_id: self.key.member_id.clone(),
                execution_id: self.key.execution_id.clone(),
                turn_id: self.key.execution_id.clone(),
                item_id: format!("workflow:{suffix}"),
                event_id: format!("workflow:{}:{suffix}", self.key.execution_id),
            },
            sequence: 0,
            delivery: if self.replay {
                SessionEventDelivery::Replay
            } else {
                SessionEventDelivery::Live
            },
            kind,
        });
    }

    fn finish_succeeded(&self) {
        if !self.terminal_emitted.swap(true, Ordering::AcqRel) {
            self.emit_control("terminal", SessionEventKind::TerminalResult { text: None });
        }
    }

    fn finish_cancelled(&self) {
        if !self.terminal_emitted.swap(true, Ordering::AcqRel) {
            self.emit_control("cancel", SessionEventKind::Cancel);
        }
    }

    fn finish_failed(&self, code: &str, retryable: bool) {
        if !self.terminal_emitted.swap(true, Ordering::AcqRel) {
            self.emit_control(
                "error",
                SessionEventKind::Error {
                    code: code.to_string(),
                    retryable,
                },
            );
        }
    }

    fn safe_detail(&self, phase: Option<AiExecutionPhase>, cleanup: Option<Value>) -> Value {
        let mut detail = member_task_detail(
            &self.key.tenant_id,
            &self.key.team_id,
            &self.key.member_id,
            &self.key.execution_id,
            self.replay,
            &phase
                .map(|phase| format!("{phase:?}").to_ascii_lowercase())
                .unwrap_or_else(|| "running".to_string()),
        );
        if let Some(cleanup) = cleanup {
            detail["cleanup"] = cleanup;
        }
        detail
    }
}

impl AiExecutionProgressSink for MemberTurnProgressSink {
    fn set_phase(&self, phase: AiExecutionPhase) {
        if let Ok(mut current) = self.phase.lock() {
            *current = Some(phase);
        }
        let _ = self
            .task_runtime
            .update_detail(&self.task_id, self.safe_detail(Some(phase), None));
    }

    fn emit_session_event(&self, mut event: SessionEvent) {
        let observed_sequence = event.sequence;
        let mut current = self.sequence.load(Ordering::Acquire);
        let sequence = loop {
            let next = current.saturating_add(1).max(observed_sequence);
            match self
                .sequence
                .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => break next,
                Err(actual) => current = actual,
            }
        };
        event.sequence = sequence;
        event.identity.session_id = self.key.member_id.clone();
        event.identity.member_id = self.key.member_id.clone();
        event.identity.execution_id = self.key.execution_id.clone();
        event.identity.turn_id = self.key.execution_id.clone();
        if event.identity.item_id.trim().is_empty() {
            event.identity.item_id = format!("workflow:item:{sequence}");
        }
        if event.identity.event_id.trim().is_empty() {
            event.identity.event_id = format!("workflow:{}:{sequence}", self.key.execution_id);
        }
        event.delivery = if self.replay {
            SessionEventDelivery::Replay
        } else {
            SessionEventDelivery::Live
        };
        if matches!(
            event.kind,
            SessionEventKind::TerminalResult { .. }
                | SessionEventKind::Cancel
                | SessionEventKind::Error { .. }
        ) {
            self.terminal_emitted.store(true, Ordering::Release);
        }
        if matches!(
            self.projection.apply(event),
            crate::backend::ai_execution::SessionEventApplyResult::Applied
        ) {
            // TaskRuntime is the resident event fan-out used by the desktop
            // shell. Re-publishing only a safe identity/detail projection
            // gives Tauri low-latency notifications without putting event
            // bodies into task snapshots.
            let phase = self.phase.lock().ok().and_then(|phase| *phase);
            let _ = self
                .task_runtime
                .update_detail(&self.task_id, self.safe_detail(phase, None));
        }
    }

    fn set_cleanup_report(&self, report: crate::backend::ai_execution::AiExecutionCleanupReport) {
        let cleanup = json!({
            "process_reaped": report.process_reaped,
            "workspace_removed": report.workspace_removed,
            "failure_count": report.failure_count,
            "session_closed": report.session_closed,
            "session_deleted": report.session_deleted,
            "session_delete_method": report.session_delete_method,
        });
        let phase = self.phase.lock().ok().and_then(|phase| *phase);
        let _ = self
            .task_runtime
            .update_detail(&self.task_id, self.safe_detail(phase, Some(cleanup)));
    }
}

fn member_task_detail(
    tenant_id: &str,
    team_id: &str,
    member_id: &str,
    execution_id: &str,
    replay: bool,
    phase: &str,
) -> Value {
    json!({
        "workflow": MEMBER_TURN_WORKFLOW,
        "tenant_id": tenant_id,
        "team_id": team_id,
        "member_id": member_id,
        "execution_id": execution_id,
        "replay": replay,
        "phase": phase,
    })
}

fn member_task_result(team_id: &str, member_id: &str, execution_id: &str, replay: bool) -> Value {
    json!({
        "workflow": MEMBER_TURN_WORKFLOW,
        "team_id": team_id,
        "member_id": member_id,
        "execution_id": execution_id,
        "replay": replay,
        "terminal": true,
    })
}

fn is_member_turn_task(snapshot: &TaskSnapshot) -> bool {
    snapshot.kind == TaskKind::TeamRun
        && snapshot.detail.get("workflow").and_then(Value::as_str) == Some(MEMBER_TURN_WORKFLOW)
}

fn validate_context_key(key: &str) -> AppResult<()> {
    let key = key.trim();
    if key.is_empty() || key.len() > MAX_CONTEXT_KEY_BYTES || key.chars().any(char::is_control) {
        return Err(AppError::Domain {
            code: "team_member_context_invalid".to_string(),
            message: "The Team member execution context is invalid.".to_string(),
            retryable: false,
            details: None,
        });
    }
    Ok(())
}

fn validate_binding(
    member: &TeamMember,
    tenant_id: &str,
    binding: &crate::backend::ai_execution::PersistentExecutionBinding,
) -> AppResult<()> {
    let anchor = binding.provider_session_id.trim();
    if binding.tenant_id != tenant_id
        || binding.execution_context_key != member.execution_context_key
        || binding.agent_id != member.agent_id
        || anchor.is_empty()
        || anchor.len() > MAX_PROVIDER_ANCHOR_BYTES
        || anchor.chars().any(char::is_control)
        || anchor.starts_with("native-session-")
    {
        return Err(AppError::Domain {
            code: "team_member_anchor_invalid".to_string(),
            message: "The Team member provider session anchor is invalid.".to_string(),
            retryable: false,
            details: Some(json!({ "memberId": member.id })),
        });
    }
    Ok(())
}

fn member_capability_error(member: &TeamMember, missing: &[&str]) -> AppError {
    AppError::Domain {
        code: "team_member_capabilities_missing".to_string(),
        message: "The Team member does not provide the required Session capabilities.".to_string(),
        retryable: false,
        details: Some(json!({
            "memberId": member.id,
            "missingCapabilities": missing,
        })),
    }
}

fn ai_error(error: AiExecutionError) -> AppError {
    let view = error.to_view();
    AppError::Domain {
        code: view.code,
        message: view.message,
        retryable: view.retryable,
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use crate::backend::{
        agents::types::{AgentCatalogEntry, AgentId, AgentProtocol, DeclaredAgentCapabilities},
        ai_execution::{
            executor::BackendFuture, AiExecutionError, AiExecutionRequest, AiExecutionResult,
            SessionEvent, SessionEventDelivery, SessionEventIdentity, SessionEventKind,
        },
        application::AppService,
        models::{CreateTeamInput, TeamMemberInput, TeamMemberTurnInput, TeamRole},
    };
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex,
        },
        time::{Duration, Instant},
    };

    #[test]
    fn member_turn_start_returns_before_provider_finishes_and_exposes_events() {
        let fixture = FixtureRuntime::new();
        let service = fixture.open_service("member-turn");
        let team = fixture.create_team(&service, "team-member-turn");
        let member = &team.members[1];

        let started_at = Instant::now();
        let initial = service
            .start_member_turn(TeamMemberTurnInput {
                team_id: team.team.id.clone(),
                member_id: member.id.clone(),
                message: "SECRET_PROMPT".to_string(),
                replay: false,
            })
            .expect("start member turn");

        assert!(started_at.elapsed() < Duration::from_millis(200));
        assert!(initial.task.state.is_active());
        assert_eq!(initial.member_id, member.id);
        assert_eq!(
            initial.stream.event_count, 1,
            "the user acknowledgement is live"
        );
        assert!(!serde_json::to_string(&initial.task.detail)
            .expect("serialize task detail")
            .contains("SECRET_PROMPT"));

        fixture.wait_until_started();
        let streamed = service
            .get_member_stream(&team.team.id, &member.id, &initial.execution_id)
            .expect("read member stream")
            .expect("stream exists");
        assert!(streamed
            .stream
            .items
            .iter()
            .any(|item| item.text.as_deref() == Some("provider delta")));

        fixture.release();
        fixture.wait_until_terminal(&service, &initial.task.task_id);
        let terminal = service
            .get_member_stream(&team.team.id, &member.id, &initial.execution_id)
            .expect("read terminal stream")
            .expect("terminal stream exists");
        assert!(terminal.task.state.is_terminal());
        assert!(!serde_json::to_string(&terminal.task.result)
            .expect("serialize task result")
            .contains("provider delta"));
    }

    #[test]
    fn member_turn_continues_without_a_consumer_and_cancel_is_scoped_to_one_member() {
        let fixture = FixtureRuntime::new();
        let service = fixture.open_service("member-turn-cancel");
        let team = fixture.create_team(&service, "team-member-turn-cancel");

        let first = service
            .start_member_turn(TeamMemberTurnInput {
                team_id: team.team.id.clone(),
                member_id: team.members[1].id.clone(),
                message: "first".to_string(),
                replay: false,
            })
            .expect("start first turn");
        let second = service
            .start_member_turn(TeamMemberTurnInput {
                team_id: team.team.id.clone(),
                member_id: team.members[2].id.clone(),
                message: "second".to_string(),
                replay: false,
            })
            .expect("start second turn");

        fixture.wait_until_started_count(2);
        let cancelled = service
            .cancel_member_turn(&team.team.id, &team.members[1].id, &first.execution_id)
            .expect("cancel first turn");
        assert!(cancelled.task.state.is_active());

        fixture.wait_until_terminal(&service, &first.task.task_id);
        let first_terminal = service
            .get_member_stream(&team.team.id, &team.members[1].id, &first.execution_id)
            .expect("read cancelled stream")
            .expect("cancelled stream exists");
        assert_eq!(
            first_terminal.task.state,
            crate::backend::runtime::tasks::TaskState::Canceled
        );
        assert!(first_terminal
            .stream
            .items
            .iter()
            .any(|item| item.kind == crate::backend::ai_execution::SessionItemKind::Cancelled));

        let second_live = service
            .get_member_stream(&team.team.id, &team.members[2].id, &second.execution_id)
            .expect("read second stream")
            .expect("second stream exists");
        assert!(second_live.task.state.is_active());
        fixture.release();
        fixture.wait_until_terminal(&service, &second.task.task_id);
    }

    #[test]
    fn member_replay_uses_the_same_workflow_and_marks_provider_events_as_replay() {
        let fixture = FixtureRuntime::new();
        let service = fixture.open_service("member-replay");
        let team = fixture.create_team(&service, "team-member-replay");
        let member = &team.members[1];
        fixture.seed_binding(&service, member);

        let initial = service
            .start_member_turn(TeamMemberTurnInput {
                team_id: team.team.id.clone(),
                member_id: member.id.clone(),
                message: String::new(),
                replay: true,
            })
            .expect("start member replay");

        assert!(initial.task.state.is_active());
        assert_eq!(initial.stream.event_count, 0);
        fixture.wait_until_started();
        let streamed = service
            .get_member_stream(&team.team.id, &member.id, &initial.execution_id)
            .expect("read replay stream")
            .expect("replay stream exists");
        assert!(streamed.stream.items.iter().any(|item| {
            item.text.as_deref() == Some("provider delta")
                && item.delivery == SessionEventDelivery::Replay
        }));

        fixture.release();
        fixture.wait_until_terminal(&service, &initial.task.task_id);
        let terminal = service
            .get_member_stream(&team.team.id, &member.id, &initial.execution_id)
            .expect("read replay terminal stream")
            .expect("replay terminal stream exists");
        assert!(terminal.task.state.is_terminal());
        assert!(terminal
            .stream
            .items
            .iter()
            .all(|item| item.delivery == SessionEventDelivery::Replay));
    }

    #[test]
    fn member_turn_rejects_cross_team_member_missing_capability_and_missing_anchor() {
        let fixture = FixtureRuntime::new();
        let service = fixture.open_service("member-turn-validation");
        let team = fixture.create_team(&service, "team-validation");
        let other_team = fixture.create_team(&service, "other-team");

        let cross_team = service.start_member_turn(TeamMemberTurnInput {
            team_id: team.team.id.clone(),
            member_id: other_team.members[1].id.clone(),
            message: "message".to_string(),
            replay: false,
        });
        assert_eq!(
            cross_team.expect_err("cross team member must fail").code(),
            "not_found"
        );

        fixture.set_capabilities(DeclaredAgentCapabilities {
            live_events: false,
            ..fixture.capabilities()
        });
        let missing_capability = service.start_member_turn(TeamMemberTurnInput {
            team_id: team.team.id.clone(),
            member_id: team.members[1].id.clone(),
            message: "message".to_string(),
            replay: false,
        });
        assert_eq!(
            missing_capability
                .expect_err("missing live capability must fail")
                .code(),
            "team_member_capabilities_missing"
        );

        fixture.set_capabilities(fixture.capabilities());
        let missing_anchor = service.start_member_turn(TeamMemberTurnInput {
            team_id: team.team.id.clone(),
            member_id: team.members[1].id.clone(),
            message: String::new(),
            replay: true,
        });
        assert_eq!(
            missing_anchor
                .expect_err("replay without a provider anchor must fail")
                .code(),
            "team_member_anchor_unavailable"
        );
    }

    #[test]
    fn member_stream_transport_redacts_tool_payload_and_keeps_sequence() {
        let public =
            super::public_session_snapshot(crate::backend::ai_execution::SessionSnapshot {
                revision: 7,
                event_count: 1,
                items: vec![crate::backend::ai_execution::SessionItemSnapshot {
                    identity: crate::backend::ai_execution::SessionItemIdentity {
                        session_id: "session".to_string(),
                        member_id: "member".to_string(),
                        execution_id: "execution".to_string(),
                        turn_id: "turn".to_string(),
                        item_id: "tool".to_string(),
                    },
                    kind: crate::backend::ai_execution::SessionItemKind::Tool,
                    sequence: 7,
                    delivery: SessionEventDelivery::Live,
                    state: crate::backend::ai_execution::SessionItemState::Completed,
                    text: Some("RAW_TOOL_PAYLOAD".to_string()),
                    status: None,
                    code: None,
                }],
            });

        assert_eq!(public.revision, 7);
        assert_eq!(public.items[0].text, None);
        assert!(!serde_json::to_string(&public)
            .expect("serialize public stream")
            .contains("RAW_TOOL_PAYLOAD"));
    }

    struct FixtureRuntime {
        started: Arc<Mutex<usize>>,
        release: Arc<AtomicBool>,
        capabilities: Arc<Mutex<DeclaredAgentCapabilities>>,
    }

    impl FixtureRuntime {
        fn new() -> Self {
            Self {
                started: Arc::new(Mutex::new(0)),
                release: Arc::new(AtomicBool::new(false)),
                capabilities: Arc::new(Mutex::new(all_capabilities())),
            }
        }

        fn open_service(&self, name: &str) -> AppService {
            let root = std::env::temp_dir().join(format!(
                "assetiweave-t06-{name}-{}",
                uuid::Uuid::new_v4().simple()
            ));
            std::fs::create_dir_all(&root).expect("create fixture root");
            let runtime: Arc<dyn crate::backend::ai_execution::AgentExecutionRuntime> =
                Arc::new(self.clone_for_runtime());
            AppService::open_with_db_path_and_runtime(root.join("app.db"), runtime)
                .expect("open fixture service")
        }

        fn clone_for_runtime(&self) -> BlockingRuntime {
            BlockingRuntime {
                started: self.started.clone(),
                release: self.release.clone(),
                capabilities: self.capabilities.clone(),
            }
        }

        fn create_team(
            &self,
            service: &AppService,
            id: &str,
        ) -> crate::backend::models::TeamDetail {
            service
                .create_team(CreateTeamInput {
                    id: Some(id.to_string()),
                    name: id.to_string(),
                    description: None,
                    members: vec![
                        TeamMemberInput {
                            id: Some(format!("{id}-leader")),
                            role: TeamRole::Leader,
                            sort_order: Some(0),
                            agent_id: "fixture-agent".to_string(),
                            model: None,
                        },
                        TeamMemberInput {
                            id: Some(format!("{id}-a")),
                            role: TeamRole::Teammate,
                            sort_order: Some(1),
                            agent_id: "fixture-agent".to_string(),
                            model: None,
                        },
                        TeamMemberInput {
                            id: Some(format!("{id}-b")),
                            role: TeamRole::Teammate,
                            sort_order: Some(2),
                            agent_id: "fixture-agent".to_string(),
                            model: None,
                        },
                    ],
                })
                .expect("create Team")
        }

        fn seed_binding(&self, service: &AppService, member: &crate::backend::models::TeamMember) {
            let binding = crate::backend::ai_execution::PersistentExecutionBinding {
                tenant_id: service.tenant_id().to_string(),
                execution_context_key: member.execution_context_key.clone(),
                provider_session_id: "provider-anchor".to_string(),
                agent_id: member.agent_id.clone(),
                installation_id: None,
                model: member.model.clone(),
                workspace_path: "/fixture/workspace".to_string(),
                binding_version: 1,
                provider_metadata_json: "{}".to_string(),
            };
            let store = crate::backend::ai_execution::PersistentBindingStore::new(
                service.db.pool().clone(),
            );
            service
                .runtime
                .run_sync(store.save(&binding))
                .expect("save provider binding");
        }

        fn wait_until_started(&self) {
            self.wait_until_started_count(1);
        }

        fn wait_until_started_count(&self, expected: usize) {
            for _ in 0..200 {
                if *self.started.lock().expect("started lock") >= expected {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!("fixture provider did not start");
        }

        fn wait_until_terminal(&self, service: &AppService, task_id: &str) {
            for _ in 0..200 {
                if service
                    .get_member_turn_task(task_id)
                    .expect("read member task")
                    .is_some_and(|task| task.state.is_terminal())
                {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!("member task did not become terminal: {task_id}");
        }

        fn release(&self) {
            self.release.store(true, Ordering::Release);
        }

        fn capabilities(&self) -> DeclaredAgentCapabilities {
            self.capabilities.lock().expect("capability lock").clone()
        }

        fn set_capabilities(&self, capabilities: DeclaredAgentCapabilities) {
            *self.capabilities.lock().expect("capability lock") = capabilities;
        }
    }

    impl Clone for FixtureRuntime {
        fn clone(&self) -> Self {
            Self {
                started: self.started.clone(),
                release: self.release.clone(),
                capabilities: self.capabilities.clone(),
            }
        }
    }

    struct BlockingRuntime {
        started: Arc<Mutex<usize>>,
        release: Arc<AtomicBool>,
        capabilities: Arc<Mutex<DeclaredAgentCapabilities>>,
    }

    impl crate::backend::ai_execution::AgentExecutionRuntime for BlockingRuntime {
        fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
            let started = self.started.clone();
            let release = self.release.clone();
            Box::pin(async move {
                if let Some(progress) = request.progress.as_ref() {
                    progress.emit_session_event(event(
                        &request.execution_id,
                        SessionEventKind::AssistantTextDelta {
                            text: "provider delta".to_string(),
                        },
                    ));
                }
                *started.lock().expect("started lock") += 1;
                while !release.load(Ordering::Acquire) && !request.cancellation.is_cancelled() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                if request.cancellation.is_cancelled() {
                    return Err(AiExecutionError::Cancelled {
                        program: PathBuf::from("fixture-agent"),
                    });
                }
                Ok(AiExecutionResult {
                    text: "provider delta".to_string(),
                    agent_id: request.agent_id,
                    protocol: AgentProtocol::Acp,
                    requested_model: request.model,
                    elapsed_ms: 1,
                    persistent_binding: None,
                    replay_text: request.replay.then(|| "replayed".to_string()),
                })
            })
        }

        fn list_agent_catalog(&self) -> Vec<AgentCatalogEntry> {
            vec![AgentCatalogEntry {
                id: "fixture-agent".to_string(),
                display_name: "Fixture Agent".to_string(),
                command: "fixture-agent".to_string(),
                args: Vec::new(),
                availability_command: "fixture-agent".to_string(),
                protocol: "acp".to_string(),
                capabilities: self.capabilities.lock().expect("capability lock").clone(),
            }]
        }

        fn agent_capabilities(&self, _agent_id: &AgentId) -> Option<DeclaredAgentCapabilities> {
            Some(self.capabilities.lock().expect("capability lock").clone())
        }
    }

    fn event(execution_id: &str, kind: SessionEventKind) -> SessionEvent {
        SessionEvent {
            identity: SessionEventIdentity {
                session_id: "provider-session".to_string(),
                member_id: "provider-member".to_string(),
                execution_id: "provider-execution".to_string(),
                turn_id: "provider-turn".to_string(),
                item_id: "assistant".to_string(),
                event_id: format!("provider-event-{execution_id}"),
            },
            sequence: 1,
            delivery: SessionEventDelivery::Live,
            kind,
        }
    }

    fn all_capabilities() -> DeclaredAgentCapabilities {
        DeclaredAgentCapabilities {
            text_prompt: true,
            resume: true,
            history_replay: true,
            live_events: true,
            rich_history_replay: false,
            team_tools: false,
            resume_args: None,
        }
    }
}
