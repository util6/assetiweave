/* @vitest-environment jsdom */

import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AiExecutionTaskSnapshot } from "../../services/cardTranslation";
import {
  DEFAULT_CONVERSATION_TRANSLATION_PROMPT_TEMPLATE,
  DEFAULT_CONVERSATION_TRANSLATION_TARGET_LANGUAGE,
  type ResolvedConversationTranslationSettings,
} from "../../store/settings/AppSettingsProvider";
import { useConversationContentController, type ConversationTranslationTaskController } from "./useConversationContentController";
import type { ConversationContentBlock } from "./ConversationContentCards";

const t = ((key: string) => key) as never;
const settings: ResolvedConversationTranslationSettings = {
  agentId: "opencode",
  model: "test-model",
  promptTemplate: DEFAULT_CONVERSATION_TRANSLATION_PROMPT_TEMPLATE,
  provider: "cli",
  targetLanguage: DEFAULT_CONVERSATION_TRANSLATION_TARGET_LANGUAGE,
};
const block: ConversationContentBlock = {
  id: "block-1",
  partId: "part-1",
  role: "assistant",
  text: "hello",
  type: "answer",
};

function task(overrides: Partial<AiExecutionTaskSnapshot> = {}): AiExecutionTaskSnapshot {
  return {
    agent_id: "opencode",
    created_at: "2026-08-16T00:00:00Z",
    error: null,
    finished_at: null,
    id: "task-1",
    phase: "prompting",
    purpose: "translation",
    result: null,
    state: "running",
    updated_at: "2026-08-16T00:00:01Z",
    ...overrides,
  };
}

afterEach(() => cleanup());

describe("useConversationContentController", () => {
  it("keeps expanded and translated state across rerenders and isolates a new question scope", async () => {
    const translationAvailabilityChecker = vi.fn(async () => ({ available: true, error: null, version: "1" }));
    const translationSaver = vi.fn(async () => undefined);
    const { result, rerender } = renderHook(
      ({ scopeKey }) => useConversationContentController({
        blocks: [block],
        recordKind: "session",
        scopeKey,
        t,
        translationAvailabilityChecker,
        translationSaver,
        translationSettings: settings,
        translator: async () => ({ translated_text: "你好" }),
      }),
      { initialProps: { scopeKey: "question-1" } },
    );

    await waitFor(() => expect(result.current.translationAvailability).toBe("available"));
    act(() => result.current.toggleExpanded(block.id));
    expect(result.current.expandedBlockIds.has(block.id)).toBe(true);
    await act(async () => result.current.translateBlock(block));
    expect(result.current.getTranslatedText(block)).toBe("你好");
    expect(translationSaver).toHaveBeenCalledWith(expect.objectContaining({ partId: "part-1", translatedText: "你好" }));
    expect(translationAvailabilityChecker).toHaveBeenCalledTimes(1);

    rerender({ scopeKey: "question-2" });
    expect(result.current.expandedBlockIds.has(block.id)).toBe(false);
    expect(result.current.getTranslatedText(block)).toBeUndefined();
  });

  it("restores task phase and result without starting a second task", async () => {
    let currentTask = task();
    const controller: ConversationTranslationTaskController = {
      cancelTask: vi.fn(async () => {
        currentTask = task({ state: "cancelled", phase: "cancelling" });
        return currentTask;
      }),
      startTranslation: vi.fn(async () => currentTask),
      tasks: [],
    };
    const translationAvailabilityChecker = vi.fn(async () => ({ available: true, error: null, version: "1" }));
    const { result, rerender, unmount } = renderHook(() => useConversationContentController({
      blocks: [block],
      recordKind: "session",
      t,
      translationAvailabilityChecker,
      translationSettings: settings,
      translationTaskController: controller,
    }));

    await waitFor(() => expect(result.current.translationAvailability).toBe("available"));
    await act(async () => result.current.translateBlock(block));
    expect(controller.startTranslation).toHaveBeenCalledTimes(1);
    controller.tasks = [currentTask];
    rerender();
    expect(result.current.isTranslating(block.id)).toBe(true);
    expect(result.current.getTranslationPhase(block.id)).toBe("prompting");

    currentTask = task({
      result: { text: "translated" },
      state: "succeeded",
      updated_at: "2026-08-16T00:00:02Z",
    });
    controller.tasks = [currentTask];
    rerender();
    await waitFor(() => expect(result.current.getTranslatedText(block)).toBe("translated"));
    expect(controller.startTranslation).toHaveBeenCalledTimes(1);
    unmount();
  });
});
