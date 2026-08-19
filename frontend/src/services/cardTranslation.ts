import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isTauriRuntime } from "./appUpdater";
import {
  DEFAULT_CONVERSATION_TRANSLATION_PROMPT_TEMPLATE,
  normalizeConversationTranslationTargetLanguage,
  type ConversationTranslationCli,
  type ConversationTranslationProvider,
  type ConversationTranslationTargetLanguage,
} from "../store/settings/settingsSchema";

export interface OpencodeTranslationAvailability {
  available: boolean;
  error: string | null;
  version: string | null;
}

export interface OpencodeTranslationResult {
  translated_text: string;
}

export type AiExecutionTaskState =
  | "queued"
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled";

const AI_EXECUTION_TASK_UPDATED_EVENT = "ai-execution://task-updated";

export type AiExecutionPhase =
  | "queued"
  | "resolving"
  | "spawning"
  | "initializing"
  | "creating_session"
  | "configuring"
  | "prompting"
  | "cancelling"
  | "closing"
  | "cleaning_up";

export interface AiExecutionErrorView {
  code: string;
  message: string;
  retryable: boolean;
  phase: AiExecutionPhase | null;
}

export interface AiExecutionTaskSnapshot {
  id: string;
  purpose: "translation" | "connection_test";
  agent_id: string;
  state: AiExecutionTaskState;
  phase: AiExecutionPhase;
  created_at: string;
  updated_at: string;
  finished_at: string | null;
  result: { text: string } | null;
  error: AiExecutionErrorView | null;
}

export interface ConversationCardTranslationRequest {
  agentId: string;
  model: string;
  promptTemplate?: string;
  provider: ConversationTranslationProvider;
  targetLanguage: ConversationTranslationTargetLanguage;
  text: string;
}

export interface ConversationCardTranslationPromptRequest {
  promptTemplate?: string;
  targetLanguage: ConversationTranslationTargetLanguage;
  text: string;
}

export interface ConversationTranslationCommandParams {
  agent_id: string;
  model: string;
  prompt: string;
  provider: ConversationTranslationProvider;
}

export interface ConversationTranslationConnectionRequest {
  agentId?: string;
  cli: ConversationTranslationCli;
  model: string;
  prompt: string;
  provider: ConversationTranslationProvider;
}

export interface ConversationTranslationModelsRequest {
  cli: ConversationTranslationCli;
  provider: ConversationTranslationProvider;
}

export interface ConversationTranslationAvailabilityRequest {
  agentId: string;
  model: string;
  provider: ConversationTranslationProvider;
}

export interface ConversationTranslationModelsResult {
  error: string | null;
  models: string[];
}

export interface ConversationPartTranslationUpdateRequest {
  partId: string;
  recordKind: "session" | "web";
  translatedText: string;
}

export function buildConversationCardTranslationPrompt({
  promptTemplate,
  targetLanguage,
  text,
}: ConversationCardTranslationPromptRequest) {
  const normalizedTargetLanguage = normalizeConversationTranslationTargetLanguage(targetLanguage);
  const template = promptTemplate?.trim() || DEFAULT_CONVERSATION_TRANSLATION_PROMPT_TEMPLATE;
  const rendered = template
    .split("{targetLanguageJson}").join(JSON.stringify(normalizedTargetLanguage))
    .split("{targetLanguage}").join(normalizedTargetLanguage)
    .split("{content}").join(text);

  return rendered.includes(text) ? rendered : `${rendered}\n\n<content>\n${text}\n</content>`;
}

export async function checkOpencodeTranslationAvailability(): Promise<OpencodeTranslationAvailability> {
  if (!isTauriRuntime()) {
    return {
      available: false,
      error: "opencode translation requires the desktop app runtime",
      version: null,
    };
  }

  return invoke<OpencodeTranslationAvailability>("check_opencode_translation_availability");
}

