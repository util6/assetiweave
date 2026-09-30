import { CornerDownLeft, Loader2, Search, Sparkles, X } from "lucide-react";
import {
  type ChangeEvent,
  type ClipboardEvent,
  type CompositionEvent,
  type KeyboardEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useTransition,
} from "react";
import clsx from "clsx";

import type { Translator } from "../../i18n/I18nProvider";
import type { ConversationSearchHit } from "../../types";
import type { ConversationSearchResultState } from "../../hooks/conversations/useConversationsController";
import type { ConversationContentCardColorSettings } from "../../store/settings/settingsSchema";
import { Button } from "../ui/button";
import { DialogFrame } from "../foundation/DialogFrame";
import {
  type ConversationSearchAppChipMeta,
  filterVisibleConversationSearchHits,
  getHitKey,
} from "./conversationSearchHelpers";
import {
  type ConversationSearchFilterValues,
  useConversationSearchDraftFilters,
} from "./useConversationSearchDraftFilters";
import { ConversationContentSearchResults } from "./ConversationContentSearchResults";

export * from "./conversationSearchHelpers";
export * from "./useConversationSearchDraftFilters";
export { ConversationSearchHitPreview } from "./ConversationSearchHitPreview";
export { ConversationContentSearchResults } from "./ConversationContentSearchResults";

export interface ConversationSearchDialogProps {
  adapterId?: string | null;
  appMetaById?: ReadonlyMap<string, ConversationSearchAppChipMeta>;
  commitDelayMs?: number;
  commitImmediatelyWhen?: (value: string) => boolean;
  contentCardColors: ConversationContentCardColorSettings;
  includeQuestions: boolean;
  loading: boolean;
  loadingMore?: boolean;
  onAdapterChange?: (adapterId: string | null) => void;
  onApplyFilters?: (filters: ConversationSearchFilterValues) => void;
  onCardKindToggle?: (kind: string) => void;
  onClose: () => void;
  onLoadMore?: () => void;
  onOpenHit: (hit: ConversationSearchHit) => void;
  onQueryChange: (query: string) => void;
  onQuestionToggle?: () => void;
  onSemanticRoleToggle?: (role: string) => void;
  onShowAllCardTypes?: () => void;
  open: boolean;
  query: string;
  result: ConversationSearchResultState | null;
  selectedCardKinds: string[];
  selectedSemanticRoles: string[];
  t: Translator;
}

