import { type CSSProperties, type ReactNode } from "react";
import { Search } from "lucide-react";
import clsx from "clsx";

import type { Translator } from "../../i18n/I18nProvider";
import { cn } from "../../lib/utils";
import type {
  ConversationSearchCardType,
  ConversationSearchHit,
} from "../../types";
import type { ConversationContentCardColorSettings } from "../../store/settings/settingsSchema";
import {
  conversationCardColor,
  conversationCardLabel,
} from "./ConversationContentCards";
import {
  ConversationCardKindIcon,
  conversationCardPresentationKind,
  useConversationCardKindRegistry,
} from "./ConversationCardKindRegistry";

export interface ConversationSearchAppChipMeta {
  accentColor?: string | null;
  name: string;
}

export function getHitKey(hit: ConversationSearchHit): string {
  return `${hit.session.id}-${hit.block_id}-${hit.question_id}-${hit.part_id ?? ""}`;
}

/**
 * Highlights matches in text according to Tantivy highlight_segments or fallback query matching.
 */
export function renderHitSnippet(
  hit: ConversationSearchHit,
  query: string,
): ReactNode {
  if (hit.highlight_segments && hit.highlight_segments.length > 0) {
    return hit.highlight_segments.map((segment, idx) =>
      segment.matched ? (
        <mark
          className="rounded-md bg-primary/25 px-1 py-0.5 font-semibold text-primary shadow-[0_0_10px_rgb(var(--theme-glow)/0.15)]"
          key={idx}
        >
          {segment.text}
        </mark>
      ) : (
        segment.text
      ),
    );
  }
  return highlightMatch(hit.snippet, query);
}

export function highlightMatch(text: string, query: string): ReactNode {
  if (!query || !query.trim()) return text;
  const trimmed = query.trim();
  const escaped = trimmed.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const regex = new RegExp(`(${escaped})`, "gi");
  const parts = text.split(regex);
  if (parts.length === 1) return text;
  return parts.map((part, idx) =>
    regex.test(part) ? (
      <mark
        className="rounded-md bg-primary/25 px-1 py-0.5 font-semibold text-primary shadow-[0_0_10px_rgb(var(--theme-glow)/0.15)]"
        key={idx}
      >
        {part}
      </mark>
    ) : (
      part
    ),
  );
}

export function SearchHitMetaChip({
  accentColor,
  className = "",
  label,
}: {
  accentColor?: string | null;
  className?: string;
  label: string;
}) {
  return (
    <span
      className={cn(
        "inline-flex h-5 max-w-full items-center overflow-hidden text-ellipsis whitespace-nowrap rounded-md border border-theme-control-border bg-theme-control/80 px-2 text-code-xs font-medium text-on-surface-variant shadow-[var(--theme-shadow-control-inset)]",
        className,
      )}
      style={
        accentColor ? searchHitMetaChipAccentStyle(accentColor) : undefined
      }
      title={label}
    >
      {label}
    </span>
  );
}

function searchHitMetaChipAccentStyle(accentColor: string): CSSProperties {
  return {
    backgroundColor: `${accentColor}18`,
    borderColor: `${accentColor}44`,
    color: accentColor,
  };
}

export function SearchCardTypeFilterButton({
  active,
  cardType,
  colors,
  count,
  disabled,
  onClick,
  t,
}: {
  active: boolean;
  cardType: ConversationSearchCardType;
  colors: ConversationContentCardColorSettings;
  count?: number;
  disabled: boolean;
  onClick: () => void;
  t: Translator;
}) {
  const { definitions } = useConversationCardKindRegistry();
  const definition = definitions.get(cardType);
  const palette = searchCardTypePalette(cardType, colors);

  const label =
    definition?.label ?? conversationSearchCardTypeLabel(cardType, t);

  return (
    <button
      aria-label={label}
      aria-pressed={active}
      className={clsx(
        "inline-flex h-7 shrink-0 items-center gap-1.5 rounded-full border px-2.5 text-caption font-medium transition-[transform,background-color,border-color,box-shadow,color] duration-150 hover:-translate-y-px active:translate-y-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55 disabled:cursor-default",
        active
          ? "shadow-sm"
          : "border-theme-control-border/80 bg-theme-control/70 text-on-surface-variant hover:bg-theme-control-hover hover:text-on-surface",
      )}
      disabled={disabled}
      onClick={onClick}
      style={{
        backgroundColor: active ? palette.backgroundColor : undefined,
        borderColor: active ? palette.borderColor : undefined,
        color: active ? palette.accentColor : undefined,
      }}
      type="button"
    >
      {cardType === "question" ? (
        <span
          className="size-2 rounded-full"
          style={{ backgroundColor: palette.accentColor }}
        />
      ) : (
        <ConversationCardKindIcon
          iconHint={definition?.icon_hint}
          kind={cardType}
          renderer={definition?.default_renderer ?? "plain"}
          size={13}
        />
      )}
      <span>{label}</span>
      {typeof count === "number" && count > 0 ? (
        <span
          aria-hidden="true"
          className={`ml-0.5 inline-flex min-w-4 items-center justify-center rounded-full px-1 text-[10px] font-mono leading-none ${
            active ? "" : "bg-theme-control-border/25 text-on-surface-variant"
          }`}
          style={
            active
              ? {
                  backgroundColor: palette.borderColor,
                  color: palette.accentColor,
                }
              : undefined
          }
        >
          {count}
        </span>
      ) : null}
    </button>
  );
}

