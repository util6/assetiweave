import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Translator } from "../../i18n/I18nProvider";
import {
  checkConversationTranslationAvailability,
  updateConversationPartTranslation,
  type AiExecutionPhase,
  type AiExecutionTaskSnapshot,
  type ConversationCardTranslationRequest,
  type ConversationPartTranslationUpdateRequest,
  type OpencodeTranslationAvailability,
  type OpencodeTranslationResult,
} from "../../services/cardTranslation";
import { useOptionalAiExecutionTasks } from "../../app/backgroundTasks/AiExecutionTaskProvider";
import type { ConversationContentBlock } from "./ConversationContentCards";
import type { ConversationRecordKind } from "../../types";
import type {
  ResolvedConversationTranslationSettings,
} from "../../store/settings/AppSettingsProvider";
import {
  DEFAULT_CONVERSATION_TRANSLATION_PROMPT_TEMPLATE,
  DEFAULT_CONVERSATION_TRANSLATION_TARGET_LANGUAGE,
} from "../../store/settings/AppSettingsProvider";

export type TranslationAvailabilityStatus = "idle" | "checking" | "available" | "unavailable";

export interface ConversationTranslationTaskController {
  tasks: AiExecutionTaskSnapshot[];
  startTranslation: (
    request: ConversationCardTranslationRequest,
  ) => Promise<AiExecutionTaskSnapshot>;
  cancelTask: (taskId: string) => Promise<AiExecutionTaskSnapshot>;
}

export interface ConversationContentController {
  cancelTranslation(blockId: string): Promise<void>;
  copyBlock(block: ConversationContentBlock): Promise<void>;
  expandedBlockIds: ReadonlySet<string>;
  getTranslatedText(block: ConversationContentBlock): string | undefined;
  getTranslationPhase(blockId: string): AiExecutionPhase | undefined;
  isCopied(blockId: string): boolean;
  isTranslating(blockId: string): boolean;
  toggleExpanded(blockId: string): void;
  translateBlock(block: ConversationContentBlock): Promise<void>;
  translationAvailability: TranslationAvailabilityStatus;
}

export interface UseConversationContentControllerOptions {
  blocks: readonly ConversationContentBlock[];
  enabled?: boolean;
  onCopyError?: (message: string) => void;
  onTranslationError?: (message: string) => void;
  recordKind: ConversationRecordKind;
  scopeKey?: string;
  t: Translator;
  translationAvailabilityChecker?: () => Promise<OpencodeTranslationAvailability>;
  translationSaver?: (
    request: ConversationPartTranslationUpdateRequest,
  ) => Promise<void>;
  translationSettings: ResolvedConversationTranslationSettings;
  translationTaskController?: ConversationTranslationTaskController;
  translator?: (
    request: ConversationCardTranslationRequest,
  ) => Promise<OpencodeTranslationResult>;
}

export const DEFAULT_TRANSLATION_SETTINGS: ResolvedConversationTranslationSettings = {
  agentId: "opencode",
  model: "",
  promptTemplate: DEFAULT_CONVERSATION_TRANSLATION_PROMPT_TEMPLATE,
  provider: "cli",
  targetLanguage: DEFAULT_CONVERSATION_TRANSLATION_TARGET_LANGUAGE,
};

