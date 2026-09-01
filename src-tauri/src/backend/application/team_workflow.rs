use crate::backend::{
    agents::types::AgentId,
    ai_execution::{
        execute_agent_blocking, AgentSessionMode, AiExecutionCancellation, AiExecutionLimits,
        AiExecutionPurpose, AiExecutionRequest, AiTeamTools,
    },
    application::AppService,
    models::{
        TeamConfirmInput, TeamDetail, TeamDraftInput, TeamLeaderChatInput, TeamLeaderChatResult,
        TeamMailboxMessage, TeamMailboxReadInput, TeamMailboxSendInput, TeamReviewInput,
        TeamRunSnapshot, TeamTask, TeamTaskDraft, TeamTaskState, TeamTaskUpdateInput,
        TeamToolCredential, TeamToolCredentialInput, TeamToolTaskListInput,
    },
    runtime::{tasks::ProgressHandle, AppError, AppResult},
    store::{
        cancel_team_run_sqlx, claim_team_task_sqlx, complete_team_run_draft_sqlx,
        confirm_team_run_sqlx, create_team_run_shell_sqlx, fail_team_run_sqlx,
        finish_team_task_sqlx, get_team_run_snapshot_sqlx, get_team_task_sqlx,
        mark_team_run_terminal_sqlx, read_team_mailbox_sqlx, review_team_run_sqlx,
        send_team_mailbox_sqlx,
    },
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use uuid::Uuid;

impl AppService {
    pub(crate) fn team_run_task(
        &self,
        task_id: &str,
    ) -> AppResult<Option<crate::backend::runtime::tasks::TaskSnapshot>> {
        Ok(self
            .runtime
            .task_runtime()
            .get_for_tenant(self.tenant_id(), task_id)
            .filter(|snapshot| snapshot.kind == crate::backend::runtime::tasks::TaskKind::TeamRun))
    }

    pub(crate) fn list_team_run_tasks(
        &self,
    ) -> AppResult<Vec<crate::backend::runtime::tasks::TaskSnapshot>> {
        Ok(self.runtime.task_runtime().list_for_tenant(
            self.tenant_id(),
            crate::backend::runtime::tasks::TaskFilter {
                kind: Some(crate::backend::runtime::tasks::TaskKind::TeamRun),
                active_only: false,
            },
        ))
    }

    pub(crate) fn cancel_team_run_task(
        &self,
        run_id: &str,
    ) -> AppResult<crate::backend::runtime::tasks::TaskSnapshot> {
        let task = self
            .runtime
            .task_runtime()
            .list_for_tenant(
                self.tenant_id(),
                crate::backend::runtime::tasks::TaskFilter {
                    kind: Some(crate::backend::runtime::tasks::TaskKind::TeamRun),
                    active_only: true,
                },
            )
            .into_iter()
            .find(|task| {
                task.detail.get("run_id").and_then(Value::as_str) == Some(run_id)
                    || task.task_id.ends_with(run_id)
            })
            .ok_or_else(|| AppError::NotFound(format!("No active Team task for run: {run_id}")))?;
        match self
            .runtime
            .task_runtime()
            .cancel_for_tenant(self.tenant_id(), &task.task_id)
        {
            crate::backend::runtime::tasks::CancelOutcome::Requested(snapshot)
            | crate::backend::runtime::tasks::CancelOutcome::AlreadyFinished(snapshot) => {
                Ok(snapshot)
            }
            crate::backend::runtime::tasks::CancelOutcome::NotFound => Err(AppError::NotFound(
                format!("Team task not found: {}", task.task_id),
            )),
        }
    }

    pub(crate) fn leader_chat(
        &self,
        input: TeamLeaderChatInput,
    ) -> AppResult<TeamLeaderChatResult> {
        let team = self.team_detail(&input.team_id)?;
        let leader = team
            .members
            .iter()
            .find(|member| member.role == crate::backend::models::TeamRole::Leader)
            .ok_or_else(|| AppError::Validation("Team has no leader".to_string()))?;
        let agent_id = AgentId::parse(leader.agent_id.clone())
            .map_err(|error| AppError::Validation(error.to_string()))?;
        let replay = input.replay;
        let execution_id = format!("team-exec-{}", Uuid::new_v4().simple());
        let request = AiExecutionRequest {
            execution_id: execution_id.clone(),
            agent_id,
            purpose: AiExecutionPurpose::TeamLeaderChat,
            session_mode: AgentSessionMode::Persistent,
            prompt: if replay {
                "Load the saved provider history for this Team leader context.".to_string()
            } else {
                input.message.trim().to_string()
            },
            model: leader.model.clone(),
            limits: AiExecutionLimits::default(),
            cancellation: AiExecutionCancellation::default(),
            progress: None,
            tenant_id: Some(self.tenant_id().to_string()),
            execution_context_key: Some(leader.execution_context_key.clone()),
            binding: None,
            replay,
            restore_only: false,
            team_tools: None,
            recall_tools: None,
        };
        let result =
            execute_agent_blocking(self.agent_runtime.clone(), request).map_err(ai_error)?;
        Ok(TeamLeaderChatResult {
            team_id: input.team_id,
            member_id: leader.id.clone(),
            execution_id,
            text: result.replay_text.unwrap_or(result.text),
            replay,
        })
    }

    pub(crate) fn draft_team(&self, input: TeamDraftInput) -> AppResult<TeamRunSnapshot> {
        if input.leader_message.trim().is_empty() {
            return Err(AppError::Validation(
                "Leader message is required".to_string(),
            ));
        }
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let shell = self.runtime.run_sync(async {
            create_team_run_shell_sqlx(&pool, &tenant_id, &input.team_id).await
        })?;
        let run_id = shell.run.id.clone();
        let task_id = format!("team-task-draft-{run_id}");
        let runtime = self.runtime.clone();
        let spec = crate::backend::runtime::tasks::TaskSpec::new(
            crate::backend::runtime::tasks::TaskKind::TeamRun,
            Some(format!("team-draft:{run_id}")),
        )
        .with_task_id(task_id.clone())
        .with_tenant_id(tenant_id)
        .with_conflict_key(format!("team-run:{run_id}"));
        let registration = self.runtime.task_runtime().register_external(spec)?;
        if let crate::backend::runtime::tasks::ExternalRegistrationOutcome::Started(_) =
            registration
        {
            let worker_runtime = runtime.clone();
            let worker_input = input.clone();
            let worker_run_id = run_id.clone();
            self.runtime.task_runtime().start_external_with(
                &task_id,
                serde_json::json!({ "run_id": run_id, "phase": "drafting" }),
                Box::new(move |context| {
                    let service = AppService::from_runtime(&worker_runtime);
                    if context.is_cancelled() {
                        service.cancel_team_run(&worker_run_id, "cancelled")?;
                        return Err(AppError::Canceled("Team draft was cancelled".to_string()));
                    }
                    let progress = context.progress();
                    progress.progress(0, None, Some("leader_draft"));
                    let result = service.generate_team_drafts(
                        worker_input,
                        &worker_run_id,
                        AiExecutionCancellation::from_token(context.cancellation()),
                    );
                    match result {
                        Ok(drafts) => {
                            let pool = service.db.pool().clone();
                            let tenant_id = service.tenant_id().to_string();
                            let run_id_for_store = worker_run_id.clone();
                            let snapshot = service.runtime.run_sync(async move {
                                complete_team_run_draft_sqlx(
                                    &pool,
                                    &tenant_id,
                                    &run_id_for_store,
                                    &drafts,
                                )
                                .await
                            })?;
                            progress.progress(1, Some(1), Some("review_ready"));
                            serde_json::to_value(team_runtime_projection(&snapshot))
                                .map_err(AppError::external)
                        }
                        Err(error) => {
                            let pool = service.db.pool().clone();
                            let tenant_id = service.tenant_id().to_string();
                            let run_id_for_store = worker_run_id.clone();
                            let error_code = error
                                .to_string()
                                .split(':')
                                .next()
                                .unwrap_or("team_draft_failed")
                                .to_string();
                            service.runtime.run_sync(async move {
                                fail_team_run_sqlx(
                                    &pool,
                                    &tenant_id,
                                    &run_id_for_store,
                                    &error_code,
                                )
                                .await
                            })?;
                            Err(error)
                        }
                    }
                }),
            )?;
        }
        Ok(shell)
    }

    fn generate_team_drafts(
        &self,
        input: TeamDraftInput,
        run_id: &str,
        cancellation: AiExecutionCancellation,
    ) -> AppResult<Vec<TeamTaskDraft>> {
        let run = self
            .get_team_run(run_id)?
            .ok_or_else(|| AppError::NotFound(format!("Team run not found: {run_id}")))?;
        if run.run.team_id != input.team_id
            || run.run.state != crate::backend::models::TeamRunState::Drafting
        {
            return Err(AppError::Conflict(
                "Team run is not the active drafting snapshot".to_string(),
            ));
        }
        let leader = run
            .run
            .roster_snapshot
            .iter()
            .find(|member| member.role == crate::backend::models::TeamRole::Leader)
            .ok_or_else(|| AppError::Validation("Team has no leader".to_string()))?;
        let agent_id = AgentId::parse(leader.agent_id.clone())
            .map_err(|error| AppError::Validation(error.to_string()))?;
        let roster = serde_json::to_string(
            &run.run
                .roster_snapshot
                .iter()
                .map(|member| {
                    serde_json::json!({
                        "member_id": member.member_id,
                        "role": member.role,
                        "sort_order": member.sort_order,
                        "agent_id": member.agent_id,
                        "model": member.model,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .map_err(AppError::external)?;
        let prompt = format!(
            "Return only JSON matching {{\"tasks\":[{{\"title\":string,\"description\":string,\"recommended_member_id\":string}}]}}. Use only teammate member_id values from this frozen roster, preserving its Agent/model/order contract: {roster}. User request: {}",
            input.leader_message.trim()
        );
        let result = execute_agent_blocking(
            self.agent_runtime.clone(),
            AiExecutionRequest {
                execution_id: format!("team-draft-{}", Uuid::new_v4().simple()),
                agent_id,
                purpose: AiExecutionPurpose::TeamDraft,
                session_mode: AgentSessionMode::Persistent,
                prompt,
                model: leader.model.clone(),
                limits: AiExecutionLimits::default(),
                cancellation,
                progress: None,
                tenant_id: Some(self.tenant_id().to_string()),
                execution_context_key: Some(leader.execution_context_key.clone()),
                binding: None,
                replay: false,
                restore_only: false,
                team_tools: None,
                recall_tools: None,
            },
        )
        .map_err(ai_error)?;
        parse_draft(&result.text)
    }

    pub(crate) fn get_team_run(&self, run_id: &str) -> AppResult<Option<TeamRunSnapshot>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let run_id = run_id.to_string();
        self.runtime
            .run_sync(async move { get_team_run_snapshot_sqlx(&pool, &tenant_id, &run_id).await })
    }

    pub(crate) fn latest_team_run(&self, team_id: &str) -> AppResult<Option<TeamRunSnapshot>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let team_id = team_id.to_string();
        self.runtime.run_sync(async move {
            crate::backend::store::get_latest_team_run_snapshot_sqlx(&pool, &tenant_id, &team_id)
                .await
        })
    }

    /// Register restoration as a background Team task. Restoring provider
    /// sessions may touch the filesystem and invoke an external Agent, so the
    /// command returns the canonical TaskRuntime snapshot immediately.
    pub(crate) fn restore_team_run(
        &self,
        run_id: &str,
    ) -> AppResult<crate::backend::runtime::tasks::TaskSnapshot> {
        if self.get_team_run(run_id)?.is_none() {
            return Err(AppError::NotFound(format!("Team run not found: {run_id}")));
        }
        let run_id = run_id.to_string();
        let task_id = format!("team-task-restore-{run_id}");
        let tenant_id = self.tenant_id().to_string();
        let spec = crate::backend::runtime::tasks::TaskSpec::new(
            crate::backend::runtime::tasks::TaskKind::TeamRun,
            Some(format!("team-restore:{run_id}")),
        )
        .with_task_id(task_id.clone())
        .with_tenant_id(tenant_id)
        .with_conflict_key(format!("team-run:{run_id}"));
        let registration = self.runtime.task_runtime().register_external(spec)?;
        if let crate::backend::runtime::tasks::ExternalRegistrationOutcome::Started(_) =
            registration
        {
            let runtime = self.runtime.clone();
            let worker_run_id = run_id.clone();
            self.runtime.task_runtime().start_external_with(
                &task_id,
                serde_json::json!({ "run_id": run_id, "phase": "restoring" }),
                Box::new(move |context| {
                    if context.is_cancelled() {
                        return Err(AppError::Canceled(
                            "Team restoration was cancelled".to_string(),
                        ));
                    }
                    let service = AppService::from_runtime(&runtime);
                    let snapshot = service.restore_team_run_sync(&worker_run_id)?;
                    let Some(snapshot) = snapshot else {
                        return Err(AppError::NotFound(format!(
                            "Team run not found: {worker_run_id}"
                        )));
                    };
                    serde_json::to_value(crate::backend::models::TeamRestoreTaskResult {
                        run_id: snapshot.run.run.id,
                        leader_error_code: snapshot.leader_error_code,
                        members: snapshot.members,
                    })
                    .map_err(AppError::external)
                }),
            )?;
        }
        self.runtime
            .task_runtime()
            .get_for_tenant(self.tenant_id(), &task_id)
            .ok_or_else(|| AppError::NotFound(format!("Team restore task not found: {task_id}")))
    }

    /// Restores the provider-backed projection in leader-first order.  Team
    /// facts remain the source of truth; a missing binding is reported per
    /// member and never repaired by silently creating a new session.
    fn restore_team_run_sync(
        &self,
        run_id: &str,
    ) -> AppResult<Option<crate::backend::models::TeamRestoreSnapshot>> {
        let Some(run) = self.get_team_run(run_id)? else {
            return Ok(None);
        };
        let leader_member = run
            .run
            .roster_snapshot
            .iter()
            .find(|member| member.role == crate::backend::models::TeamRole::Leader)
            .ok_or_else(|| AppError::Validation("Team run has no frozen leader".to_string()))?;
        let leader = AgentId::parse(leader_member.agent_id.clone())
            .map_err(|error| AppError::Validation(error.to_string()))
            .and_then(|agent_id| {
                let execution_id = format!("team-exec-{}", Uuid::new_v4().simple());
                execute_agent_blocking(
                    self.agent_runtime.clone(),
                    AiExecutionRequest {
                        execution_id: execution_id.clone(),
                        agent_id,
                        purpose: AiExecutionPurpose::TeamLeaderChat,
                        session_mode: AgentSessionMode::Persistent,
                        prompt: "Load the saved provider history for this Team leader context."
                            .to_string(),
                        model: leader_member.model.clone(),
                        limits: AiExecutionLimits::default(),
                        cancellation: AiExecutionCancellation::default(),
                        progress: None,
                        tenant_id: Some(self.tenant_id().to_string()),
                        execution_context_key: Some(leader_member.execution_context_key.clone()),
                        binding: None,
                        replay: true,
                        restore_only: false,
                        team_tools: None,
                        recall_tools: None,
                    },
                )
                .map_err(ai_error)
                .map(|result| TeamLeaderChatResult {
                    team_id: run.run.team_id.clone(),
                    member_id: leader_member.member_id.clone(),
                    execution_id,
                    text: result.replay_text.unwrap_or(result.text),
                    replay: true,
                })
            });
        let (leader, leader_error_code) = match leader {
            Ok(result) => (Some(result), None),
            Err(error) => (None, Some(error.view().code)),
        };
        let binding_store =
            crate::backend::ai_execution::PersistentBindingStore::new(self.db.pool().clone());
        let tenant_id = self.tenant_id().to_string();
        let mut members = Vec::with_capacity(run.run.roster_snapshot.len());
        for member in &run.run.roster_snapshot {
            if member.role == crate::backend::models::TeamRole::Leader {
                members.push(crate::backend::models::TeamMemberRestoreStatus {
                    member_id: member.member_id.clone(),
                    role: member.role,
                    state: if leader_error_code.is_none() {
                        crate::backend::models::TeamMemberRestoreState::Ready
                    } else {
                        crate::backend::models::TeamMemberRestoreState::Unavailable
                    },
                    error_code: leader_error_code.clone(),
                });
                continue;
            }
            let binding = self
                .runtime
                .run_sync(binding_store.load(&tenant_id, &member.execution_context_key))?;
            let Some(agent_id) = AgentId::parse(member.agent_id.clone()).ok() else {
                members.push(unavailable_member_restore_status(member));
                continue;
            };
            let capabilities_ready = self
                .agent_runtime
                .agent_capabilities(&agent_id)
                .is_none_or(|capabilities| capabilities.resume);
            let binding_ready = binding.as_ref().is_some_and(|binding| {
                binding.agent_id == member.agent_id
                    && PathBuf::from(&binding.workspace_path).is_dir()
                    && capabilities_ready
            });
            let ready = binding_ready
                && execute_agent_blocking(
                    self.agent_runtime.clone(),
                    AiExecutionRequest {
                        execution_id: format!("team-restore-{}", Uuid::new_v4().simple()),
                        agent_id,
                        purpose: AiExecutionPurpose::TeamTask,
                        session_mode: AgentSessionMode::Persistent,
                        prompt: String::new(),
                        model: member.model.clone(),
                        limits: AiExecutionLimits::default(),
                        cancellation: AiExecutionCancellation::default(),
                        progress: None,
                        tenant_id: Some(self.tenant_id().to_string()),
                        execution_context_key: Some(member.execution_context_key.clone()),
                        binding,
                        replay: false,
                        restore_only: true,
                        team_tools: None,
                        recall_tools: None,
                    },
                )
                .is_ok();
            members.push(crate::backend::models::TeamMemberRestoreStatus {
                member_id: member.member_id.clone(),
                role: member.role,
                state: if ready {
                    crate::backend::models::TeamMemberRestoreState::Ready
                } else {
                    crate::backend::models::TeamMemberRestoreState::Unavailable
                },
                error_code: (!ready).then_some("resume_unavailable".to_string()),
            });
        }
        Ok(Some(crate::backend::models::TeamRestoreSnapshot {
            run,
            leader,
            leader_error_code,
            members,
        }))
    }

    pub(crate) fn review_team_run(&self, input: TeamReviewInput) -> AppResult<TeamRunSnapshot> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.runtime
            .run_sync(async move { review_team_run_sqlx(&pool, &tenant_id, &input).await })
    }

    pub(crate) fn confirm_team_run(&self, input: TeamConfirmInput) -> AppResult<TeamRunSnapshot> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let snapshot = self
            .runtime
            .run_sync(async { confirm_team_run_sqlx(&pool, &tenant_id, &input).await })?;
        let run_id = snapshot.run.id.clone();
        self.schedule_team_run_execution(&run_id)?;
        self.runtime.notify_domain_events();
        Ok(snapshot)
    }

    pub(crate) fn recover_team_runs(&self) -> AppResult<usize> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let run_ids = self.runtime.run_sync(async {
            crate::backend::store::list_recoverable_team_run_ids_sqlx(&pool, &tenant_id).await
        })?;
        let mut scheduled = 0;
        for run_id in run_ids {
            self.schedule_team_run_execution(&run_id)?;
            scheduled += 1;
        }
        Ok(scheduled)
    }

    fn schedule_team_run_execution(&self, run_id: &str) -> AppResult<()> {
        let run_id = run_id.to_string();
        let task_id = format!("team-task-run-{run_id}");
        let runtime = self.runtime.clone();
        let tenant_id = self.tenant_id().to_string();
        let spec = crate::backend::runtime::tasks::TaskSpec::new(
            crate::backend::runtime::tasks::TaskKind::TeamRun,
            Some(format!("team-execute:{run_id}")),
        )
        .with_task_id(task_id.clone())
        .with_tenant_id(tenant_id)
        .with_conflict_key(format!("team-run:{run_id}"));
        let registration = self.runtime.task_runtime().register_external(spec)?;
        if let crate::backend::runtime::tasks::ExternalRegistrationOutcome::Started(_) =
            registration
        {
            let worker_runtime = runtime.clone();
            let worker_run_id = run_id.clone();
            self.runtime.task_runtime().start_external_with(
                &task_id,
                serde_json::json!({ "run_id": run_id, "domain": "team" }),
                Box::new(move |context| {
                    let service = AppService::from_runtime(&worker_runtime);
                    if context.is_cancelled() {
                        service.cancel_team_run(&worker_run_id, "cancelled")?;
                        return Err(AppError::Canceled(
                            "Team execution was cancelled".to_string(),
                        ));
                    }
                    let result = service.execute_team_tasks(
                        worker_run_id.clone(),
                        AiExecutionCancellation::from_token(context.cancellation()),
                        Some(context.progress()),
                    );
                    if let Err(error) = &result {
                        let _ = service.cancel_team_run(&worker_run_id, &error.view().code);
                    }
                    result.map(|snapshot| {
                        serde_json::to_value(team_runtime_projection(&snapshot))
                            .map_err(AppError::external)
                    })?
                }),
            )?;
        }
        Ok(())
    }

    fn execute_team_tasks(
        &self,
        run_id: String,
        cancellation: AiExecutionCancellation,
        progress: Option<ProgressHandle>,
    ) -> AppResult<TeamRunSnapshot> {
        let snapshot = self
            .get_team_run(&run_id)?
            .ok_or_else(|| AppError::NotFound(format!("Team run not found: {run_id}")))?;
        let runnable_tasks = snapshot
            .tasks
            .iter()
            .filter(|task| matches!(task.state, TeamTaskState::Queued | TeamTaskState::Running))
            .collect::<Vec<_>>();
        let total = runnable_tasks.len() as u64;
        for (index, queued) in runnable_tasks.into_iter().enumerate() {
            if let Some(progress) = &progress {
                progress.progress(index as u64, Some(total), Some("teammate_execution"));
            }
            if cancellation.is_cancelled() {
                self.cancel_team_run(&run_id, "cancelled")?;
                return Err(AppError::Canceled(
                    "Team execution was cancelled".to_string(),
                ));
            }
            let Some(task) = self.claim_team_task(&queued.id)? else {
                continue;
            };
            let member = snapshot
                .run
                .roster_snapshot
                .iter()
                .find(|member| Some(member.member_id.as_str()) == task.owner_member_id.as_deref())
                .ok_or_else(|| {
                    AppError::Validation(
                        "Team task owner is absent from roster snapshot".to_string(),
                    )
                })?;
            let agent_id = AgentId::parse(member.agent_id.clone())
                .map_err(|error| AppError::Validation(error.to_string()))?;
            // Tool transport is an independent capability.  A Native
            // Resume-only member still gets its Persistent execution, while
            // a declared Team-tools member receives the short-lived scoped
            // credential used by the ACP MCP bridge.
            let team_tools_enabled = AgentId::parse(member.agent_id.clone())
                .ok()
                .and_then(|id| self.agent_runtime.agent_capabilities(&id))
                .map(|capabilities| capabilities.team_tools)
                .unwrap_or(true);
            let tool_credential = if team_tools_enabled {
                match self.issue_team_tool_credential(TeamToolCredentialInput {
                    team_id: task.team_id.clone(),
                    run_id: task.run_id.clone(),
                    member_id: member.member_id.clone(),
                    ttl_seconds: Some(900),
                }) {
                    Ok(value) => Some(value.credential),
                    Err(error) => {
                        let error_code = error.view().code;
                        self.finish_team_task(
                            &task.id,
                            TeamTaskState::Failed,
                            None,
                            Some(&error_code),
                        )?;
                        continue;
                    }
                }
            } else {
                None
            };
            let request = AiExecutionRequest {
                execution_id: format!("team-task-{}", Uuid::new_v4().simple()),
                agent_id,
                purpose: AiExecutionPurpose::TeamTask,
                session_mode: AgentSessionMode::Persistent,
                prompt: task.description.clone(),
                model: member.model.clone(),
                limits: AiExecutionLimits::default(),
                cancellation: cancellation.clone(),
                progress: None,
                tenant_id: Some(self.tenant_id().to_string()),
                execution_context_key: Some(member.execution_context_key.clone()),
                binding: None,
                replay: false,
                restore_only: false,
                recall_tools: None,
                team_tools: tool_credential.map(|credential| AiTeamTools {
                    tenant_id: self.tenant_id().to_string(),
                    team_id: task.team_id.clone(),
                    run_id: task.run_id.clone(),
                    member_id: member.member_id.clone(),
                    credential,
                    database_path: self.runtime.db_path().to_string_lossy().into_owned(),
                }),
            };
            match execute_agent_blocking(self.agent_runtime.clone(), request) {
                Ok(result) => {
                    self.finish_team_task(
                        &task.id,
                        TeamTaskState::Succeeded,
                        Some(&result.text),
                        None,
                    )?;
                }
                Err(error) => {
                    let view = error.to_view();
                    let canceled = cancellation.is_cancelled()
                        || matches!(
                            error,
                            crate::backend::ai_execution::AiExecutionError::Cancelled { .. }
                        );
                    self.finish_team_task(
                        &task.id,
                        if canceled {
                            TeamTaskState::Canceled
                        } else {
                            TeamTaskState::Failed
                        },
                        None,
                        Some(&view.code),
                    )?;
                    if canceled {
                        self.cancel_team_run(&run_id, "cancelled")?;
                        return Err(AppError::Canceled(
                            "Team execution was cancelled".to_string(),
                        ));
                    }
                }
            }
        }
        if let Some(progress) = &progress {
            progress.progress(total, Some(total), Some("terminal"));
        }
        self.finalize_team_run(&run_id)?;
        self.get_team_run(&run_id)?
            .ok_or_else(|| AppError::NotFound(format!("Team run not found: {run_id}")))
    }

    fn consume_team_mailbox_and_summarize(
        &self,
        snapshot: &TeamRunSnapshot,
    ) -> AppResult<Option<TeamLeaderChatResult>> {
        let leader = snapshot
            .run
            .roster_snapshot
            .iter()
            .find(|member| member.role == crate::backend::models::TeamRole::Leader)
            .ok_or_else(|| AppError::Validation("Team run has no frozen leader".to_string()))?;
        let messages = self.read_team_mailbox(TeamMailboxReadInput {
            team_id: snapshot.run.team_id.clone(),
            run_id: snapshot.run.id.clone(),
            recipient_member_id: leader.member_id.clone(),
            ack: false,
        })?;
        if messages.is_empty() {
            return Ok(None);
        }
        let body = messages
            .iter()
            .map(|message| message.body.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let agent_id = AgentId::parse(leader.agent_id.clone())
            .map_err(|error| AppError::Validation(error.to_string()))?;
        let execution_id = format!("team-summary-{}", Uuid::new_v4().simple());
        let result = execute_agent_blocking(
            self.agent_runtime.clone(),
            AiExecutionRequest {
                execution_id: execution_id.clone(),
                agent_id,
                purpose: AiExecutionPurpose::TeamSummary,
                session_mode: AgentSessionMode::Persistent,
                prompt: format!(
                    "Summarize these completed Team task reports for the user. Do not create or mutate tasks; reply with the concise user-facing summary only. Reports:\n{body}"
                ),
                model: leader.model.clone(),
                limits: AiExecutionLimits::default(),
                cancellation: AiExecutionCancellation::default(),
                progress: None,
                tenant_id: Some(self.tenant_id().to_string()),
                execution_context_key: Some(leader.execution_context_key.clone()),
                binding: None,
                replay: false,
                restore_only: false,
                team_tools: None,
        recall_tools: None,
            },
        )
        .map_err(ai_error)?;
        self.read_team_mailbox(TeamMailboxReadInput {
            team_id: snapshot.run.team_id.clone(),
            run_id: snapshot.run.id.clone(),
            recipient_member_id: leader.member_id.clone(),
            ack: true,
        })?;
        Ok(Some(TeamLeaderChatResult {
            team_id: snapshot.run.team_id.clone(),
            member_id: leader.member_id.clone(),
            execution_id,
            text: result.text,
            replay: false,
        }))
    }

    fn claim_team_task(&self, task_id: &str) -> AppResult<Option<TeamTask>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let task_id = task_id.to_string();
        self.runtime
            .run_sync(async move { claim_team_task_sqlx(&pool, &tenant_id, &task_id).await })
    }

    fn finish_team_task(
        &self,
        task_id: &str,
        state: TeamTaskState,
        result: Option<&str>,
        error_code: Option<&str>,
    ) -> AppResult<TeamTask> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let task_id = task_id.to_string();
        let result = result.map(ToString::to_string);
        let error_code = error_code.map(ToString::to_string);
        let task = self.runtime.run_sync(async move {
            finish_team_task_sqlx(
                &pool,
                &tenant_id,
                &task_id,
                state,
                result.as_deref(),
                error_code.as_deref(),
            )
            .await
        })?;
        // A task may also be completed through the scoped Team tool rather
        // than the resident worker. Run the same summary/finalization path in
        // either case, while keeping a summary failure recoverable.
        let _ = self.finalize_team_run(&task.run_id);
        Ok(task)
    }

    fn finalize_team_run(&self, run_id: &str) -> AppResult<()> {
        let Some(snapshot) = self.get_team_run(run_id)? else {
            return Err(AppError::NotFound(format!("Team run not found: {run_id}")));
        };
        if matches!(
            snapshot.run.state,
            crate::backend::models::TeamRunState::Executing
        ) && snapshot.tasks.iter().all(|task| task.state.is_terminal())
        {
            // Terminal task facts are committed to the mailbox before this
            // call. A failed summary leaves those facts unacknowledged and
            // the resident coordinator retries the same terminal run.
            let _ = self.consume_team_mailbox_and_summarize(&snapshot);
            let pool = self.db.pool().clone();
            let tenant_id = self.tenant_id().to_string();
            let run_id = run_id.to_string();
            self.runtime.run_sync(async move {
                mark_team_run_terminal_sqlx(&pool, &tenant_id, &run_id).await
            })?;
        } else if matches!(
            snapshot.run.state,
            crate::backend::models::TeamRunState::Terminal
        ) {
            // Recovery of a terminal run can resume after a process exit that
            // happened between task commit and mailbox acknowledgement.
            let _ = self.consume_team_mailbox_and_summarize(&snapshot);
        }
        Ok(())
    }

    fn cancel_team_run(&self, run_id: &str, error_code: &str) -> AppResult<()> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let run_id = run_id.to_string();
        let error_code = error_code.to_string();
        self.runtime.run_sync(async move {
            cancel_team_run_sqlx(&pool, &tenant_id, &run_id, &error_code).await
        })
    }

    pub(crate) fn send_team_mailbox(
        &self,
        input: TeamMailboxSendInput,
    ) -> AppResult<TeamMailboxMessage> {
        let snapshot = self
            .get_team_run(&input.run_id)?
            .ok_or_else(|| AppError::NotFound(format!("Team run not found: {}", input.run_id)))?;
        if snapshot.run.team_id != input.team_id
            || !snapshot
                .run
                .roster_snapshot
                .iter()
                .any(|member| member.member_id == input.sender_member_id)
            || !snapshot
                .run
                .roster_snapshot
                .iter()
                .any(|member| member.member_id == input.recipient_member_id)
        {
            return Err(AppError::Conflict(
                "Mailbox participants must belong to the frozen Team roster".to_string(),
            ));
        }
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.runtime
            .run_sync(async move { send_team_mailbox_sqlx(&pool, &tenant_id, &input).await })
    }

    pub(crate) fn read_team_mailbox(
        &self,
        input: TeamMailboxReadInput,
    ) -> AppResult<Vec<TeamMailboxMessage>> {
        let snapshot = self
            .get_team_run(&input.run_id)?
            .ok_or_else(|| AppError::NotFound(format!("Team run not found: {}", input.run_id)))?;
        if snapshot.run.team_id != input.team_id
            || !snapshot
                .run
                .roster_snapshot
                .iter()
                .any(|member| member.member_id == input.recipient_member_id)
        {
            return Err(AppError::Conflict(
                "Mailbox recipient must belong to the frozen Team roster".to_string(),
            ));
        }
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.runtime
            .run_sync(async move { read_team_mailbox_sqlx(&pool, &tenant_id, &input).await })
    }

    pub(crate) fn update_team_task(&self, input: TeamTaskUpdateInput) -> AppResult<TeamTask> {
        let task = self
            .get_team_task(&input.task_id)?
            .ok_or_else(|| AppError::NotFound(format!("Team task not found: {}", input.task_id)))?;
        if task.team_id != input.team_id || task.run_id != input.run_id {
            return Err(AppError::NotFound(
                "Team task is outside the requested run".to_string(),
            ));
        }
        if task.owner_member_id.as_deref() != Some(input.member_id.as_str()) {
            return Err(AppError::Conflict(
                "A teammate may update only its own task".to_string(),
            ));
        }
        if input.state == TeamTaskState::Running {
            let pool = self.db.pool().clone();
            let tenant_id = self.tenant_id().to_string();
            let task_id = input.task_id.clone();
            return self.runtime.run_sync(async move {
                crate::backend::store::mark_team_task_running_sqlx(&pool, &tenant_id, &task_id)
                    .await
            });
        }
        self.finish_team_task(
            &input.task_id,
            input.state,
            input.result.as_deref(),
            input.error_code.as_deref(),
        )
    }

    fn get_team_task(&self, task_id: &str) -> AppResult<Option<TeamTask>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let task_id = task_id.to_string();
        self.runtime
            .run_sync(async move { get_team_task_sqlx(&pool, &tenant_id, &task_id).await })
    }

    fn team_detail(&self, team_id: &str) -> AppResult<TeamDetail> {
        self.get_team(team_id)?
            .ok_or_else(|| AppError::NotFound(format!("Team not found: {team_id}")))
    }

    pub(crate) fn issue_team_tool_credential(
        &self,
        input: TeamToolCredentialInput,
    ) -> AppResult<TeamToolCredential> {
        let snapshot = self
            .get_team_run(&input.run_id)?
            .ok_or_else(|| AppError::NotFound(format!("Team run not found: {}", input.run_id)))?;
        let _member = snapshot
            .run
            .roster_snapshot
            .iter()
            .find(|member| member.member_id == input.member_id)
            .ok_or_else(|| {
                AppError::Conflict("Credential member is outside the frozen roster".to_string())
            })?;
        if snapshot.run.team_id != input.team_id {
            return Err(AppError::Conflict(
                "Credential scope does not match the frozen Team run".to_string(),
            ));
        }
        let ttl = input.ttl_seconds.unwrap_or(900);
        if ttl == 0 || ttl > 3600 {
            return Err(AppError::Validation(
                "Team tool credential TTL must be between 1 and 3600 seconds".to_string(),
            ));
        }
        let credential = format!("team-tool-{}", Uuid::new_v4().simple());
        let credential_hash = hash_team_tool_credential(&credential);
        let expires_at = (chrono::Utc::now() + chrono::Duration::seconds(ttl as i64)).to_rfc3339();
        let expires_at_for_store = expires_at.clone();
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.runtime.run_sync(async move {
            crate::backend::store::create_team_tool_credential_sqlx(
                &pool,
                &tenant_id,
                &credential_hash,
                &input,
                &expires_at_for_store,
            )
            .await
        })?;
        Ok(TeamToolCredential {
            credential,
            expires_at,
        })
    }

    pub(crate) fn team_tool_list_tasks(
        &self,
        credential: &str,
        input: TeamToolTaskListInput,
        member_id: &str,
    ) -> AppResult<Vec<TeamTask>> {
        self.authenticate_team_tool(credential, &input.team_id, &input.run_id, member_id)?;
        let snapshot = self
            .get_team_run(&input.run_id)?
            .ok_or_else(|| AppError::NotFound(format!("Team run not found: {}", input.run_id)))?;
        let is_leader = snapshot.run.roster_snapshot.iter().any(|member| {
            member.member_id == member_id && member.role == crate::backend::models::TeamRole::Leader
        });
        Ok(if is_leader {
            snapshot.tasks
        } else {
            snapshot
                .tasks
                .into_iter()
                .filter(|task| task.owner_member_id.as_deref() == Some(member_id))
                .collect()
        })
    }

    pub(crate) fn team_tool_update_task(
        &self,
        credential: &str,
        input: TeamTaskUpdateInput,
    ) -> AppResult<TeamTask> {
        self.authenticate_team_tool(credential, &input.team_id, &input.run_id, &input.member_id)?;
        let snapshot = self
            .get_team_run(&input.run_id)?
            .ok_or_else(|| AppError::NotFound(format!("Team run not found: {}", input.run_id)))?;
        let is_leader = snapshot.run.roster_snapshot.iter().any(|member| {
            member.member_id == input.member_id
                && member.role == crate::backend::models::TeamRole::Leader
        });
        if is_leader {
            let task = self.get_team_task(&input.task_id)?.ok_or_else(|| {
                AppError::NotFound(format!("Team task not found: {}", input.task_id))
            })?;
            if task.team_id != input.team_id || task.run_id != input.run_id {
                return Err(AppError::NotFound(
                    "Team task is outside the requested run".to_string(),
                ));
            }
            if input.state == TeamTaskState::Running {
                let pool = self.db.pool().clone();
                let tenant_id = self.tenant_id().to_string();
                let task_id = input.task_id.clone();
                self.runtime.run_sync(async move {
                    crate::backend::store::mark_team_task_running_sqlx(&pool, &tenant_id, &task_id)
                        .await
                })
            } else {
                self.finish_team_task(
                    &input.task_id,
                    input.state,
                    input.result.as_deref(),
                    input.error_code.as_deref(),
                )
            }
        } else {
            self.update_team_task(input)
        }
    }

    pub(crate) fn team_tool_send_mailbox(
        &self,
        credential: &str,
        input: TeamMailboxSendInput,
    ) -> AppResult<TeamMailboxMessage> {
        self.authenticate_team_tool(
            credential,
            &input.team_id,
            &input.run_id,
            &input.sender_member_id,
        )?;
        self.send_team_mailbox(input)
    }

    pub(crate) fn team_tool_read_mailbox(
        &self,
        credential: &str,
        input: TeamMailboxReadInput,
    ) -> AppResult<Vec<TeamMailboxMessage>> {
        self.authenticate_team_tool(
            credential,
            &input.team_id,
            &input.run_id,
            &input.recipient_member_id,
        )?;
        self.read_team_mailbox(input)
    }

    fn authenticate_team_tool(
        &self,
        credential: &str,
        team_id: &str,
        run_id: &str,
        member_id: &str,
    ) -> AppResult<()> {
        if credential.trim().is_empty() {
            return Err(AppError::Conflict(
                "Team tool credential is required".to_string(),
            ));
        }
        let hash = hash_team_tool_credential(credential);
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let team_id = team_id.to_string();
        let run_id = run_id.to_string();
        let member_id = member_id.to_string();
        let valid = self.runtime.run_sync(async move {
            crate::backend::store::authenticate_team_tool_sqlx(
                &pool, &tenant_id, &hash, &team_id, &run_id, &member_id,
            )
            .await
        })?;
        if valid {
            Ok(())
        } else {
            Err(AppError::Conflict(
                "Team tool credential is invalid or expired".to_string(),
            ))
        }
    }
}

