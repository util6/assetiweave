use crate::backend::ai_execution::{
    AiExecutionPhase, AiExecutionProgressSink, SessionEvent, SessionEventDelivery,
    SessionEventIdentity, SessionEventKind, SessionEventProjection,
};
use crate::backend::dto::{
    AgentInfoView, AgentSessionContextView, AgentSessionRef, AgentSessionTerminalView,
};
use crate::backend::runtime::session_streams::{AgentSessionMetadata, SessionStreamKey};
use crate::backend::runtime::AppRuntime;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use uuid::Uuid;

pub(crate) struct MemoryTurnProgressSink {
    projection: Arc<SessionEventProjection>,
    execution_id: String,
    sequence: Arc<AtomicU64>,
    terminal_emitted: Arc<AtomicBool>,
}

impl MemoryTurnProgressSink {
    pub(crate) fn new(projection: Arc<SessionEventProjection>, execution_id: String) -> Self {
        Self {
            projection,
            execution_id,
            sequence: Arc::new(AtomicU64::new(0)),
            terminal_emitted: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn finish_succeeded(&self) {
        if !self.terminal_emitted.swap(true, Ordering::AcqRel) {
            self.emit_control("terminal", SessionEventKind::TerminalResult { text: None });
        }
    }

    pub(crate) fn finish_cancelled(&self) {
        if !self.terminal_emitted.swap(true, Ordering::AcqRel) {
            self.emit_control("cancel", SessionEventKind::Cancel);
        }
    }

    pub(crate) fn finish_failed(&self, code: &str, retryable: bool) {
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

    pub(crate) fn emit_control(&self, suffix: &str, kind: SessionEventKind) {
        self.emit_session_event(SessionEvent {
            identity: SessionEventIdentity {
                session_id: self.execution_id.clone(),
                member_id: self.execution_id.clone(),
                execution_id: self.execution_id.clone(),
                turn_id: self.execution_id.clone(),
                item_id: format!("memory:{suffix}"),
                event_id: format!("memory:{}:{suffix}", self.execution_id),
            },
            sequence: 0,
            delivery: SessionEventDelivery::Live,
            kind,
            truncation: None,
        });
    }
}

impl AiExecutionProgressSink for MemoryTurnProgressSink {
    fn set_phase(&self, _phase: AiExecutionPhase) {}

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
        event.identity.session_id = self.execution_id.clone();
        event.identity.member_id = self.execution_id.clone();
        event.identity.execution_id = self.execution_id.clone();
        event.identity.turn_id = self.execution_id.clone();
        if event.identity.item_id.trim().is_empty() {
            event.identity.item_id = format!("memory:item:{sequence}");
        }
        if event.identity.event_id.trim().is_empty() {
            event.identity.event_id = format!("memory:{}:{sequence}", self.execution_id);
        }
        event.delivery = SessionEventDelivery::Live;
        self.projection.apply(event);
    }
}

pub(crate) struct MemoryAgentSessionParams<'a> {
    pub(crate) tenant_id: &'a str,
    pub(crate) scope: &'a str, // "session" | "project" | "global" | "recall"
    pub(crate) job_id: &'a str,
    pub(crate) task_id: Option<&'a str>,
    pub(crate) agent_id: &'a str,
    pub(crate) display_name: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) prompt_summary: &'a str,
    pub(crate) custom_session_ref: Option<AgentSessionRef>,
    pub(crate) persistent: bool,
}

pub(crate) struct ActiveMemoryAgentSession {
    pub(crate) session_ref: AgentSessionRef,
    pub(crate) sink: Arc<MemoryTurnProgressSink>,
    pub(crate) projection: Arc<SessionEventProjection>,
    pub(crate) key: SessionStreamKey,
}

impl ActiveMemoryAgentSession {
    pub(crate) fn start(runtime: &AppRuntime, params: MemoryAgentSessionParams) -> Self {
        let session_ref = params
            .custom_session_ref
            .unwrap_or_else(|| AgentSessionRef {
                schema_version: 1,
                value: format!("agent-session://{}-memory/{}", params.scope, Uuid::new_v4()),
            });
        let execution_id = if params.scope == "recall" {
            format!("memory-recall-session:{}", params.job_id)
        } else {
            format!("{}-memory-job:{}", params.scope, params.job_id)
        };
        let key = SessionStreamKey {
            tenant_id: params.tenant_id.to_string(),
            team_id: "memory".to_string(),
            member_id: format!("{}_memory", params.scope),
            execution_id: execution_id.clone(),
        };
        let purpose = format!("{}_memory", params.scope);
        let metadata = AgentSessionMetadata {
            session_ref: session_ref.clone(),
            execution_id: key.execution_id.clone(),
            purpose,
            mode: if params.persistent {
                "persistent".to_string()
            } else {
                "oneshot".to_string()
            },
            tenant_id: Some(params.tenant_id.to_string()),
            agent: AgentInfoView {
                id: params.agent_id.to_string(),
                display_name: params.display_name,
                model: params.model,
                protocol: "builtin".to_string(),
            },
            context: AgentSessionContextView {
                team_id: None,
                member_id: None,
                memory_scope: Some(params.scope.to_string()),
                memory_job_id: Some(params.job_id.to_string()),
                task_id: params.task_id.map(str::to_string),
            },
            allow_stop: true,
        };
        let projection = runtime
            .session_streams()
            .register_with_metadata(key.clone(), metadata);
        let sink = Arc::new(MemoryTurnProgressSink::new(
            projection.clone(),
            key.execution_id.clone(),
        ));
        sink.emit_control(
            "request",
            SessionEventKind::UserMessageAcknowledged {
                accepted: true,
                text: Some(params.prompt_summary.to_string()),
            },
        );
        Self {
            session_ref,
            sink,
            projection,
            key,
        }
    }

    pub(crate) fn finish_succeeded(&self, runtime: &AppRuntime) {
        self.sink.finish_succeeded();
        runtime.session_streams().mark_terminal_by_ref(
            &self.session_ref.value,
            Some(AgentSessionTerminalView {
                state: "succeeded".to_string(),
                code: None,
                message: None,
                retryable: false,
            }),
        );
    }

    pub(crate) fn finish_cancelled(&self, runtime: &AppRuntime) {
        self.sink.finish_cancelled();
        runtime.session_streams().mark_terminal_by_ref(
            &self.session_ref.value,
            Some(AgentSessionTerminalView {
                state: "canceled".to_string(),
                code: Some("canceled".to_string()),
                message: Some("任务已被取消".to_string()),
                retryable: false,
            }),
        );
    }

    pub(crate) fn finish_failed(
        &self,
        runtime: &AppRuntime,
        code: &str,
        message: &str,
        retryable: bool,
    ) {
        self.sink.finish_failed(code, retryable);
        runtime.session_streams().mark_terminal_by_ref(
            &self.session_ref.value,
            Some(AgentSessionTerminalView {
                state: "failed".to_string(),
                code: Some(code.to_string()),
                message: Some(message.to_string()),
                retryable,
            }),
        );
    }
}
