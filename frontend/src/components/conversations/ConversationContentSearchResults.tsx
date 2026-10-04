import {
  Code2,
  FileCode,
  Loader2,
  MessageSquare,
  RotateCcw,
  SearchX,
  Sparkles,
  Terminal,
} from "lucide-react";
import { type RefObject, memo, useEffect, useMemo, useRef } from "react";

import type { Translator } from "../../i18n/I18nProvider";
import type { ConversationSearchHit } from "../../types";
import type { ConversationSearchResultState } from "../../hooks/conversations/useConversationsController";
import type { ConversationContentCardColorSettings } from "../../store/settings/settingsSchema";
import { RenderSafeScrollSurface } from "../common/rendering/RenderSafeScrollSurface";
import {
  type ConversationSearchAppChipMeta,
  countHitsByKind,
  filterVisibleConversationSearchHits,
  getHitKey,
} from "./conversationSearchHelpers";
import {
  conversationCardPresentationKind,
  isRedundantConversationCardKind,
  useConversationCardKindRegistry,
} from "./ConversationCardKindRegistry";
import { ConversationSearchFilterBar } from "./ConversationSearchFilterBar";
import { ConversationSearchCardItem } from "./ConversationSearchCardItem";

export interface ConversationContentSearchResultsProps {
  activeHitKey?: string | null;
  adapterId?: string | null;
  appMetaById?: ReadonlyMap<string, ConversationSearchAppChipMeta>;
  contentCardColors: ConversationContentCardColorSettings;
  hasPendingFilterChanges?: boolean;
  includeQuestions: boolean;
  itemRefs?: RefObject<Map<string, HTMLElement>>;
  loading: boolean;
  loadingMore?: boolean;
  onAdapterChange?: (adapterId: string | null) => void;
  onApplyFilters?: () => void;
  onCardKindToggle: (kind: string) => void;
  onLoadMore?: () => void;
  onOpenHit: (hit: ConversationSearchHit) => void;
  onQuestionToggle: () => void;
  onSelectHit?: (hit: ConversationSearchHit) => void;
  onSemanticRoleToggle: (role: string) => void;
  onShowAllCardTypes: () => void;
  pendingChangeCount?: number;
  result: ConversationSearchResultState | null;
  selectedCardKinds: string[];
  selectedSemanticRoles: string[];
  t: Translator;
}