fn hash_team_tool_credential(credential: &str) -> String {
    format!("{:x}", Sha256::digest(credential.as_bytes()))
}

fn team_runtime_projection(snapshot: &TeamRunSnapshot) -> Value {
    serde_json::json!({
        "run_id": snapshot.run.id,
        "state": snapshot.run.state,
        "revision": snapshot.run.revision,
        "tasks": snapshot.tasks.iter().map(|task| serde_json::json!({
            "task_id": task.id,
            "owner_member_id": task.owner_member_id,
            "state": task.state,
            "error_code": task.error_code,
        })).collect::<Vec<_>>(),
    })
}

fn unavailable_member_restore_status(
    member: &crate::backend::models::TeamRosterSnapshotMember,
) -> crate::backend::models::TeamMemberRestoreStatus {
    crate::backend::models::TeamMemberRestoreStatus {
        member_id: member.member_id.clone(),
        role: member.role,
        state: crate::backend::models::TeamMemberRestoreState::Unavailable,
        error_code: Some("resume_unavailable".to_string()),
    }
}

fn parse_draft(text: &str) -> AppResult<Vec<TeamTaskDraft>> {
    #[derive(Deserialize)]
    struct Envelope {
        tasks: Vec<TeamTaskDraft>,
    }
    let value: Value = serde_json::from_str(text.trim()).map_err(|_| {
        AppError::Validation("Leader response is not valid Team draft JSON".to_string())
    })?;
    let drafts = if value.is_array() {
        serde_json::from_value::<Vec<TeamTaskDraft>>(value)
    } else {
        serde_json::from_value::<Envelope>(value).map(|envelope| envelope.tasks)
    }
    .map_err(|_| {
        AppError::Validation("Leader response does not match the Team task schema".to_string())
    })?;
    if drafts.is_empty() {
        return Err(AppError::Validation(
            "Leader draft contains no tasks".to_string(),
        ));
    }
    Ok(drafts)
}

