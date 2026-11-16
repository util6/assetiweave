import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type {
  ConversationRecordKind,
  ConversationSearchHit,
  ConversationSearchResult,
} from "../../types";
import type { ConversationSearchResultState } from "../../hooks/conversations/useConversationsController";
import type { TaskView } from "../../types/taskCenter";
import {
  getConversationContentSearchTaskResult,
  searchConversationRecords,
  startConversationContentSearchTask,
} from "../../services/conversations";
import { getHitKey } from "../../components/conversations/conversationSearchHelpers";
import { isTauriRuntime } from "../../services/taskCenterService";
import { useAppUiStore } from "../../store/ui/appUiStore";
import { useOptionalTaskCenter } from "./TaskCenterProvider";

export interface ConversationSearchSubmission {
  query: string;
  recordKind: ConversationRecordKind;
  adapterId: string | null;
  cardKinds: string[];
  semanticRoles: string[];
  includeQuestions: boolean;
}

export interface ActiveConversationSearchTask {
  id: string;
  query: string;
  recordKind: ConversationRecordKind;
  task: TaskView | null;
}

export interface PendingConversationSearchConflict {
  activeTaskId: string;
  activeQuery: string;
  nextQuery: string;
  submission: ConversationSearchSubmission;
}

export interface ConversationSearchContextValue {
  dialogOpen: boolean;
  setDialogOpen: (open: boolean) => void;
  committedQuery: string;
  committedRecordKind: ConversationRecordKind;
  committedAdapterId: string | null;
  committedCardKinds: string[];
  committedSemanticRoles: string[];
  committedIncludesQuestions: boolean;
  result: ConversationSearchResultState | null;
  loading: boolean;
  loadingMore: boolean;
  activeTask: ActiveConversationSearchTask | null;
  pendingConflict: PendingConversationSearchConflict | null;
  lastError: string | null;
  clearLastError: () => void;
  submitSearch: (submission: ConversationSearchSubmission) => Promise<void>;
  confirmConflictReplace: () => Promise<void>;
  dismissConflictKeepWaiting: () => void;
  cancelActiveSearch: () => Promise<void>;
  loadMore: () => Promise<void>;
  openResultForTask: (taskId: string) => Promise<void>;
  getTaskRecordKind: (taskId: string) => ConversationRecordKind;
}

const ConversationSearchContext =
  createContext<ConversationSearchContextValue | null>(null);

function isActiveTaskState(state: string) {
  return state === "pending" || state === "running" || state === "cancelling";
}

function conversationSearchCardKinds(result: ConversationSearchResult): string[] {
  const counts = result.content_type_counts ?? {};
  const keys = Object.keys(counts).filter((key) => key !== "question");
  if (keys.length > 0) return keys;
  return Array.from(
    new Set(
      result.hits
        .map((hit) => hit.card_type)
        .filter((kind) => kind && kind !== "question"),
    ),
  );
}

function toResultState(
  raw: ConversationSearchResult,
  fallbackIncludeQuestions: boolean,
): ConversationSearchResultState {
  return {
    cardKinds: conversationSearchCardKinds(raw),
    semanticRoles: Object.keys(raw.semantic_role_counts ?? {}),
    includeQuestions: raw.scope?.include_questions ?? fallbackIncludeQuestions,
    hits: raw.hits,
    recordKind: raw.record_kind,
    query: raw.query,
    totalCount: raw.total_count,
  };
}