export function ConversationSearchDialog({
  adapterId, appMetaById, commitDelayMs = 700, commitImmediatelyWhen,
  contentCardColors, includeQuestions, loading, loadingMore = false,
  onAdapterChange, onApplyFilters, onCardKindToggle, onClose, onLoadMore,
  onOpenHit, onQueryChange, onQuestionToggle, onSemanticRoleToggle, onShowAllCardTypes,
  open, query, result, selectedCardKinds, selectedSemanticRoles, t,
}: ConversationSearchDialogProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [draftQuery, setDraftQuery] = useState(query);
  const draftRef = useRef(query);
  const committedQueryRef = useRef(query);
  const composingRef = useRef(false);
  const isPasteRef = useRef(false);
  const timerRef = useRef<number | null>(null);
  const [, setPending] = useState(false);
  const [, startTransition] = useTransition();

  const {
    applyDraftFilters, draftAdapterId, draftCardKinds, draftIncludeQuestions,
    draftSemanticRoles, handleDiscardDraft, handleDraftAdapterChange,
    handleDraftCardKindToggle, handleDraftQuestionToggle, handleDraftReset,
    handleDraftSemanticRoleToggle, hasPendingFilterChanges, pendingChangeCount,
  } = useConversationSearchDraftFilters({
    committedAdapterId: adapterId,
    committedCardKinds: selectedCardKinds,
    committedIncludeQuestions: includeQuestions,
    committedSemanticRoles: selectedSemanticRoles,
    onAdapterChange, onApplyFilters, onCardKindToggle, onQuestionToggle,
    onResetFilters: onShowAllCardTypes, onSemanticRoleToggle, open,
  });

  const [activeHitKey, setActiveHitKey] = useState<string | null>(null);
  const itemRefs = useRef<Map<string, HTMLElement>>(new Map());

  const clearTimer = () => {
    if (timerRef.current !== null) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  };

  useEffect(() => () => clearTimer(), []);

  useEffect(() => {
    if (open) {
      clearTimer();
      setPending(false);
      composingRef.current = false;
      isPasteRef.current = false;
      committedQueryRef.current = query;
      draftRef.current = query;
      setDraftQuery(query);
      const timer = window.setTimeout(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
      }, 50);
      return () => window.clearTimeout(timer);
    }
  }, [open, query]);

  const hits = result?.hits ?? [];
  const trimmedQuery = draftQuery.trim();
  const displayedTotalCount = result?.totalCount ?? hits.length;
  const isShortId = /^[0-9a-f]{8}$/i.test(trimmedQuery);
  const hasUncommittedDraft =
    trimmedQuery !== committedQueryRef.current.trim() && trimmedQuery.length > 0;
  const hasUncommittedChanges = hasUncommittedDraft || hasPendingFilterChanges;

  const visibleHits = useMemo(
    () =>
      filterVisibleConversationSearchHits(hits, {
        adapterId: draftAdapterId,
        includeQuestions: draftIncludeQuestions,
        selectedCardKinds: draftCardKinds,
        selectedSemanticRoles: draftSemanticRoles,
      }),
    [hits, draftAdapterId, draftIncludeQuestions, draftCardKinds, draftSemanticRoles],
  );

  useEffect(() => {
    if (visibleHits.length > 0) {
      const exists = visibleHits.some((h) => getHitKey(h) === activeHitKey);
      if (!exists) {
        setActiveHitKey(getHitKey(visibleHits[0]));
      }
    } else {
      setActiveHitKey(null);
    }
  }, [visibleHits, activeHitKey]);

  const currentHitIndex = visibleHits.findIndex((h) => getHitKey(h) === activeHitKey);
  const activeHit =
    (currentHitIndex >= 0 ? visibleHits[currentHitIndex] : null) ?? visibleHits[0] ?? null;

  const commitDraft = (nextValue: string) => {
    clearTimer();
    setPending(false);
    if (nextValue === committedQueryRef.current) return;
    committedQueryRef.current = nextValue;
    onQueryChange(nextValue);
  };

  const scheduleCommit = (nextValue: string) => {
    clearTimer();
    setPending(nextValue !== committedQueryRef.current);
    timerRef.current = window.setTimeout(() => {
      timerRef.current = null;
      if (!composingRef.current) {
        commitDraft(nextValue);
      }
    }, commitDelayMs);
  };

  const handlePaste = () => {
    isPasteRef.current = true;
  };

  const handleInputChange = (event: ChangeEvent<HTMLInputElement>) => {
    const value = event.target.value;
    const prevLength = draftRef.current.length;
    draftRef.current = value;
    setDraftQuery(value);
    if (composingRef.current || inputEventIsComposing(event)) {
      clearTimer();
      return;
    }
    const isBulkOrPaste = isPasteRef.current || Math.abs(value.length - prevLength) !== 1;
    isPasteRef.current = false;
    if (isBulkOrPaste && commitImmediatelyWhen?.(value)) {
      clearTimer();
      commitDraft(value);
      return;
    }
    scheduleCommit(value);
  };

  const handleCompositionStart = () => {
    composingRef.current = true;
    clearTimer();
    setPending(false);
  };

  const handleCompositionEnd = (event: CompositionEvent<HTMLInputElement>) => {
    composingRef.current = false;
    const value = event.currentTarget.value;
    draftRef.current = value;
    setDraftQuery(value);
    scheduleCommit(value);
  };

  const handleClear = () => {
    clearTimer();
    draftRef.current = "";
    setDraftQuery("");
    commitDraft("");
    inputRef.current?.focus();
  };

  const handleHitSelect = useCallback(
    (hit: ConversationSearchHit) => {
      onOpenHit(hit);
      onClose();
    },
    [onOpenHit, onClose],
  );

  const handleSelectHit = useCallback((hit: ConversationSearchHit) => {
    setActiveHitKey(getHitKey(hit));
  }, []);

  const scrollToHit = (key: string) => {
    const el = itemRefs.current.get(key);
    if (typeof el?.scrollIntoView === "function") {
      el.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
  };

  const handleNavigateHit = (direction: "prev" | "next") => {
    if (visibleHits.length === 0) return;
    const effectiveIndex = currentHitIndex >= 0 ? currentHitIndex : 0;
    const nextIdx =
      direction === "next"
        ? Math.min(effectiveIndex + 1, visibleHits.length - 1)
        : Math.max(effectiveIndex - 1, 0);
    const targetHit = visibleHits[nextIdx];
    const targetKey = getHitKey(targetHit);
    setActiveHitKey(targetKey);
    scrollToHit(targetKey);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (
      composingRef.current ||
      (event.nativeEvent as globalThis.KeyboardEvent).isComposing ||
      event.keyCode === 229
    ) {
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      handleNavigateHit("next");
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      handleNavigateHit("prev");
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      clearTimer();
      const hasQueryChange = draftRef.current.trim() !== committedQueryRef.current.trim();
      if (hasQueryChange || hasPendingFilterChanges) {
        if (hasPendingFilterChanges) applyDraftFilters();
        if (hasQueryChange) commitDraft(draftRef.current);
      } else if (activeHit) {
        handleHitSelect(activeHit);
      }
      return;
    }
    if (event.key === "Escape") {
      if (draftRef.current.trim().length > 0) {
        event.preventDefault();
        event.stopPropagation();
        handleClear();
      } else if (hasPendingFilterChanges) {
        event.preventDefault();
        event.stopPropagation();
        handleDiscardDraft();
      }
    }
  };

  const handleSubmit = () => {
    if (composingRef.current) return;
    clearTimer();
    if (hasPendingFilterChanges) applyDraftFilters();
    commitDraft(draftRef.current);
  };

  if (!open) return null;

  return (
    <DialogFrame
      className="flex h-[88vh] max-h-[900px] w-[96vw] max-w-[1180px] flex-col overflow-hidden shadow-2xl"
      closeLabel={t("conversation.search.dialogClose")}
      contentClassName="flex min-h-0 flex-1 flex-col overflow-hidden p-0"
      description={t("conversation.search.dialogDescription")}
      footer={
        <div className="flex w-full flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-2 text-body-sm text-on-surface-variant">
            {loading ? (
              <span className="flex items-center gap-2 text-status-update font-medium">
                <Loader2 className="size-4 animate-spin" />
                {t("conversation.search.loading")}
              </span>
            ) : result && trimmedQuery ? (
              <span className="font-medium">
                {t("conversation.search.resultsCount", {
                  count: displayedTotalCount,
                  query: trimmedQuery,
                })}
              </span>
            ) : (
              <span className="flex items-center gap-1.5 text-on-surface-muted">
                <Sparkles className="size-3.5 text-primary/70" />
                {t("conversation.search.initialHint")}
              </span>
            )}
          </div>

          <div className="flex items-center gap-4">
            <div className="hidden items-center gap-2.5 font-mono text-caption text-on-surface-muted sm:flex">
              <span className="inline-flex items-center gap-1">
                <kbd className="rounded-lg border border-theme-control-border bg-theme-control/80 px-1.5 py-0.5 text-[11px] shadow-sm">↑↓</kbd>
                {t("conversation.search.shortcutNav")}
              </span>
              <span className="inline-flex items-center gap-1">
                <kbd className="rounded-lg border border-theme-control-border bg-theme-control/80 px-1.5 py-0.5 text-[11px] shadow-sm">↵</kbd>
                {t("conversation.search.shortcutOpen")}
              </span>
              <span className="inline-flex items-center gap-1">
                <kbd className="rounded-lg border border-theme-control-border bg-theme-control/80 px-1.5 py-0.5 text-[11px] shadow-sm">Esc</kbd>
                {t("conversation.search.dialogClose")}
              </span>
            </div>

            <div className="flex shrink-0 items-center gap-2">
              <Button
                disabled={!activeHit}
                onClick={() => activeHit && handleHitSelect(activeHit)}
                size="default"
                type="button"
                variant="default"
              >
                <CornerDownLeft className="mr-1.5 size-4" />
                {t("conversation.search.openSessionAndJump")}
              </Button>
              <Button onClick={onClose} size="default" type="button" variant="outline">
                {t("conversation.search.dialogClose")}
              </Button>
            </div>
          </div>
        </div>
      }
      icon={<Search size={19} />}
      onClose={onClose}
      size="xl"
      title={t("conversation.search.dialogTitle")}
    >
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        {/* Spotlight Search Bar */}
        <div className="shrink-0 border-b border-theme-card-border/60 bg-theme-toolbar/40 px-5 py-3.5 backdrop-blur-md">
          <div className="relative flex h-12 w-full items-center gap-3 rounded-2xl border border-theme-control-border/80 bg-theme-control/90 px-3.5 text-on-surface shadow-[var(--theme-shadow-control-inset)] transition-[border-color,box-shadow] focus-within:border-primary/60 focus-within:shadow-[0_0_28px_rgb(var(--theme-glow)/0.2)]">
            {loading ? (
              <Loader2 className="size-5 shrink-0 animate-spin text-status-update" />
            ) : (
              <Search className="size-5 shrink-0 text-primary/80" />
            )}
            <input
              aria-label={t("conversation.search.contentPlaceholder")}
              autoFocus
              className="min-w-0 flex-1 border-0 bg-transparent text-body-base font-medium text-on-surface outline-none placeholder:text-outline"
              onChange={handleInputChange}
              onCompositionEnd={handleCompositionEnd}
              onCompositionStart={handleCompositionStart}
              onKeyDown={handleKeyDown}
              onPaste={handlePaste}
              placeholder={t("conversation.search.contentPlaceholder")}
              ref={inputRef}
              type="search"
              value={draftQuery}
            />

            {/* Short ID badge */}
            {isShortId ? (
              <span className="hidden rounded-full border border-primary/40 bg-primary/10 px-2 py-0.5 text-caption font-mono font-medium text-primary sm:inline-block">
                #短 ID
              </span>
            ) : null}

            {/* Total count pill */}
            {result && trimmedQuery && !loading ? (
              <span className="hidden rounded-full bg-primary/15 px-2.5 py-0.5 text-caption font-semibold text-primary sm:inline-block">
                {displayedTotalCount} 个结果
              </span>
            ) : null}

            {/* Clear button */}
            {draftQuery ? (
              <button
                aria-label={t("conversation.search.clear")}
                className="flex size-7 items-center justify-center rounded-xl text-outline transition-colors hover:bg-theme-control-hover hover:text-on-surface"
                onClick={handleClear}
                title={t("conversation.search.clear")}
                type="button"
              >
                <X size={15} />
              </button>
            ) : null}

            {/* Submit button with visual cue */}
            <button
              aria-label={t("conversation.search.submit")}
              className={clsx(
                "flex items-center justify-center rounded-xl transition-all",
                hasUncommittedChanges
                  ? "h-7 gap-1 border border-primary/45 bg-primary/15 px-2 text-caption font-semibold text-primary hover:bg-primary/25"
                  : "size-7 text-outline hover:bg-theme-control-hover hover:text-on-surface",
              )}
              onClick={handleSubmit}
              title={t("conversation.search.submit")}
              type="button"
            >
              <Search size={15} />
              {hasUncommittedChanges ? (
                <kbd className="hidden text-[10px] font-mono leading-none sm:inline">↵</kbd>
              ) : null}
            </button>
          </div>
        </div>

        {/* Hits list with filter & pagination */}
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
          <ConversationContentSearchResults
            activeHitKey={activeHitKey}
            adapterId={draftAdapterId}
            appMetaById={appMetaById}
            contentCardColors={contentCardColors}
            hasPendingFilterChanges={hasPendingFilterChanges}
            includeQuestions={draftIncludeQuestions}
            itemRefs={itemRefs}
            loading={loading}
            loadingMore={loadingMore}
            onAdapterChange={handleDraftAdapterChange}
            onApplyFilters={applyDraftFilters}
            onCardKindToggle={handleDraftCardKindToggle}
            onLoadMore={onLoadMore}
            onOpenHit={handleHitSelect}
            onQuestionToggle={handleDraftQuestionToggle}
            onSelectHit={handleSelectHit}
            onSemanticRoleToggle={handleDraftSemanticRoleToggle}
            onShowAllCardTypes={handleDraftReset}
            pendingChangeCount={pendingChangeCount}
            result={result}
            selectedCardKinds={draftCardKinds}
            selectedSemanticRoles={draftSemanticRoles}
            t={t}
          />
        </div>
      </div>
    </DialogFrame>
  );
}

function inputEventIsComposing(event: ChangeEvent<HTMLInputElement>) {
  return Boolean((event.nativeEvent as InputEvent)?.isComposing);
}

export { ConversationSearchTrigger } from "./conversationSearchHelpers";