fn ai_error(error: crate::backend::ai_execution::AiExecutionError) -> AppError {
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
    use super::*;
    use crate::backend::{
        agents::types::{AgentCatalogEntry, AgentId, AgentProtocol, DeclaredAgentCapabilities},
        ai_execution::{executor::BackendFuture, AgentExecutionRuntime, AiExecutionResult},
        models::{CreateTeamInput, TeamMemberInput, TeamReviewTaskInput, TeamRole, TeamTaskState},
    };
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct RequestObservation {
        purpose: AiExecutionPurpose,
        agent_id: String,
        replay: bool,
        prompt: String,
    }

    struct FakeTeamRuntime {
        observations: Arc<Mutex<Vec<RequestObservation>>>,
    }

    impl FakeTeamRuntime {
        fn new() -> (Arc<Self>, Arc<Mutex<Vec<RequestObservation>>>) {
            let observations = Arc::new(Mutex::new(Vec::new()));
            (
                Arc::new(Self {
                    observations: observations.clone(),
                }),
                observations,
            )
        }
    }

    impl AgentExecutionRuntime for FakeTeamRuntime {
        fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
            let observations = self.observations.clone();
            Box::pin(async move {
                observations.lock().unwrap().push(RequestObservation {
                    purpose: request.purpose,
                    agent_id: request.agent_id.to_string(),
                    replay: request.replay,
                    prompt: request.prompt.clone(),
                });
                let text = match request.purpose {
                    AiExecutionPurpose::TeamDraft => {
                        r#"{"tasks":[{"id":"task-a","title":"A work","description":"Work for A","recommended_member_id":"member-a"},{"id":"task-b","title":"B work","description":"Work for B","recommended_member_id":"member-b"}]}"#.to_string()
                    }
                    AiExecutionPurpose::TeamLeaderChat if request.replay => String::new(),
                    AiExecutionPurpose::TeamLeaderChat => "leader reply".to_string(),
                    AiExecutionPurpose::TeamTask => format!("done by {}", request.agent_id),
                    AiExecutionPurpose::TeamSummary => "summary reply".to_string(),
                    _ => "unused".to_string(),
                };
                Ok(AiExecutionResult {
                    text,
                    agent_id: request.agent_id,
                    protocol: AgentProtocol::Acp,
                    requested_model: request.model,
                    elapsed_ms: 1,
                    persistent_binding: None,
                    replay_text: request
                        .replay
                        .then(|| "replayed leader history".to_string()),
                })
            })
        }

        fn list_agent_catalog(&self) -> Vec<AgentCatalogEntry> {
            ["leader-agent", "agent-a", "agent-b"]
                .into_iter()
                .map(|id| AgentCatalogEntry {
                    id: id.to_string(),
                    display_name: id.to_string(),
                    command: "fixture".to_string(),
                    args: Vec::new(),
                    availability_command: "fixture".to_string(),
                    protocol: "acp".to_string(),
                    capabilities: DeclaredAgentCapabilities {
                        text_prompt: true,
                        resume: true,
                        history_replay: true,
                        live_events: true,
                        rich_history_replay: false,
                        team_tools: true,
                        resume_args: None,
                    },
                })
                .collect()
        }

        fn agent_capabilities(&self, _agent_id: &AgentId) -> Option<DeclaredAgentCapabilities> {
            Some(DeclaredAgentCapabilities {
                text_prompt: true,
                resume: true,
                history_replay: true,
                live_events: true,
                rich_history_replay: false,
                team_tools: true,
                resume_args: None,
            })
        }
    }

    #[test]
    fn team_workflow_freezes_roster_requires_review_and_uses_confirmed_owners() {
        let root = std::env::temp_dir().join(format!("assetiweave-team-flow-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let (runtime, observations) = FakeTeamRuntime::new();
        let service = AppService::open_with_db_path_and_runtime(root.join("app.db"), runtime)
            .expect("open fixture service");
        let team = service
            .create_team(CreateTeamInput {
                id: Some("team-flow".to_string()),
                name: "Flow".to_string(),
                description: None,
                members: vec![
                    TeamMemberInput {
                        id: Some("leader".to_string()),
                        role: TeamRole::Leader,
                        sort_order: Some(99),
                        agent_id: "leader-agent".to_string(),
                        model: None,
                    },
                    TeamMemberInput {
                        id: Some("member-a".to_string()),
                        role: TeamRole::Teammate,
                        sort_order: Some(1),
                        agent_id: "agent-a".to_string(),
                        model: None,
                    },
                    TeamMemberInput {
                        id: Some("member-b".to_string()),
                        role: TeamRole::Teammate,
                        sort_order: Some(0),
                        agent_id: "agent-b".to_string(),
                        model: None,
                    },
                ],
            })
            .expect("create Team");
        assert_eq!(
            team.members
                .iter()
                .map(|member| member.sort_order)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );

        let live = service
            .leader_chat(TeamLeaderChatInput {
                team_id: team.team.id.clone(),
                message: "hello".to_string(),
                replay: false,
            })
            .expect("leader chat");
        assert_eq!(live.text, "leader reply");
        let replay = service
            .leader_chat(TeamLeaderChatInput {
                team_id: team.team.id.clone(),
                message: String::new(),
                replay: true,
            })
            .expect("leader replay");
        assert_eq!(replay.text, "replayed leader history");

        let shell = service
            .draft_team(TeamDraftInput {
                team_id: team.team.id.clone(),
                leader_message: "split the work".to_string(),
            })
            .expect("draft shell");
        assert_eq!(
            shell.run.state,
            crate::backend::models::TeamRunState::Drafting
        );
        let draft_task_id = format!("team-task-draft-{}", shell.run.id);
        wait_for_task_terminal(&service, &draft_task_id);
        let draft = wait_for_run_state(
            &service,
            &shell.run.id,
            crate::backend::models::TeamRunState::AwaitingReview,
        );
        assert_eq!(draft.tasks.len(), 2);
        assert!(draft
            .tasks
            .iter()
            .all(|task| task.owner_member_id.is_none()));

        let reviewed = service
            .review_team_run(TeamReviewInput {
                run_id: draft.run.id.clone(),
                revision: draft.run.revision,
                tasks: vec![
                    TeamReviewTaskInput {
                        task_id: "task-b".to_string(),
                        owner_member_id: "member-a".to_string(),
                        sort_order: 100,
                    },
                    TeamReviewTaskInput {
                        task_id: "task-a".to_string(),
                        owner_member_id: "member-b".to_string(),
                        sort_order: -100,
                    },
                ],
            })
            .expect("review draft");
        assert_eq!(
            reviewed.tasks[0].owner_member_id.as_deref(),
            Some("member-a")
        );
        assert_eq!(
            reviewed.tasks[1].owner_member_id.as_deref(),
            Some("member-b")
        );

        let confirmed = service
            .confirm_team_run(TeamConfirmInput {
                run_id: reviewed.run.id.clone(),
                revision: reviewed.run.revision,
            })
            .expect("confirm run");
        assert_eq!(
            confirmed.run.state,
            crate::backend::models::TeamRunState::Executing
        );
        let terminal = wait_for_run_state(
            &service,
            &confirmed.run.id,
            crate::backend::models::TeamRunState::Terminal,
        );
        assert_eq!(terminal.tasks[0].state, TeamTaskState::Succeeded);
        assert_eq!(terminal.tasks[1].state, TeamTaskState::Succeeded);
        assert_eq!(
            terminal.tasks[0].owner_member_id.as_deref(),
            Some("member-a")
        );
        assert_eq!(
            terminal.tasks[1].owner_member_id.as_deref(),
            Some("member-b")
        );
        assert_eq!(terminal.unread_mailbox_count, 0);

        let observations = observations.lock().unwrap().clone();
        let task_agents = observations
            .iter()
            .filter(|observation| observation.purpose == AiExecutionPurpose::TeamTask)
            .map(|observation| observation.agent_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(task_agents, vec!["agent-a", "agent-b"]);
        assert!(observations
            .iter()
            .filter(|observation| observation.replay)
            .all(|observation| observation.purpose == AiExecutionPurpose::TeamLeaderChat));
        assert!(observations
            .iter()
            .all(|observation| !observation.prompt.contains("team-tool-")));

        drop(service);
        std::fs::remove_dir_all(root).ok();
    }

    fn wait_for_task_terminal(service: &AppService, task_id: &str) {
        for _ in 0..200 {
            if service
                .team_run_task(task_id)
                .expect("read task")
                .is_some_and(|task| task.state.is_terminal())
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("Team task did not become terminal: {task_id}");
    }

    fn wait_for_run_state(
        service: &AppService,
        run_id: &str,
        state: crate::backend::models::TeamRunState,
    ) -> TeamRunSnapshot {
        for _ in 0..200 {
            if let Some(snapshot) = service.get_team_run(run_id).expect("read Team run") {
                if snapshot.run.state == state {
                    return snapshot;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("Team run did not become {state:?}: {run_id}");
    }
}
