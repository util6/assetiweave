import { memo } from "react";
import { CornerDownLeft, Folder, MessageSquare } from "lucide-react";
import clsx from "clsx";
import type { Translator } from "../../i18n/I18nProvider";
import type { ConversationSearchHit } from "../../types";
import type { ConversationContentCardColorSettings } from "../../store/settings/settingsSchema";
import { abbreviateHomePath } from "../../utils/path";
import { conversationIdFragment } from "../../utils/conversationIds";
import {
  type ConversationSearchAppChipMeta,
  conversationSearchCardTypeLabel,
  formatRelativeTime,
  getHitKey,
  highlightMatch,
  renderHitSnippet,
  SearchCardTypeBadge,
  SearchHitMetaChip,
} from "./conversationSearchHelpers";
import { useConversationCardKindRegistry } from "./ConversationCardKindRegistry";

export interface ConversationSearchCardItemProps {
  active?: boolean;
  appMetaById?: ReadonlyMap<string, ConversationSearchAppChipMeta>;
  colors: ConversationContentCardColorSettings;
  hit: ConversationSearchHit;
  onClick: (hit: ConversationSearchHit) => void;
  onMouseEnter?: (hit: ConversationSearchHit) => void;
  query: string;
  refCallback?: (element: HTMLElement | null) => void;
  showProjectPath?: boolean;
  t: Translator;
}

export const ConversationSearchCardItem = memo(
  function ConversationSearchCardItem({
    active = false,
    appMetaById,
    colors,
    hit,
    onClick,
    onMouseEnter,
    query,
    refCallback,
    showProjectPath = true,
    t,
  }: ConversationSearchCardItemProps) {
    const { definitions } = useConversationCardKindRegistry();
    const hitKey = getHitKey(hit);
    const appMeta = appMetaById?.get(hit.session.adapter_id);
    const appName = appMeta?.name ?? hit.session.adapter_id;
    const relativeTime = formatRelativeTime(hit.session.updated_at);
    const definition = definitions.get(hit.card_type);
    const cardTypeLabel =
      definition?.label ?? conversationSearchCardTypeLabel(hit.card_type, t);

    return (
      <button
        aria-label={t("conversation.search.openHit", {
          title: hit.session.title,
          type: cardTypeLabel,
        })}
        aria-selected={active}
        className={clsx(
          "conversation-search-card group relative flex w-full cursor-pointer flex-col gap-2 rounded-2xl border p-4 text-left transition-[background-color,border-color,box-shadow,transform] duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/60",
          active
            ? "border-primary/50 bg-primary/10 shadow-[inset_3px_0_0_0_rgb(var(--color-primary-strong)),0_2px_12px_rgb(var(--theme-glow)/0.12)]"
            : "border-theme-card-border/60 bg-theme-card/40 hover:border-theme-card-border hover:bg-theme-control-hover/70 hover:shadow-sm",
        )}
        data-hit-key={hitKey}
        onClick={() => onClick(hit)}
        onMouseEnter={() => onMouseEnter?.(hit)}
        ref={refCallback}
        type="button"
      >
        {/* Line 1: Metadata row */}
        <div className="flex min-w-0 items-center justify-between gap-2">
          <div className="flex min-w-0 flex-wrap items-center gap-1.5">
            <SearchCardTypeBadge
              cardType={hit.card_type}
              colors={colors}
              t={t}
            />
            <SearchHitMetaChip
              accentColor={appMeta?.accentColor}
              label={t("conversation.search.appChip", { app: appName })}
            />
            <SearchHitMetaChip
              className="font-mono"
              label={t("conversation.search.sessionChip", {
                sessionId: conversationIdFragment(hit.session.id),
              })}
            />
            {relativeTime ? (
              <span className="text-code-xs text-on-surface-muted">
                {relativeTime}
              </span>
            ) : null}
          </div>

          {/* Action cue */}
          <span
            className={clsx(
              "flex shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-caption font-medium transition-opacity",
              active
                ? "bg-primary/20 text-primary opacity-100"
                : "text-outline opacity-0 group-hover:opacity-100",
            )}
          >
            <span>打开</span>
            <CornerDownLeft className="size-3" />
          </span>
        </div>

        {/* Line 2: Session Title */}
        <h3
          className={clsx(
            "line-clamp-2 text-body-sm font-semibold transition-colors",
            active
              ? "text-primary"
              : "text-on-surface group-hover:text-primary",
          )}
        >
          {highlightMatch(hit.session.title, query)}
        </h3>

        {/* Line 3: Question or Contextual Path if available */}
        {hit.question_title ? (
          <div className="flex items-center gap-1.5 rounded-lg bg-theme-control/40 px-2 py-1 text-code-xs text-on-surface-variant">
            <MessageSquare className="size-3.5 shrink-0 text-primary/70" />
            <span className="line-clamp-1">
              {highlightMatch(hit.question_title, query)}
            </span>
          </div>
        ) : null}

        {/* Line 4: Snippet content with match highlighting */}
        <div className="rounded-xl border border-theme-control-border/40 bg-theme-control/30 px-3 py-2 text-body-sm leading-relaxed text-on-surface-variant">
          <span className="line-clamp-3 font-mono text-[12.5px] leading-relaxed break-words">
            {renderHitSnippet(hit, query)}
          </span>
        </div>

        {/* Line 5: Project folder path */}
        {showProjectPath && hit.session.project_path ? (
          <div className="flex items-center gap-1.5 text-code-xs text-on-surface-muted">
            <Folder className="size-3 shrink-0 text-outline" />
            <span className="truncate font-mono">
              {abbreviateHomePath(hit.session.project_path)}
            </span>
          </div>
        ) : null}
      </button>
    );
  },
);
