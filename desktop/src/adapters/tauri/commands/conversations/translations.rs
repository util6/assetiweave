use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

use crate::adapters::app_state::AppState;
use crate::adapters::tauri::background_tasks::{AiExecutionTaskSnapshot, BackgroundTaskRegistry};
use crate::adapters::tauri::commands::agents::AI_EXECUTION_TASK_UPDATED_EVENT;
use crate::backend::application::conversations::card_translation::{
    ConversationTranslationConnectionRequest, ConversationTranslationModelsRequest,
    ConversationTranslationModelsResult, ConversationTranslationRequest,
    OpencodeTranslationAvailability, OpencodeTranslationRequest, OpencodeTranslationResult,
    PreparedConversationCardTranslation, PromptOptimizationRequest, PromptOptimizationResult,
};
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::application::ConversationPartTranslationUpdateParams;
use crate::backend::infrastructure::agent_execution::{
    AiExecutionCancellation, AiExecutionCleanupReport, AiExecutionError, AiExecutionPhase,
    AiExecutionProgressSink, AiExecutionPurpose,
};

pub(crate) trait AiExecutionTaskEmitter: Send + Sync {
    fn emit(&self, snapshot: &AiExecutionTaskSnapshot);
}

pub(crate) struct TauriAiExecutionTaskEmitter {
    app: AppHandle,
}

impl AiExecutionTaskEmitter for TauriAiExecutionTaskEmitter {
    fn emit(&self, snapshot: &AiExecutionTaskSnapshot) {
        if let Err(error) = self.app.emit(AI_EXECUTION_TASK_UPDATED_EVENT, snapshot) {
            tracing::error!(
                action = "ai_execution.task",
                task_id = %snapshot.id,
                error = %error,
                "推送 AI 执行任务状态失败"
            );
        }
    }
}

pub(crate) struct RegistryAiExecutionProgressSink {
    tasks: Arc<BackgroundTaskRegistry>,
    task_id: String,
    emitter: Arc<dyn AiExecutionTaskEmitter>,
    last_execution_phase: Mutex<Option<AiExecutionPhase>>,
}

impl AiExecutionProgressSink for RegistryAiExecutionProgressSink {
    fn set_phase(&self, phase: AiExecutionPhase) {
        if !matches!(
            phase,
            AiExecutionPhase::Cancelling | AiExecutionPhase::Closing | AiExecutionPhase::CleaningUp
        ) {
            if let Ok(mut last_phase) = self.last_execution_phase.lock() {
                *last_phase = Some(phase);
            }
        }
        match self.tasks.update_ai_execution_phase(&self.task_id, phase) {
            Ok(snapshot) => self.emitter.emit(&snapshot),
            Err(error) => tracing::error!(
                action = "ai_execution.task",
                task_id = %self.task_id,
                error = %error,
                "更新 AI 执行任务阶段失败"
            ),
        }
    }

    fn failure_phase(&self) -> Option<AiExecutionPhase> {
        self.last_execution_phase
            .lock()
            .ok()
            .and_then(|last_phase| *last_phase)
    }

    fn set_cleanup_report(&self, report: AiExecutionCleanupReport) {
        match self
            .tasks
            .update_ai_execution_cleanup(&self.task_id, report)
        {
            Ok(snapshot) => self.emitter.emit(&snapshot),
            Err(error) => tracing::error!(
                action = "ai_execution.task",
                task_id = %self.task_id,
                error = %error,
                "更新 AI 执行清理报告失败"
            ),
        }
    }
}

pub(crate) fn prepare_ai_execution_task_for_tenant(
    tenant_id: &str,
    tasks: Arc<BackgroundTaskRegistry>,
    prepared: PreparedConversationCardTranslation,
    emitter: Arc<dyn AiExecutionTaskEmitter>,
) -> RuntimeAppResult<(
    AiExecutionTaskSnapshot,
    PreparedConversationCardTranslation,
    AiExecutionCancellation,
    Arc<dyn AiExecutionProgressSink>,
)> {
    let (snapshot, cancellation) = tasks.begin_ai_execution_for_tenant(
        tenant_id,
        AiExecutionPurpose::Translation,
        &prepared.0,
    )?;
    let progress: Arc<dyn AiExecutionProgressSink> = Arc::new(RegistryAiExecutionProgressSink {
        tasks,
        task_id: snapshot.id.clone(),
        emitter: emitter.clone(),
        last_execution_phase: Mutex::new(None),
    });
    emitter.emit(&snapshot);
    Ok((snapshot, prepared, cancellation, progress))
}

