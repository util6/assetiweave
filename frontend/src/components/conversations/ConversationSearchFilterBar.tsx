import { memo } from "react";
import { Check, Layers, RotateCcw } from "lucide-react";
import clsx from "clsx";
import type { Translator } from "../../i18n/I18nProvider";
import type { ConversationContentCardColorSettings } from "../../store/settings/settingsSchema";
import {
  type ConversationSearchAppChipMeta,
  SearchCardTypeFilterButton,
  SemanticRoleFilterButton,
} from "./conversationSearchHelpers";

export interface ConversationSearchFilterBarProps {
  adapterId?: string | null;
  allCardTypesSelected: boolean;
  appMetaById?: ReadonlyMap<string, ConversationSearchAppChipMeta>;
  availableAdapters: string[];
  availableCardKinds: string[];
  availableSemanticRoles: string[];
  contentCardColors: ConversationContentCardColorSettings;
  countByKind: Map<string, number>;
  displayedTotalCount: number;
  hasPendingFilterChanges?: boolean;
  includeQuestions: boolean;
  loading: boolean;
  onAdapterChange?: (adapterId: string | null) => void;
  onApplyFilters?: () => void;
  onCardKindToggle: (kind: string) => void;
  onQuestionToggle: () => void;
  onResetFilters: () => void;
  onSemanticRoleToggle: (role: string) => void;
  pendingChangeCount?: number;
  query: string;
  questionCount: number;
  resultHitsCount: number;
  selectedCardKinds: string[];
  selectedSemanticRoles: string[];
  t: Translator;
}