export function useConversationContentController({
  blocks,
  enabled = true,
  onCopyError,
  onTranslationError,
  recordKind,
  scopeKey,
  t,
  translationAvailabilityChecker,
  translationSaver = updateConversationPartTranslation,
  translationSettings = DEFAULT_TRANSLATION_SETTINGS,
  translationTaskController,
  translator,
}: UseConversationContentControllerOptions): ConversationContentController {
  const globalTranslationTasks = useOptionalAiExecutionTasks();
  const taskController = translationTaskController ?? globalTranslationTasks;
  const [expandedBlockIds, setExpandedBlockIds] = useState<Set<string>>(new Set());
  const [copiedBlockId, setCopiedBlockId] = useState<string | null>(null);
  const [translatedBlocks, setTranslatedBlocks] = useState<Record<string, string>>({});
  const [translatingBlockIds, setTranslatingBlockIds] = useState<Set<string>>(new Set());
  const [translationTaskByBlockId, setTranslationTaskByBlockId] = useState<Record<string, string>>({});
  const [translationAvailability, setTranslationAvailability] = useState<TranslationAvailabilityStatus>("idle");
  const copiedResetTimerRef = useRef<number | null>(null);
  const handledTerminalTaskIdsRef = useRef(new Set<string>());
  const mountedRef = useRef(true);
  const previousScopeKeyRef = useRef(scopeKey);
  const blockById = useMemo(() => new Map(blocks.map((block) => [block.id, block])), [blocks]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      clearCopiedResetTimer(copiedResetTimerRef);
    };
  }, []);

  useEffect(() => {
    if (previousScopeKeyRef.current === scopeKey) return;
    previousScopeKeyRef.current = scopeKey;
    setExpandedBlockIds(new Set());
    setCopiedBlockId(null);
    setTranslatedBlocks({});
    setTranslatingBlockIds(new Set());
    setTranslationTaskByBlockId({});
    setTranslationAvailability("idle");
    handledTerminalTaskIdsRef.current.clear();
  }, [scopeKey]);

  useEffect(() => {
    if (!enabled || blocks.length === 0) return;
    let cancelled = false;
    setTranslationAvailability("checking");
    const checkAvailability = translationAvailabilityChecker ?? (() =>
      checkConversationTranslationAvailability({
        agentId: translationSettings.agentId,
        model: translationSettings.model,
        provider: translationSettings.provider,
      }));
    checkAvailability()
      .then((availability) => {
        if (!cancelled) setTranslationAvailability(availability.available ? "available" : "unavailable");
      })
      .catch(() => {
        if (!cancelled) setTranslationAvailability("unavailable");
      });
    return () => {
      cancelled = true;
    };
  }, [
    blocks.length,
    enabled,
    scopeKey,
    translationAvailabilityChecker,
    translationSettings.agentId,
    translationSettings.model,
    translationSettings.provider,
  ]);

  useEffect(() => {
    if (!enabled || !taskController) return;
    for (const [blockId, taskId] of Object.entries(translationTaskByBlockId)) {
      const task = taskController.tasks.find((candidate) => candidate.id === taskId);
      if (!task || !isTerminalAiExecutionTask(task)) continue;
      if (handledTerminalTaskIdsRef.current.has(task.id)) continue;
      handledTerminalTaskIdsRef.current.add(task.id);

      setTranslationTaskByBlockId((current) => {
        if (current[blockId] !== taskId) return current;
        const next = { ...current };
        delete next[blockId];
        return next;
      });
      setTranslatingBlockIds((current) => {
        const next = new Set(current);
        next.delete(blockId);
        return next;
      });

      if (task.state === "cancelled") {
        continue;
      }
      if (task.state === "failed" || !task.result?.text) {
        const message = task.state === "failed"
          ? task.error?.message ?? t("conversation.content.translationUnknownError")
          : t("conversation.content.translationUnknownError");
        onTranslationError?.(t("conversation.content.translationFailed", { message }));
        continue;
      }

      const translatedText = task.result.text;
      setTranslatedBlocks((current) => ({ ...current, [blockId]: translatedText }));
      const block = blockById.get(blockId);
      if (!block?.partId) continue;
      void translationSaver({ partId: block.partId, recordKind, translatedText }).catch((error) => {
        if (!mountedRef.current) return;
        const message = errorMessage(error);
        onTranslationError?.(t("conversation.content.translationSaveFailed", { message }));
      });
    }
  }, [
    blockById,
    enabled,
    onTranslationError,
    recordKind,
    t,
    taskController,
    translationSaver,
    translationTaskByBlockId,
  ]);

  const copyBlock = useCallback(async (block: ConversationContentBlock) => {
    try {
      await writeClipboardText(block.text);
      clearCopiedResetTimer(copiedResetTimerRef);
      setCopiedBlockId(block.id);
      copiedResetTimerRef.current = window.setTimeout(() => {
        setCopiedBlockId((current) => (current === block.id ? null : current));
        copiedResetTimerRef.current = null;
      }, 1400);
    } catch (error) {
      onCopyError?.(t("conversation.content.copyFailed", { message: errorMessage(error) }));
    }
  }, [onCopyError, t]);

  const translateBlock = useCallback(async (block: ConversationContentBlock) => {
    if (!enabled || translationAvailability !== "available") return;
    setTranslatingBlockIds((current) => new Set(current).add(block.id));
    const request: ConversationCardTranslationRequest = {
      agentId: translationSettings.agentId,
      model: translationSettings.model,
      promptTemplate: translationSettings.promptTemplate,
      provider: translationSettings.provider,
      targetLanguage: translationSettings.targetLanguage,
      text: block.text,
    };
    try {
      if (taskController) {
        const snapshot = await taskController.startTranslation(request);
        handledTerminalTaskIdsRef.current.delete(snapshot.id);
        setTranslationTaskByBlockId((current) => ({ ...current, [block.id]: snapshot.id }));
        return;
      }
      if (!translator) throw new Error("AI execution task provider is unavailable");
      const result = await translator(request);
      setTranslatedBlocks((current) => ({ ...current, [block.id]: result.translated_text }));
      if (block.partId) {
        try {
          await translationSaver({ partId: block.partId, recordKind, translatedText: result.translated_text });
        } catch (error) {
          const message = errorMessage(error);
          onTranslationError?.(t("conversation.content.translationSaveFailed", { message }));
        }
      }
    } catch (error) {
      const message = errorMessage(error);
      onTranslationError?.(t("conversation.content.translationFailed", { message }));
    } finally {
      if (!taskController) {
        setTranslatingBlockIds((current) => {
          const next = new Set(current);
          next.delete(block.id);
          return next;
        });
      }
    }
  }, [
    enabled,
    onTranslationError,
    recordKind,
    t,
    taskController,
    translationAvailability,
    translationSaver,
    translationSettings.agentId,
    translationSettings.model,
    translationSettings.promptTemplate,
    translationSettings.provider,
    translationSettings.targetLanguage,
    translator,
  ]);

  const cancelTranslation = useCallback(async (blockId: string) => {
    const taskId = translationTaskByBlockId[blockId];
    if (!taskController || !taskId) return;
    try {
      await taskController.cancelTask(taskId);
    } catch (error) {
      const message = errorMessage(error);
      onTranslationError?.(t("conversation.content.translationFailed", { message }));
    }
  }, [onTranslationError, t, taskController, translationTaskByBlockId]);

  const toggleExpanded = useCallback((blockId: string) => {
    setExpandedBlockIds((current) => {
      const next = new Set(current);
      if (next.has(blockId)) next.delete(blockId);
      else next.add(blockId);
      return next;
    });
  }, []);

  return useMemo(() => ({
    cancelTranslation,
    copyBlock,
    expandedBlockIds,
    getTranslatedText: (block: ConversationContentBlock) => translatedBlocks[block.id] ?? block.translatedText ?? undefined,
    getTranslationPhase: (blockId: string) => {
      const taskId = translationTaskByBlockId[blockId];
      const task = taskId ? taskController?.tasks.find((candidate) => candidate.id === taskId) : undefined;
      return task && !isTerminalAiExecutionTask(task) ? task.phase : undefined;
    },
    isCopied: (blockId: string) => copiedBlockId === blockId,
    isTranslating: (blockId: string) => {
      const taskId = translationTaskByBlockId[blockId];
      const task = taskId ? taskController?.tasks.find((candidate) => candidate.id === taskId) : undefined;
      return translatingBlockIds.has(blockId) || Boolean(task && !isTerminalAiExecutionTask(task));
    },
    toggleExpanded,
    translateBlock,
    translationAvailability,
  }), [
    cancelTranslation,
    copiedBlockId,
    copyBlock,
    expandedBlockIds,
    taskController,
    translatedBlocks,
    translatingBlockIds,
    translationAvailability,
    translationTaskByBlockId,
    toggleExpanded,
    translateBlock,
  ]);
}

function isTerminalAiExecutionTask(task: AiExecutionTaskSnapshot) {
  return task.state === "succeeded" || task.state === "failed" || task.state === "cancelled";
}

function clearCopiedResetTimer(timerRef: { current: number | null }) {
  if (timerRef.current === null) return;
  window.clearTimeout(timerRef.current);
  timerRef.current = null;
}

async function writeClipboardText(value: string) {
  if (typeof navigator === "undefined" || !navigator.clipboard?.writeText) {
    throw new Error("Clipboard API is unavailable");
  }
  await navigator.clipboard.writeText(value);
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
