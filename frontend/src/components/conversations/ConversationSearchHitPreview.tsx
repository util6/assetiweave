import {
  Check,
  ChevronDown,
  ChevronUp,
  Code2,
  Copy,
  CornerDownLeft,
  FileText,
  Folder,
  Layers,
  MessageSquare,
  Sparkles,
  Terminal,
} from "lucide-react";
import { useState } from "react";
import clsx from "clsx";

import type { Translator } from "../../i18n/I18nProvider";
import type { ConversationSearchHit } from "../../types";
import type { ConversationContentCardColorSettings } from "../../store/settings/settingsSchema";
import { abbreviateHomePath } from "../../utils/path";
import { conversationIdFragment } from "../../utils/conversationIds";
import { Button } from "../ui/button";
import { RenderSafeScrollSurface } from "../common/rendering/RenderSafeScrollSurface";
import {
  type ConversationSearchAppChipMeta,
  SearchCardTypeBadge,
  SearchHitMetaChip,
  renderHitSnippet,
} from "./conversationSearchHelpers";

export interface ConversationSearchHitPreviewProps {
  activeHit: ConversationSearchHit | null;
  appMetaById?: ReadonlyMap<string, ConversationSearchAppChipMeta>;
  contentCardColors: ConversationContentCardColorSettings;
  currentIndex?: number;
  onNavigateHit?: (direction: "prev" | "next") => void;
  onOpenHit: (hit: ConversationSearchHit) => void;
  query: string;
  t: Translator;
  totalHits?: number;
}