export const ConversationSearchFilterBar = memo(
  function ConversationSearchFilterBar({
    adapterId,
    allCardTypesSelected,
    appMetaById,
    availableAdapters,
    availableCardKinds,
    availableSemanticRoles,
    contentCardColors,
    countByKind,
    displayedTotalCount,
    hasPendingFilterChanges = false,
    includeQuestions,
    loading,
    onAdapterChange,
    onApplyFilters,
    onCardKindToggle,
    onQuestionToggle,
    onResetFilters,
    onSemanticRoleToggle,
    pendingChangeCount = 0,
    query,
    questionCount,
    resultHitsCount,
    selectedCardKinds,
    selectedSemanticRoles,
    t,
  }: ConversationSearchFilterBarProps) {
    const isAnyFilterActive =
      Boolean(adapterId) ||
      !allCardTypesSelected ||
      selectedCardKinds.length > 0 ||
      selectedSemanticRoles.length > 0 ||
      !includeQuestions;

    return (
      <header className="conversation-section-header flex shrink-0 flex-col gap-2.5 border-b border-theme-card-border/40 bg-theme-toolbar/30 px-5 py-3 backdrop-blur-md">
        {/* Top line: Search stats and App Selector */}
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <h2 className="text-label-caps font-semibold text-on-surface-variant">
              {t("conversation.search.resultsTitle")}
            </h2>
            <span className="font-mono text-code-xs text-on-surface-muted">
              {loading
                ? t("conversation.search.loading")
                : displayedTotalCount > 0
                  ? t("conversation.search.resultsCount", {
                      count: displayedTotalCount,
                      query,
                    })
                  : t("conversation.search.empty")}
            </span>
          </div>

          {/* Source App Filter Pills */}
          {availableAdapters.length > 0 ? (
            <div
              aria-label="按来源应用筛选"
              className="flex items-center gap-1 rounded-full border border-theme-card-border/70 bg-theme-control/40 p-0.5"
              role="group"
            >
              <button
                aria-pressed={!adapterId}
                className={clsx(
                  "inline-flex h-6 items-center gap-1 rounded-full px-2.5 text-caption font-medium transition-all",
                  !adapterId
                    ? "bg-primary/20 text-primary shadow-xs"
                    : "text-on-surface-variant hover:bg-theme-control-hover hover:text-on-surface",
                )}
                onClick={() => onAdapterChange?.(null)}
                type="button"
              >
                <Layers className="size-3" />
                <span>全部应用</span>
              </button>
              {availableAdapters.map((id) => {
                const meta = appMetaById?.get(id);
                const name = meta?.name ?? id;
                const isSelected = adapterId === id;
                return (
                  <button
                    aria-pressed={isSelected}
                    className={clsx(
                      "inline-flex h-6 items-center gap-1.5 rounded-full px-2.5 text-caption font-medium transition-all",
                      isSelected
                        ? "bg-primary/20 text-primary shadow-xs"
                        : "text-on-surface-variant hover:bg-theme-control-hover hover:text-on-surface",
                    )}
                    key={id}
                    onClick={() => onAdapterChange?.(id)}
                    type="button"
                  >
                    {meta?.accentColor ? (
                      <span
                        className="size-1.5 rounded-full"
                        style={{ backgroundColor: meta.accentColor }}
                      />
                    ) : null}
                    <span>{name}</span>
                  </button>
                );
              })}
            </div>
          ) : null}
        </div>

        {/* Bottom line: Card Types & Semantic Roles Filter Pills */}
        <div
          aria-label={t("conversation.search.typeFilterAria")}
          className="flex min-w-0 flex-wrap items-center gap-1.5 py-0.5"
          role="group"
        >
          <button
            aria-label={t("conversation.search.type.all")}
            aria-pressed={allCardTypesSelected}
            className={clsx(
              "inline-flex h-7 shrink-0 items-center gap-1.5 rounded-full border px-3 text-caption font-medium transition-[transform,background-color,border-color,box-shadow,color] duration-150 hover:-translate-y-px active:translate-y-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55",
              allCardTypesSelected
                ? "border-primary/50 bg-primary/15 text-primary shadow-sm"
                : "border-theme-control-border/80 bg-theme-control/70 text-on-surface-variant hover:bg-theme-control-hover hover:text-on-surface",
            )}
            onClick={onResetFilters}
            type="button"
          >
            <span>{t("conversation.search.type.all")}</span>
            {resultHitsCount > 0 ? (
              <span
                aria-hidden="true"
                className="rounded-full bg-primary/20 px-1 text-[10px] font-mono leading-none text-primary"
              >
                {resultHitsCount}
              </span>
            ) : null}
          </button>

          <SearchCardTypeFilterButton
            active={includeQuestions}
            cardType="question"
            colors={contentCardColors}
            count={questionCount}
            disabled={false}
            onClick={onQuestionToggle}
            t={t}
          />

          {availableCardKinds.map((cardType: string) => (
            <SearchCardTypeFilterButton
              active={selectedCardKinds.includes(cardType)}
              cardType={cardType}
              colors={contentCardColors}
              count={countByKind.get(cardType)}
              disabled={false}
              key={cardType}
              onClick={() => onCardKindToggle(cardType)}
              t={t}
            />
          ))}

          {availableSemanticRoles.map((role: string) => (
            <SemanticRoleFilterButton
              active={selectedSemanticRoles.includes(role)}
              key={role}
              onClick={() => onSemanticRoleToggle(role)}
              role={role}
            />
          ))}

          {/* Action Area: Apply Filters button when pending changes exist */}
          {hasPendingFilterChanges ? (
            <button
              aria-label="应用筛选"
              className="ml-auto inline-flex h-7 shrink-0 items-center gap-1.5 rounded-full border border-primary/50 bg-primary/20 px-3 text-caption font-semibold text-primary shadow-xs transition-all hover:bg-primary/30 active:scale-95"
              onClick={onApplyFilters}
              type="button"
            >
              <Check className="size-3 stroke-[2.5]" />
              <span>应用筛选</span>
              {pendingChangeCount > 0 ? (
                <span className="rounded-full bg-primary/30 px-1 text-[10px] font-mono leading-none">
                  {pendingChangeCount}
                </span>
              ) : null}
              <kbd className="hidden text-[10px] font-mono opacity-75 sm:inline">
                ↵
              </kbd>
            </button>
          ) : null}

          {isAnyFilterActive ? (
            <button
              className={clsx(
                "inline-flex h-7 shrink-0 items-center gap-1.5 rounded-full border border-theme-control-border/70 bg-theme-control/50 px-3 text-caption font-medium text-outline transition-colors hover:bg-theme-control-hover hover:text-primary",
                !hasPendingFilterChanges && "ml-auto",
              )}
              onClick={onResetFilters}
              title={t("conversation.search.resetFilters")}
              type="button"
            >
              <RotateCcw className="size-3" />
              <span>{t("conversation.search.resetFilters")}</span>
            </button>
          ) : null}
        </div>
      </header>
    );
  },
);