export const ConversationContentSearchResults = memo(
  function ConversationContentSearchResults({
    activeHitKey,
    adapterId,
    appMetaById,
    contentCardColors,
    hasPendingFilterChanges = false,
    includeQuestions,
    itemRefs,
    loading,
    loadingMore = false,
    onAdapterChange,
    onApplyFilters,
    onCardKindToggle,
    onLoadMore,
    onOpenHit,
    onQuestionToggle,
    onSelectHit,
    onSemanticRoleToggle,
    onShowAllCardTypes,
    pendingChangeCount = 0,
    result,
    selectedCardKinds,
    selectedSemanticRoles,
    t,
  }: ConversationContentSearchResultsProps) {
    const { definitions } = useConversationCardKindRegistry();
    const hits: ConversationSearchHit[] = result?.hits ?? [];
    const loadMoreSentinelRef = useRef<HTMLDivElement | null>(null);

    // Available adapter IDs extracted from appMetaById and hits
    const availableAdapters = useMemo(() => {
      const ids = new Set<string>();
      if (appMetaById) {
        for (const id of appMetaById.keys()) {
          ids.add(id);
        }
      }
      for (const h of hits) {
        if (h.session.adapter_id) ids.add(h.session.adapter_id);
      }
      return [...ids].sort();
    }, [appMetaById, hits]);

    const availableCardKinds: string[] = (result?.cardKinds ?? []).filter(
      (kind: string) =>
        !isRedundantConversationCardKind(kind, definitions.get(kind)),
    );
    const availableSemanticRoles: string[] = [
      ...new Set([
        ...(result?.semanticRoles ?? []),
        ...(result?.cardKinds ?? []).flatMap((kind: string) => {
          const definition = definitions.get(kind);
          return isRedundantConversationCardKind(kind, definition) &&
            definition?.semantic_role
            ? [definition.semantic_role]
            : [];
        }),
      ]),
    ];

    const showProjectPath = result?.recordKind !== "web";
    const allCardTypesSelected =
      includeQuestions &&
      selectedCardKinds.length === 0 &&
      selectedSemanticRoles.length === 0 &&
      !adapterId;

    const visibleHits = filterVisibleConversationSearchHits(hits, {
      adapterId,
      definitions,
      includeQuestions,
      selectedCardKinds,
      selectedSemanticRoles,
    });

    const query = result?.query ?? "";
    const displayedTotalCount = result?.totalCount ?? visibleHits.length;
    const { questionCount, countByKind } = countHitsByKind(hits);
    const hasMore = (result?.totalCount ?? 0) > hits.length;

    // Auto load more when scrolling to bottom sentinel
    useEffect(() => {
      if (!hasMore || loadingMore || !onLoadMore) return;
      if (
        typeof window === "undefined" ||
        typeof window.IntersectionObserver === "undefined"
      ) {
        return;
      }
      const sentinel = loadMoreSentinelRef.current;
      if (!sentinel) return;

      const observer = new IntersectionObserver(
        (entries) => {
          if (entries[0]?.isIntersecting) {
            onLoadMore();
          }
        },
        { rootMargin: "200px" },
      );
      observer.observe(sentinel);
      return () => observer.disconnect();
    }, [hasMore, loadingMore, onLoadMore]);

    // Group visible hits by card type
    const groupedCardTypes: string[] = [
      ...new Set(
        visibleHits
          .filter((hit) => hit.card_type !== "question")
          .map((hit) =>
            conversationCardPresentationKind(
              hit.card_type,
              definitions.get(hit.card_type)?.semantic_role,
            ),
          ),
      ),
    ].sort((left, right) => left.localeCompare(right));

    const groupedHits = [
      ...(includeQuestions
        ? [
            {
              cardType: "question",
              hits: visibleHits.filter((hit) => hit.card_type === "question"),
            },
          ]
        : []),
      ...groupedCardTypes.map((cardType) => ({
        cardType,
        hits: visibleHits.filter(
          (hit) =>
            conversationCardPresentationKind(
              hit.card_type,
              definitions.get(hit.card_type)?.semantic_role,
            ) === cardType,
        ),
      })),
    ].filter((group) => group.hits.length > 0);

    return (
      <section
        aria-live="polite"
        className="conversation-search-results flex min-h-0 flex-1 flex-col overflow-hidden"
      >
        {/* Top Filter and Header Bar */}
        <ConversationSearchFilterBar
          adapterId={adapterId}
          allCardTypesSelected={allCardTypesSelected}
          appMetaById={appMetaById}
          availableAdapters={availableAdapters}
          availableCardKinds={availableCardKinds}
          availableSemanticRoles={availableSemanticRoles}
          contentCardColors={contentCardColors}
          countByKind={countByKind}
          displayedTotalCount={displayedTotalCount}
          hasPendingFilterChanges={hasPendingFilterChanges}
          includeQuestions={includeQuestions}
          loading={loading}
          onAdapterChange={onAdapterChange}
          onApplyFilters={onApplyFilters}
          onCardKindToggle={onCardKindToggle}
          onQuestionToggle={onQuestionToggle}
          onResetFilters={onShowAllCardTypes}
          onSemanticRoleToggle={onSemanticRoleToggle}
          pendingChangeCount={pendingChangeCount}
          query={query}
          questionCount={questionCount}
          resultHitsCount={hits.length}
          selectedCardKinds={selectedCardKinds}
          selectedSemanticRoles={selectedSemanticRoles}
          t={t}
        />

        {/* Progress indicator */}
        {loading ? (
          <div
            aria-label={t("conversation.search.loading")}
            className="h-1 overflow-hidden bg-theme-control"
            role="progressbar"
          >
            <div className="h-full w-full animate-pulse bg-status-update" />
          </div>
        ) : null}

        {/* Scrollable Result Cards Area */}
        <RenderSafeScrollSurface
          aria-label={t("conversation.search.resultsTitle")}
          className="min-h-0 flex-1 px-5 py-4"
          tabIndex={0}
        >
          {/* Welcome state when no search executed yet */}
          {!result && !loading ? (
            <div className="flex flex-col items-center justify-center py-12 text-center">
              <div className="grid size-14 place-items-center rounded-2xl border border-primary/25 bg-primary/10 text-primary shadow-sm">
                <Sparkles className="size-7" />
              </div>
              <p className="mt-4 text-body-base font-semibold text-on-surface">
                {t("conversation.search.dialogDescription")}
              </p>
              <p className="mt-1 text-code-xs text-on-surface-muted max-w-sm">
                {t("conversation.search.quickHintsDesc")}
              </p>

              <div className="mt-6 grid w-full max-w-xl grid-cols-2 gap-3 text-left">
                {[
                  {
                    desc: "寻找提问记录",
                    icon: MessageSquare,
                    label: "用户提问",
                    onClick: onQuestionToggle,
                  },
                  {
                    desc: "定位修改过的代码",
                    icon: Code2,
                    label: "代码片段",
                    onClick: () => onCardKindToggle("code"),
                  },
                  {
                    desc: "查看执行过的命令",
                    icon: Terminal,
                    label: "终端命令",
                    onClick: () => onCardKindToggle("command"),
                  },
                  {
                    desc: "查找架构与解答",
                    icon: FileCode,
                    label: "回答内容",
                    onClick: () => onCardKindToggle("answer"),
                  },
                ].map((cat) => {
                  const Icon = cat.icon;
                  return (
                    <button
                      className="flex flex-col gap-1 rounded-2xl border border-theme-card-border/60 bg-theme-card/40 p-3.5 transition-[transform,background-color,border-color] duration-150 hover:-translate-y-px hover:border-primary/40 hover:bg-theme-control-hover hover:text-on-surface"
                      key={cat.label}
                      onClick={cat.onClick}
                      type="button"
                    >
                      <div className="flex items-center gap-2 text-body-sm font-semibold text-on-surface">
                        <Icon className="size-4 text-primary" />
                        <span>{cat.label}</span>
                      </div>
                      <span className="text-code-xs text-on-surface-muted">
                        {cat.desc}
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>
          ) : visibleHits.length === 0 ? (
            /* Empty hits state */
            <div className="flex min-h-[300px] flex-col items-center justify-center gap-3 rounded-2xl border border-dashed border-theme-control-border/70 p-8 text-center">
              <SearchX className="size-10 text-outline/60" />
              <div>
                <p className="text-body-base font-semibold text-on-surface">
                  {loading
                    ? t("conversation.search.loading")
                    : t("conversation.search.empty")}
                </p>
                <p className="mt-1 text-code-xs text-on-surface-muted max-w-sm">
                  {t("conversation.search.dialogDescription")}
                </p>
              </div>
              {!allCardTypesSelected ? (
                <button
                  className="mt-2 inline-flex items-center gap-1.5 rounded-xl border border-theme-control-border bg-theme-control px-4 py-2 text-code-sm font-medium text-primary shadow-sm transition-colors hover:bg-theme-control-hover"
                  onClick={onShowAllCardTypes}
                  type="button"
                >
                  <RotateCcw className="size-3.5" />
                  <span>{t("conversation.search.resetFilters")}</span>
                </button>
              ) : null}
            </div>
          ) : (
            /* Hit cards grouped by card type */
            <div className="space-y-6">
              {groupedHits.map((group) => (
                <section className="space-y-3" key={group.cardType}>
                  <div className="grid grid-cols-1 gap-3">
                    {group.hits.map((hit: ConversationSearchHit) => {
                      const hitKey = getHitKey(hit);
                      const isActive = activeHitKey === hitKey;

                      return (
                        <ConversationSearchCardItem
                          active={isActive}
                          appMetaById={appMetaById}
                          colors={contentCardColors}
                          hit={hit}
                          key={hitKey}
                          onClick={onOpenHit}
                          onMouseEnter={onSelectHit}
                          query={query}
                          refCallback={(el) => {
                            if (itemRefs?.current) {
                              if (el) itemRefs.current.set(hitKey, el);
                              else itemRefs.current.delete(hitKey);
                            }
                          }}
                          showProjectPath={showProjectPath}
                          t={t}
                        />
                      );
                    })}
                  </div>
                </section>
              ))}

              {/* Pagination / Load More Footer Area */}
              <div className="flex flex-col items-center justify-center gap-2 border-t border-theme-card-border/40 pt-5 pb-6">
                <span className="font-mono text-code-xs text-on-surface-muted">
                  已展示 {visibleHits.length} / 共 {displayedTotalCount} 个结果
                </span>

                {hasMore ? (
                  <div className="flex flex-col items-center gap-2">
                    <button
                      className="inline-flex h-9 items-center gap-2 rounded-xl border border-primary/40 bg-primary/10 px-5 text-caption font-semibold text-primary transition-[transform,background-color] hover:-translate-y-px hover:bg-primary/20 active:translate-y-0 disabled:opacity-50"
                      disabled={loadingMore}
                      onClick={() => onLoadMore?.()}
                      type="button"
                    >
                      {loadingMore ? (
                        <>
                          <Loader2 className="size-4 animate-spin" />
                          <span>正在加载更多...</span>
                        </>
                      ) : (
                        <span>加载更多搜索结果</span>
                      )}
                    </button>
                    <div className="h-1" ref={loadMoreSentinelRef} />
                  </div>
                ) : (
                  <span className="text-code-xs text-outline">
                    — 已加载全部搜索结果 —
                  </span>
                )}
              </div>
            </div>
          )}
        </RenderSafeScrollSurface>
      </section>
    );
  },
);