export function SemanticRoleFilterButton({
  active,
  onClick,
  role,
}: {
  active: boolean;
  onClick: () => void;
  role: string;
}) {
  return (
    <button
      aria-label={`role:${role}`}
      aria-pressed={active}
      className={`inline-flex h-7 shrink-0 items-center rounded-full border px-2.5 text-caption font-medium transition-[transform,background-color,border-color,box-shadow,color] duration-150 hover:-translate-y-px active:translate-y-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55 ${
        active
          ? "border-primary/50 bg-primary/15 text-primary shadow-sm"
          : "border-theme-control-border/80 bg-theme-control/70 text-on-surface-variant hover:bg-theme-control-hover hover:text-on-surface"
      }`}
      onClick={onClick}
      type="button"
    >
      {friendlySemanticRoleLabel(role)}
    </button>
  );
}

export function SearchCardTypeBadge({
  cardType,
  colors,
  t,
}: {
  cardType: ConversationSearchCardType;
  colors: ConversationContentCardColorSettings;
  t: Translator;
}) {
  const { definitions } = useConversationCardKindRegistry();
  const definition = definitions.get(cardType);
  const presentationType = conversationCardPresentationKind(
    cardType,
    definition?.semantic_role,
  );
  const presentationDefinition =
    presentationType === cardType ? definition : undefined;
  const palette = searchCardTypePalette(presentationType, colors);

  return (
    <span
      className="inline-flex shrink-0 items-center gap-1 rounded-full border px-2 py-0.5 text-caption font-medium"
      data-search-card-type-badge={cardType}
      style={{
        backgroundColor: palette.backgroundColor,
        borderColor: palette.borderColor,
        color: palette.accentColor,
      }}
    >
      {cardType === "question" ? (
        <span
          className="size-1.5 rounded-full"
          style={{ backgroundColor: palette.accentColor }}
        />
      ) : (
        <ConversationCardKindIcon
          iconHint={definition?.icon_hint}
          kind={cardType}
          renderer={definition?.default_renderer ?? "plain"}
          size={11}
        />
      )}
      <span>
        {presentationDefinition?.label ??
          conversationSearchCardTypeLabel(presentationType, t)}
      </span>
    </span>
  );
}

export function searchCardTypePalette(
  cardType: ConversationSearchCardType,
  colors: ConversationContentCardColorSettings,
) {
  if (cardType === "question") {
    return {
      accentColor: "rgb(var(--color-primary-strong))",
      backgroundColor: "rgb(var(--color-primary-strong) / 0.12)",
      borderColor: "rgb(var(--color-primary-strong) / 0.42)",
    };
  }
  const accentColor = conversationCardColor(cardType, colors);
  return {
    accentColor,
    backgroundColor: hexWithAlpha(accentColor, "18"),
    borderColor: hexWithAlpha(accentColor, "66"),
  };
}

function hexWithAlpha(hexColor: string, alpha: string) {
  return `${hexColor}${alpha}`;
}

export function conversationSearchCardTypeLabel(
  cardType: ConversationSearchCardType,
  t: Translator,
) {
  if (cardType === "question") {
    return t("conversation.search.card.question");
  }
  return conversationCardLabel(cardType, t);
}

