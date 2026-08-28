import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./appUpdater";
import type { ConversationTranslationProvider } from "../store/settings/settingsSchema";
import { DEFAULT_PROMPT_OPTIMIZATION_PROMPT_TEMPLATE } from "../store/settings/settingsSchema";
import type { OpencodeTranslationAvailability } from "./cardTranslation";

export interface PromptOptimizationRequest {
  agentId: string;
  model: string;
  promptTemplate?: string;
  provider: ConversationTranslationProvider;
  text: string;
}

export interface PromptOptimizationResult {
  optimized_text: string;
}

interface PromptOptimizationCommandParams {
  agent_id: string;
  model: string;
  prompt: string;
  provider: ConversationTranslationProvider;
}

export function buildPromptOptimizationPrompt({
  promptTemplate,
  text,
}: Pick<PromptOptimizationRequest, "promptTemplate" | "text">) {
  const template = promptTemplate?.trim() || DEFAULT_PROMPT_OPTIMIZATION_PROMPT_TEMPLATE;
  const rendered = template.split("{content}").join(text);
  return rendered.includes(text) ? rendered : `${rendered}\n\n<content>\n${text}\n</content>`;
}

export async function optimizePromptContent(
  request: PromptOptimizationRequest,
): Promise<PromptOptimizationResult> {
  if (!isTauriRuntime()) {
    throw new Error("prompt optimization requires the desktop app runtime");
  }

  return invoke<PromptOptimizationResult>("optimize_prompt", {
    params: {
      agent_id: request.agentId,
      model: request.model,
      prompt: buildPromptOptimizationPrompt(request),
      provider: request.provider,
    } satisfies PromptOptimizationCommandParams,
  });
}

export async function checkPromptOptimizationAvailability(): Promise<OpencodeTranslationAvailability> {
  if (!isTauriRuntime()) {
    return {
      available: false,
      error: "prompt optimization requires the desktop app runtime",
      version: null,
    };
  }
  return invoke<OpencodeTranslationAvailability>("check_prompt_optimization_availability");
}