export function ConversationSearchHitPreview({
  activeHit,
  appMetaById,
  contentCardColors,
  currentIndex,
  onNavigateHit,
  onOpenHit,
  query,
  t,
  totalHits,
}: ConversationSearchHitPreviewProps) {
  const [copied, setCopied] = useState(false);
  const [copiedId, setCopiedId] = useState(false);

  const handleCopySnippet = (text: string) => {
    void navigator.clipboard.writeText(text);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1800);
  };

  const handleCopySessionId = (id: string) => {
    void navigator.clipboard.writeText(id);
    setCopiedId(true);
    window.setTimeout(() => setCopiedId(false), 1800);
  };

  if (!activeHit) {
    return (
      <div className="flex min-h-0 flex-1 flex-col items-center justify-center p-8 text-center">
        <div className="grid size-16 place-items-center rounded-3xl border border-primary/25 bg-primary/10 text-primary shadow-[0_0_32px_rgb(var(--theme-glow)/0.25)]">
          <Sparkles className="size-8 animate-pulse" />
        </div>
        <h3 className="mt-5 text-title-xs font-bold text-on-surface">
          {t("conversation.search.quickHintsTitle")}
        </h3>
        <p className="mt-2 max-w-sm text-body-sm leading-relaxed text-on-surface-variant">
          {t("conversation.search.quickHintsDesc")}
        </p>

        <div className="mt-6 flex flex-wrap items-center justify-center gap-2">
          {[
            "用户问题",
            "回答文字",
            "代码变更",
            "终端命令",
            "Skill 资产",
            "工具调用",
          ].map((label) => (
            <span
              className="rounded-full border border-theme-control-border bg-theme-control/70 px-3 py-1 text-caption font-medium text-on-surface-variant shadow-[var(--theme-shadow-control-inset)]"
              key={label}
            >
              {label}
            </span>
          ))}
        </div>

        <div className="mt-8 flex items-center gap-3 rounded-2xl border border-theme-card-border/60 bg-theme-control/40 px-4 py-2.5 text-code-xs text-on-surface-muted shadow-sm">
          <span className="flex items-center gap-1 font-medium">
            <kbd className="rounded border border-theme-control-border bg-theme-control px-1.5 py-0.5 shadow-sm">
              ↑↓
            </kbd>
            {t("conversation.search.shortcutNav")}
          </span>
          <span>·</span>
          <span className="flex items-center gap-1 font-medium">
            <kbd className="rounded border border-theme-control-border bg-theme-control px-1.5 py-0.5 shadow-sm">
              ↵
            </kbd>
            {t("conversation.search.shortcutOpen")}
          </span>
          <span>·</span>
          <span className="flex items-center gap-1 font-medium">
            <kbd className="rounded border border-theme-control-border bg-theme-control px-1.5 py-0.5 shadow-sm">
              Esc
            </kbd>
            {t("conversation.search.dialogClose")}
          </span>
        </div>
      </div>
    );
  }

  const appMeta = appMetaById?.get(activeHit.session.adapter_id);
  const appName = appMeta?.name ?? activeHit.session.adapter_id;
  const isCodeOrCommand =
    activeHit.card_type === "code" ||
    activeHit.card_type === "command" ||
    activeHit.card_type === "result";

  const hasPrev = typeof currentIndex === "number" && currentIndex > 0;
  const hasNext =
    typeof currentIndex === "number" &&
    typeof totalHits === "number" &&
    currentIndex < totalHits - 1;

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      {/* Top Breadcrumb & Navigation Bar */}
      <div className="shrink-0 border-b border-theme-card-border/40 bg-theme-card-header/40 px-5 py-3.5 backdrop-blur-md">
        <div className="flex items-center justify-between gap-3">
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <SearchCardTypeBadge
              cardType={activeHit.card_type}
              colors={contentCardColors}
              t={t}
            />
            <SearchHitMetaChip
              accentColor={appMeta?.accentColor}
              label={t("conversation.search.appChip", { app: appName })}
            />
            <SearchHitMetaChip
              className="font-mono"
              label={t("conversation.search.sessionChip", {
                sessionId: conversationIdFragment(activeHit.session.id),
              })}
            />
          </div>

          {/* Stepper buttons (prev / next hit) */}
          {typeof currentIndex === "number" &&
          typeof totalHits === "number" &&
          totalHits > 1 ? (
            <div className="flex shrink-0 items-center gap-1.5">
              <span className="font-mono text-code-xs text-on-surface-muted">
                {currentIndex + 1} / {totalHits}
              </span>
              <div className="flex items-center rounded-xl border border-theme-control-border bg-theme-control/80 p-0.5 shadow-sm">
                <button
                  aria-label="Previous hit"
                  className="rounded-lg p-1 text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:opacity-30"
                  disabled={!hasPrev}
                  onClick={() => onNavigateHit?.("prev")}
                  title="上一条 (↑)"
                  type="button"
                >
                  <ChevronUp className="size-3.5" />
                </button>
                <button
                  aria-label="Next hit"
                  className="rounded-lg p-1 text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:opacity-30"
                  disabled={!hasNext}
                  onClick={() => onNavigateHit?.("next")}
                  title="下一条 (↓)"
                  type="button"
                >
                  <ChevronDown className="size-3.5" />
                </button>
              </div>
            </div>
          ) : null}
        </div>

        {/* Session title */}
        <h3 className="mt-2.5 truncate text-body-base font-bold text-on-surface">
          {activeHit.session.title}
        </h3>

        {/* Secondary metadata */}
        <div className="mt-1.5 flex flex-wrap items-center gap-x-4 gap-y-1 text-code-xs text-on-surface-variant">
          {activeHit.session.project_path ? (
            <span className="flex min-w-0 items-center gap-1.5 truncate font-mono text-on-surface-muted">
              <Folder className="size-3.5 shrink-0 text-outline" />
              <span className="truncate">
                {abbreviateHomePath(activeHit.session.project_path)}
              </span>
            </span>
          ) : null}
          {activeHit.turn_id ? (
            <span className="flex items-center gap-1 font-mono text-on-surface-muted">
              <Layers className="size-3 shrink-0 text-outline" />
              <span>Turn #{activeHit.question_index + 1}</span>
            </span>
          ) : null}
        </div>
      </div>

      {/* Main Scrollable Content */}
      <RenderSafeScrollSurface
        aria-label={t("conversation.search.previewTitle")}
        className="min-h-0 flex-1 space-y-4 p-5"
        tabIndex={0}
      >
        {/* Context: Related Question Banner (if hit is not the question itself) */}
        {activeHit.card_type !== "question" && activeHit.question_title ? (
          <div className="flex items-start gap-2.5 rounded-2xl border border-primary/20 bg-primary/5 p-3.5 text-body-sm shadow-[var(--theme-shadow-control-inset)]">
            <MessageSquare className="mt-0.5 size-4 shrink-0 text-primary" />
            <div className="min-w-0 flex-1">
              <span className="text-caption font-semibold uppercase tracking-wider text-primary/80">
                关联提问
              </span>
              <p className="mt-0.5 text-body-sm font-medium text-on-surface line-clamp-2">
                {activeHit.question_title}
              </p>
            </div>
          </div>
        ) : null}

        {/* Snippet Card with Mac-style header */}
        <div className="overflow-hidden rounded-2xl border border-theme-card-border/60 bg-theme-control/40 shadow-[var(--theme-shadow-control-inset)]">
          {/* Card Header Bar */}
          <div className="flex items-center justify-between border-b border-theme-card-border/40 bg-theme-card-header/50 px-4 py-2.5">
            <div className="flex items-center gap-2">
              {/* Decorative Mac window dots */}
              <div className="flex items-center gap-1.5 pr-2">
                <span className="size-2.5 rounded-full bg-status-remove/80" />
                <span className="size-2.5 rounded-full bg-status-conflict/80" />
                <span className="size-2.5 rounded-full bg-status-create/80" />
              </div>
              <span className="flex items-center gap-1.5 text-code-xs font-semibold text-on-surface-variant">
                {isCodeOrCommand ? (
                  activeHit.card_type === "code" ? (
                    <Code2 className="size-3.5 text-primary" />
                  ) : (
                    <Terminal className="size-3.5 text-primary" />
                  )
                ) : (
                  <FileText className="size-3.5 text-primary" />
                )}
                {t("conversation.search.previewTitle")}
              </span>
            </div>

            {/* Copy button */}
            <button
              className="inline-flex items-center gap-1.5 rounded-xl border border-theme-control-border bg-theme-control px-2.5 py-1 text-code-xs font-medium text-on-surface-variant shadow-sm transition-[background-color,color] hover:bg-theme-control-hover hover:text-on-surface"
              onClick={() => handleCopySnippet(activeHit.snippet)}
              type="button"
            >
              {copied ? (
                <Check className="size-3.5 text-status-success" />
              ) : (
                <Copy className="size-3.5" />
              )}
              <span>
                {copied
                  ? t("conversation.search.copied")
                  : t("conversation.search.copySnippet")}
              </span>
            </button>
          </div>

          {/* Snippet Body */}
          <div
            className={clsx(
              "p-4 select-text leading-relaxed",
              isCodeOrCommand
                ? "bg-theme-control/60 font-mono text-code-sm whitespace-pre-wrap text-on-surface shadow-inner"
                : "text-body-sm whitespace-pre-wrap text-on-surface",
            )}
          >
            {renderHitSnippet(activeHit, query)}
          </div>
        </div>

        {/* Session Details Footer Card */}
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-theme-card-border/40 bg-theme-card/30 px-3.5 py-2.5 text-code-xs text-on-surface-muted">
          <span className="truncate font-mono">
            Session: {activeHit.session.id}
          </span>
          <button
            className="inline-flex items-center gap-1 text-primary hover:underline"
            onClick={() => handleCopySessionId(activeHit.session.id)}
            type="button"
          >
            {copiedId ? (
              <Check className="size-3 text-status-success" />
            ) : (
              <Copy className="size-3" />
            )}
            <span>{copiedId ? "已复制 ID" : "复制完整 ID"}</span>
          </button>
        </div>
      </RenderSafeScrollSurface>

      {/* Primary Action Button Bar */}
      <div className="shrink-0 border-t border-theme-card-border/40 bg-theme-card-header/40 p-4 backdrop-blur-md">
        <Button
          className="h-11 w-full gap-2 text-body-base font-semibold shadow-lg shadow-primary/20"
          onClick={() => onOpenHit(activeHit)}
          size="default"
          type="button"
          variant="default"
        >
          <CornerDownLeft size={17} />
          <span>{t("conversation.search.openSessionAndJump")}</span>
        </Button>
      </div>
    </div>
  );
}