export function formatRelativeTime(isoString?: string | null): string {
  if (!isoString) return "";
  try {
    const timestamp = new Date(isoString).getTime();
    if (Number.isNaN(timestamp)) return "";
    const diff = Math.floor((Date.now() - timestamp) / 1000);
    if (diff < 60) return "刚刚";
    if (diff < 3600) return `${Math.floor(diff / 60)}分钟前`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}小时前`;
    if (diff < 86400 * 30) return `${Math.floor(diff / 86400)}天前`;
    const date = new Date(timestamp);
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
  } catch {
    return "";
  }
}

export function friendlySemanticRoleLabel(role: string): string {
  switch (role) {
    case "answer":
      return "AI 解答";
    case "tool":
      return "工具操作";
    case "command":
      return "命令执行";
    case "code":
      return "代码变更";
    case "result":
      return "执行结果";
    default:
      return role;
  }
}

export function filterVisibleConversationSearchHits(
  hits: ConversationSearchHit[],
  options: {
    adapterId?: string | null;
    definitions?: ReadonlyMap<string, { semantic_role?: string | null }>;
    includeQuestions: boolean;
    selectedCardKinds: string[];
    selectedSemanticRoles: string[];
  },
): ConversationSearchHit[] {
  const {
    adapterId,
    definitions,
    includeQuestions,
    selectedCardKinds,
    selectedSemanticRoles,
  } = options;
  return hits.filter((hit) => {
    if (adapterId && hit.session.adapter_id !== adapterId) {
      return false;
    }
    if (hit.card_type === "question") return includeQuestions;

    const hitRole =
      (hit as unknown as { semantic_role?: string }).semantic_role ??
      definitions?.get(hit.card_type)?.semantic_role ??
      (hit.card_type.includes(".")
        ? hit.card_type.split(".").pop()
        : hit.card_type);
    const matchesKind =
      selectedCardKinds.length === 0 ||
      selectedCardKinds.includes(hit.card_type);
    const matchesRole =
      selectedSemanticRoles.length === 0 ||
      Boolean(hitRole && selectedSemanticRoles.includes(hitRole));

    if (selectedCardKinds.length > 0 && selectedSemanticRoles.length > 0) {
      return matchesKind && matchesRole;
    }
    if (selectedCardKinds.length > 0) return matchesKind;
    if (selectedSemanticRoles.length > 0) return matchesRole;
    return true;
  });
}

export function countHitsByKind(hits: ConversationSearchHit[]): {
  questionCount: number;
  countByKind: Map<string, number>;
} {
  const countByKind = new Map<string, number>();
  let questionCount = 0;
  for (const h of hits) {
    if (h.card_type === "question") {
      questionCount++;
    } else {
      countByKind.set(h.card_type, (countByKind.get(h.card_type) ?? 0) + 1);
    }
  }
  return { questionCount, countByKind };
}

/**
 * Trigger component for mounting on the toolbar to open the search modal.
 */
export function ConversationSearchTrigger({
  activeQuery,
  className,
  disabled = false,
  onClick,
  placeholder,
  shortcut = "⌘K",
}: {
  activeQuery?: string;
  className?: string;
  disabled?: boolean;
  onClick: () => void;
  placeholder: string;
  shortcut?: string;
}) {
  const displayLabel = activeQuery?.trim() ? activeQuery.trim() : placeholder;
  return (
    <button
      aria-label={placeholder}
      className={clsx(
        "group flex h-10 min-w-[16rem] shrink-0 items-center justify-between gap-2.5 rounded-2xl border border-theme-control-border/80 bg-theme-control/70 px-3.5 text-left text-body-sm text-outline shadow-[var(--theme-shadow-control-inset)] backdrop-blur-md transition-[transform,border-color,background-color,box-shadow,color] duration-200 hover:-translate-y-px hover:border-primary/50 hover:bg-theme-control/95 hover:text-on-surface active:translate-y-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55 disabled:cursor-not-allowed disabled:opacity-60",
        className,
      )}
      disabled={disabled}
      onClick={onClick}
      type="button"
    >
      <div className="flex min-w-0 items-center gap-2">
        <Search className="size-4 shrink-0 transition-colors group-hover:text-primary" />
        <span
          className={clsx(
            "truncate font-medium",
            activeQuery?.trim() ? "text-on-surface" : "text-outline",
          )}
        >
          {displayLabel}
        </span>
      </div>
      {shortcut ? (
        <kbd className="hidden shrink-0 items-center rounded-lg border border-theme-control-border bg-theme-control px-2 py-0.5 font-mono text-[11px] font-medium text-on-surface-variant group-hover:border-primary/30 group-hover:text-primary sm:inline-flex shadow-sm">
          {shortcut}
        </kbd>
      ) : null}
    </button>
  );
}