export async function checkConversationTranslationAvailability(
  request: ConversationTranslationAvailabilityRequest,
): Promise<OpencodeTranslationAvailability> {
  if (request.provider === "cli" && request.agentId === "opencode") {
    return checkOpencodeTranslationAvailability();
  }

  return testConversationTranslationConnection({
    ...request,
    cli: request.agentId === "gemini" ? "gemini" : "opencode",
    agentId: request.agentId,
    prompt: "Reply with OK only.",
  });
}

export async function translateConversationCardContent(
  request: ConversationCardTranslationRequest,
): Promise<OpencodeTranslationResult> {
  if (!isTauriRuntime()) {
    throw new Error("opencode translation requires the desktop app runtime");
  }

  const prompt = buildConversationCardTranslationPrompt(request);
  return invoke<OpencodeTranslationResult>("translate_conversation_card", {
    params: {
      agent_id: request.agentId,
      model: request.model,
      prompt,
      provider: request.provider,
    } satisfies ConversationTranslationCommandParams,
  });
}

export async function startConversationCardTranslation(
  request: ConversationCardTranslationRequest,
): Promise<AiExecutionTaskSnapshot> {
  assertDesktopTranslationRuntime();
  const prompt = buildConversationCardTranslationPrompt(request);
  return invoke<AiExecutionTaskSnapshot>("start_conversation_card_translation", {
    params: {
      agent_id: request.agentId,
      model: request.model,
      prompt,
      provider: request.provider,
    } satisfies ConversationTranslationCommandParams,
  });
}

export async function getAiExecutionTask(
  taskId: string,
): Promise<AiExecutionTaskSnapshot | null> {
  assertDesktopTranslationRuntime();
  return invoke<AiExecutionTaskSnapshot | null>("get_ai_execution_task", {
    params: { task_id: taskId },
  });
}

export async function listAiExecutionTasks(): Promise<AiExecutionTaskSnapshot[]> {
  assertDesktopTranslationRuntime();
  return invoke<AiExecutionTaskSnapshot[]>("list_ai_execution_tasks");
}

export function subscribeAiExecutionTasks(listener: (snapshot: AiExecutionTaskSnapshot) => void) {
  if (!isTauriRuntime()) {
    return Promise.resolve(() => undefined);
  }
  return listen<AiExecutionTaskSnapshot>(AI_EXECUTION_TASK_UPDATED_EVENT, (event) => {
    listener(event.payload);
  });
}

export async function cancelAiExecutionTask(
  taskId: string,
): Promise<AiExecutionTaskSnapshot> {
  assertDesktopTranslationRuntime();
  return invoke<AiExecutionTaskSnapshot>("cancel_ai_execution_task", {
    params: { task_id: taskId },
  });
}

export async function testConversationTranslationConnection(
  request: ConversationTranslationConnectionRequest,
): Promise<OpencodeTranslationAvailability> {
  if (!isTauriRuntime()) {
    return {
      available: false,
      error: "translation connection testing requires the desktop app runtime",
      version: null,
    };
  }

  return invoke<OpencodeTranslationAvailability>("test_conversation_translation_connection", {
    params: {
      agent_id: request.agentId,
      cli: request.cli,
      model: request.model,
      prompt: request.prompt,
      provider: request.provider,
    },
  });
}

export async function listConversationTranslationModels(
  request: ConversationTranslationModelsRequest,
): Promise<ConversationTranslationModelsResult> {
  if (!isTauriRuntime()) {
    return {
      error: "translation model listing requires the desktop app runtime",
      models: [],
    };
  }

  return invoke<ConversationTranslationModelsResult>("list_conversation_translation_models", {
    params: request,
  });
}

export async function updateConversationPartTranslation(
  request: ConversationPartTranslationUpdateRequest,
): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }

  await invoke<void>("update_conversation_part_translation", {
    params: {
      part_id: request.partId,
      record_kind: request.recordKind,
      translated_text: request.translatedText,
    },
  });
}

function assertDesktopTranslationRuntime() {
  if (!isTauriRuntime()) {
    throw new Error("AI execution tasks require the desktop app runtime");
  }
}
