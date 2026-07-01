import { invoke } from "@tauri-apps/api/core";
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

export interface ConversationCardTranslationRequest {
  cli: ConversationTranslationCli;
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
  cli: ConversationTranslationCli;
  model: string;
  prompt: string;
  provider: ConversationTranslationProvider;
}

export interface ConversationTranslationConnectionRequest {
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
  cli: ConversationTranslationCli;
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
  if (request.provider === "cli" && request.cli === "opencode") {
    return checkOpencodeTranslationAvailability();
  }

  return testConversationTranslationConnection({
    ...request,
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
      cli: request.cli,
      model: request.model,
      prompt,
      provider: request.provider,
    } satisfies ConversationTranslationCommandParams,
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
    params: request,
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