pub(crate) async fn run_ai_execution_task(
    tasks: Arc<BackgroundTaskRegistry>,
    service: AppService,
    task_id: String,
    prepared: PreparedConversationCardTranslation,
    cancellation: AiExecutionCancellation,
    progress: Arc<dyn AiExecutionProgressSink>,
    emitter: Arc<dyn AiExecutionTaskEmitter>,
) {
    let execution_id = task_id.clone();
    let execution_progress = progress.clone();
    let execution = tokio::spawn(async move {
        service
            .execute_prepared_conversation_card_translation(
                prepared,
                execution_id,
                cancellation,
                Some(execution_progress),
            )
            .await
    });
    let result = match execution.await {
        Ok(result) => result,
        Err(_) => Err(AiExecutionError::Protocol {
            operation: "execution_task_panicked",
        }),
    };
    let failure_phase = progress.failure_phase();
    match tasks.finish_ai_execution_with_phase(&task_id, result, failure_phase) {
        Ok(snapshot) => emitter.emit(&snapshot),
        Err(error) => tracing::error!(
            action = "ai_execution.task",
            task_id = %task_id,
            error = %error,
            "收敛 AI 执行任务状态失败"
        ),
    }
}

#[tauri::command]
pub(crate) async fn check_opencode_translation_availability(
    state: State<'_, AppState>,
) -> RuntimeAppResult<OpencodeTranslationAvailability> {
    AppService::from_runtime(&state.runtime).check_opencode_translation_availability()
}

#[tauri::command]
pub(crate) async fn check_prompt_optimization_availability(
    state: State<'_, AppState>,
) -> RuntimeAppResult<
    crate::backend::application::conversations::card_translation::ActionAvailability,
> {
    AppService::from_runtime(&state.runtime).check_prompt_optimization_availability()
}

#[tauri::command]
pub(crate) async fn translate_conversation_card_with_opencode(
    state: State<'_, AppState>,
    params: OpencodeTranslationRequest,
) -> RuntimeAppResult<OpencodeTranslationResult> {
    AppService::from_runtime(&state.runtime)
        .translate_conversation_card_with_opencode(params)
        .await
}

#[tauri::command]
pub(crate) async fn translate_conversation_card(
    state: State<'_, AppState>,
    params: ConversationTranslationRequest,
) -> RuntimeAppResult<OpencodeTranslationResult> {
    AppService::from_runtime(&state.runtime)
        .translate_conversation_card(params)
        .await
}

#[tauri::command]
pub(crate) async fn optimize_prompt(
    state: State<'_, AppState>,
    params: PromptOptimizationRequest,
) -> RuntimeAppResult<PromptOptimizationResult> {
    AppService::from_runtime(&state.runtime)
        .optimize_prompt(params)
        .await
}

#[tauri::command]
pub(crate) async fn test_conversation_translation_connection(
    state: State<'_, AppState>,
    params: ConversationTranslationConnectionRequest,
) -> RuntimeAppResult<OpencodeTranslationAvailability> {
    AppService::from_runtime(&state.runtime)
        .test_conversation_translation_connection(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_conversation_translation_models(
    state: State<'_, AppState>,
    params: ConversationTranslationModelsRequest,
) -> RuntimeAppResult<ConversationTranslationModelsResult> {
    AppService::from_runtime(&state.runtime).list_conversation_translation_models(params)
}

#[tauri::command]
pub(crate) async fn start_conversation_card_translation(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationTranslationRequest,
) -> RuntimeAppResult<AiExecutionTaskSnapshot> {
    let emitter: Arc<dyn AiExecutionTaskEmitter> = Arc::new(TauriAiExecutionTaskEmitter { app });
    let tasks = state.background_tasks.clone();
    let tenant_id = state.runtime.context().tenant.id.clone();
    let prepared = AppService::prepare_conversation_card_translation(params)?;
    let (snapshot, prepared, cancellation, progress) =
        prepare_ai_execution_task_for_tenant(&tenant_id, tasks.clone(), prepared, emitter.clone())?;
    let service = AppService::from_runtime(&state.runtime);
    let task_id = snapshot.id.clone();
    tauri::async_runtime::spawn(run_ai_execution_task(
        tasks,
        service,
        task_id,
        prepared,
        cancellation,
        progress,
        emitter,
    ));
    Ok(snapshot)
}

#[tauri::command]
pub(crate) async fn update_conversation_part_translation(
    state: State<'_, AppState>,
    params: ConversationPartTranslationUpdateParams,
) -> RuntimeAppResult<()> {
    AppService::from_runtime(&state.runtime)
        .update_conversation_part_translation(params)
        .await
}