export function ConversationSearchProvider({
  children,
}: {
  children: ReactNode;
}) {
  const taskCenter = useOptionalTaskCenter();
  const dialogOpen = useAppUiStore(
    (state) => state.conversationSearchDialogOpen,
  );
  const setDialogOpen = useAppUiStore(
    (state) => state.setConversationSearchDialogOpen,
  );

  const [committedQuery, setCommittedQuery] = useState("");
  const [committedRecordKind, setCommittedRecordKind] =
    useState<ConversationRecordKind>("session");
  const [committedAdapterId, setCommittedAdapterId] = useState<string | null>(
    null,
  );
  const [committedCardKinds, setCommittedCardKinds] = useState<string[]>([]);
  const [committedSemanticRoles, setCommittedSemanticRoles] = useState<
    string[]
  >([]);
  const [committedIncludesQuestions, setCommittedIncludesQuestions] =
    useState(true);
  const [result, setResult] = useState<ConversationSearchResultState | null>(
    null,
  );
  const [loadingFallback, setLoadingFallback] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [activeTaskId, setActiveTaskId] = useState<string | null>(null);
  const [activeTaskMeta, setActiveTaskMeta] = useState<{
    id: string;
    query: string;
    recordKind: ConversationRecordKind;
    task: TaskView | null;
  } | null>(null);
  const [pendingConflict, setPendingConflict] =
    useState<PendingConversationSearchConflict | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);

  const recordKindByTaskIdRef = useRef<Map<string, ConversationRecordKind>>(
    new Map(),
  );
  const submissionByTaskIdRef = useRef<Map<string, ConversationSearchSubmission>>(
    new Map(),
  );
  const cachedResultsByTaskIdRef = useRef<
    Map<string, ConversationSearchResultState>
  >(new Map());
  const fetchedTaskIdsRef = useRef<Set<string>>(new Set());

  const liveTask = useMemo(() => {
    if (!activeTaskId || !taskCenter) return null;
    return taskCenter.tasks.find((t) => t.id === activeTaskId) ?? null;
  }, [activeTaskId, taskCenter]);

  const activeTask = useMemo<ActiveConversationSearchTask | null>(() => {
    if (!activeTaskId || !activeTaskMeta || activeTaskMeta.id !== activeTaskId) {
      return null;
    }
    const currentTask = liveTask ?? activeTaskMeta.task;
    if (currentTask && !isActiveTaskState(currentTask.state)) {
      return null;
    }
    return {
      id: activeTaskId,
      query: activeTaskMeta.query,
      recordKind: activeTaskMeta.recordKind,
      task: currentTask,
    };
  }, [activeTaskId, activeTaskMeta, liveTask]);

  const loading = Boolean(activeTask) || loadingFallback;

  const applyCompletedResult = useCallback(
    (
      taskId: string,
      raw: ConversationSearchResult,
      submission?: ConversationSearchSubmission,
    ) => {
      const state = toResultState(
        raw,
        submission?.includeQuestions ?? committedIncludesQuestions,
      );
      cachedResultsByTaskIdRef.current.set(taskId, state);
      setResult(state);
      setCommittedQuery(raw.query);
      setCommittedRecordKind(raw.record_kind);
      if (submission) {
        setCommittedAdapterId(submission.adapterId);
        setCommittedCardKinds(submission.cardKinds);
        setCommittedSemanticRoles(submission.semanticRoles);
        setCommittedIncludesQuestions(submission.includeQuestions);
      }
    },
    [committedIncludesQuestions],
  );

  // Watch active task terminal transitions
  useEffect(() => {
    if (!activeTaskId || !liveTask) return;
    if (liveTask.state === "succeeded") {
      if (fetchedTaskIdsRef.current.has(activeTaskId)) return;
      fetchedTaskIdsRef.current.add(activeTaskId);
      const submission = submissionByTaskIdRef.current.get(activeTaskId);
      void getConversationContentSearchTaskResult(activeTaskId)
        .then((raw) => {
          if (raw) {
            applyCompletedResult(activeTaskId, raw, submission);
          }
        })
        .catch((error) => {
          setLastError(
            error instanceof Error ? error.message : String(error ?? ""),
          );
        })
        .finally(() => {
          setActiveTaskId((cur) => (cur === activeTaskId ? null : cur));
        });
    } else if (
      liveTask.state === "failed" ||
      liveTask.state === "canceled"
    ) {
      if (liveTask.state === "failed") {
        setLastError(
          liveTask.error_summary ||
            liveTask.errorSummary ||
            "对话内容搜索失败",
        );
      }
      setActiveTaskId((cur) => (cur === activeTaskId ? null : cur));
    }
  }, [activeTaskId, applyCompletedResult, liveTask]);

  const launchSearchTask = useCallback(
    async (submission: ConversationSearchSubmission) => {
      const trimmedQuery = submission.query.trim();
      setLastError(null);
      setPendingConflict(null);
      setCommittedQuery(trimmedQuery);
      setCommittedRecordKind(submission.recordKind);
      setCommittedAdapterId(submission.adapterId);
      setCommittedCardKinds(submission.cardKinds);
      setCommittedSemanticRoles(submission.semanticRoles);
      setCommittedIncludesQuestions(submission.includeQuestions);

      if (!trimmedQuery) {
        setResult(null);
        return;
      }

      const params = {
        ...(submission.adapterId ? { adapter_id: submission.adapterId } : {}),
        content_types: [] as string[],
        card_kinds: submission.cardKinds,
        semantic_roles: submission.semanticRoles,
        include_questions: submission.includeQuestions,
        include_cards: true,
        limit: 50,
        query: trimmedQuery,
        record_kind: submission.recordKind,
      };

      try {
        const task = await startConversationContentSearchTask(params);
        recordKindByTaskIdRef.current.set(task.id, submission.recordKind);
        submissionByTaskIdRef.current.set(task.id, submission);
        setActiveTaskMeta({
          id: task.id,
          query: trimmedQuery,
          recordKind: submission.recordKind,
          task,
        });
        setActiveTaskId(task.id);
        void taskCenter?.refresh();
      } catch (error) {
        if (!isTauriRuntime()) {
          // Pure browser preview fallback
          setLoadingFallback(true);
          try {
            const raw = await searchConversationRecords(params);
            setResult(toResultState(raw, submission.includeQuestions));
          } catch (fallbackError) {
            setLastError(
              fallbackError instanceof Error
                ? fallbackError.message
                : String(fallbackError ?? ""),
            );
          } finally {
            setLoadingFallback(false);
          }
          return;
        }
        setLastError(
          error instanceof Error ? error.message : String(error ?? ""),
        );
      }
    },
    [taskCenter],
  );

  const submitSearch = useCallback(
    async (submission: ConversationSearchSubmission) => {
      const trimmedQuery = submission.query.trim();
      if (!trimmedQuery) {
        if (!activeTask) {
          setCommittedQuery("");
          setResult(null);
        }
        return;
      }
      if (activeTask) {
        setPendingConflict({
          activeTaskId: activeTask.id,
          activeQuery: activeTask.query,
          nextQuery: trimmedQuery,
          submission: { ...submission, query: trimmedQuery },
        });
        return;
      }
      await launchSearchTask({ ...submission, query: trimmedQuery });
    },
    [activeTask, launchSearchTask],
  );

  const confirmConflictReplace = useCallback(async () => {
    if (!pendingConflict) return;
    const { activeTaskId: oldId, submission } = pendingConflict;
    setPendingConflict(null);
    taskCenter?.suppressTaskNotification(oldId);
    try {
      await taskCenter?.cancelTask(oldId);
    } catch {
      // Ignore if old task already finished
    }
    setActiveTaskId((cur) => (cur === oldId ? null : cur));
    await launchSearchTask(submission);
  }, [launchSearchTask, pendingConflict, taskCenter]);

  const dismissConflictKeepWaiting = useCallback(() => {
    setPendingConflict(null);
  }, []);

  const cancelActiveSearch = useCallback(async () => {
    if (!activeTask) return;
    const taskId = activeTask.id;
    setPendingConflict(null);
    taskCenter?.suppressTaskNotification(taskId);
    try {
      await taskCenter?.cancelTask(taskId);
    } finally {
      setActiveTaskId((cur) => (cur === taskId ? null : cur));
    }
  }, [activeTask, taskCenter]);

  const loadMore = useCallback(async () => {
    if (
      !result ||
      loading ||
      loadingMore ||
      result.hits.length >= result.totalCount
    ) {
      return;
    }
    const trimmedQuery = result.query.trim();
    if (!trimmedQuery) return;
    setLoadingMore(true);
    try {
      const nextPage = await searchConversationRecords({
        ...(committedAdapterId ? { adapter_id: committedAdapterId } : {}),
        content_types: [],
        card_kinds: committedCardKinds,
        semantic_roles: committedSemanticRoles,
        include_questions: committedIncludesQuestions,
        include_cards: true,
        limit: 50,
        offset: result.hits.length,
        query: trimmedQuery,
        record_kind: result.recordKind,
      });
      setResult((current) => {
        if (!current) return toResultState(nextPage, committedIncludesQuestions);
        const seen = new Set(current.hits.map(getHitKey));
        const appended: ConversationSearchHit[] = [];
        for (const hit of nextPage.hits) {
          const key = getHitKey(hit);
          if (!seen.has(key)) {
            seen.add(key);
            appended.push(hit);
          }
        }
        return {
          ...current,
          hits: [...current.hits, ...appended],
          totalCount: nextPage.total_count,
        };
      });
    } catch (error) {
      setLastError(
        error instanceof Error ? error.message : String(error ?? ""),
      );
    } finally {
      setLoadingMore(false);
    }
  }, [
    committedAdapterId,
    committedCardKinds,
    committedIncludesQuestions,
    committedSemanticRoles,
    loading,
    loadingMore,
    result,
  ]);

  const openResultForTask = useCallback(
    async (taskId: string) => {
      const cached = cachedResultsByTaskIdRef.current.get(taskId);
      const submission = submissionByTaskIdRef.current.get(taskId);
      if (cached) {
        setResult(cached);
        setCommittedQuery(cached.query);
        setCommittedRecordKind(cached.recordKind);
        setDialogOpen(true);
        return;
      }
      try {
        const raw = await getConversationContentSearchTaskResult(taskId);
        if (raw) {
          recordKindByTaskIdRef.current.set(taskId, raw.record_kind);
          applyCompletedResult(taskId, raw, submission);
        }
      } finally {
        setDialogOpen(true);
      }
    },
    [applyCompletedResult, setDialogOpen],
  );

  const getTaskRecordKind = useCallback(
    (taskId: string): ConversationRecordKind => {
      return (
        recordKindByTaskIdRef.current.get(taskId) ??
        cachedResultsByTaskIdRef.current.get(taskId)?.recordKind ??
        result?.recordKind ??
        committedRecordKind
      );
    },
    [committedRecordKind, result?.recordKind],
  );

  const clearLastError = useCallback(() => setLastError(null), []);

  const value = useMemo<ConversationSearchContextValue>(
    () => ({
      dialogOpen,
      setDialogOpen,
      committedQuery,
      committedRecordKind,
      committedAdapterId,
      committedCardKinds,
      committedSemanticRoles,
      committedIncludesQuestions,
      result,
      loading,
      loadingMore,
      activeTask,
      pendingConflict,
      lastError,
      clearLastError,
      submitSearch,
      confirmConflictReplace,
      dismissConflictKeepWaiting,
      cancelActiveSearch,
      loadMore,
      openResultForTask,
      getTaskRecordKind,
    }),
    [
      dialogOpen,
      setDialogOpen,
      committedQuery,
      committedRecordKind,
      committedAdapterId,
      committedCardKinds,
      committedSemanticRoles,
      committedIncludesQuestions,
      result,
      loading,
      loadingMore,
      activeTask,
      pendingConflict,
      lastError,
      clearLastError,
      submitSearch,
      confirmConflictReplace,
      dismissConflictKeepWaiting,
      cancelActiveSearch,
      loadMore,
      openResultForTask,
      getTaskRecordKind,
    ],
  );

  return (
    <ConversationSearchContext.Provider value={value}>
      {children}
    </ConversationSearchContext.Provider>
  );
}

export function useConversationSearch(): ConversationSearchContextValue {
  const ctx = useContext(ConversationSearchContext);
  if (!ctx) {
    throw new Error(
      "useConversationSearch must be used within ConversationSearchProvider",
    );
  }
  return ctx;
}

export function useOptionalConversationSearch(): ConversationSearchContextValue | null {
  return useContext(ConversationSearchContext);
}
