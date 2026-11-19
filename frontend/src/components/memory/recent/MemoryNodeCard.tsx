import clsx from "clsx";
import {
  CheckCircle2,
  CornerDownRight,
  ExternalLink,
  EyeOff,
  GitBranch,
  RotateCcw,
  Terminal,
} from "lucide-react";
import type { Translator } from "../../../i18n/I18nProvider";
import type {
  MemoryNavigationTarget,
  MemoryNodeStatus,
  RecentTreeNodeView,
} from "../../../types/memory";
import { SessionShortIdChip } from "../../conversations/SessionShortIdChip";
import { Badge } from "../../foundation/Badge";
import { TruncatedText } from "../../foundation/TruncatedText";
import { Button } from "../../ui/button";
import {
  buildNodeNavigationTarget,
  formatClockTime,
  formatShortTime,
  getStatusBadgeTone,
  parseNodeAnchor,
} from "./recentModel";

export interface MemoryNodeCardProps {
  compactTime?: boolean;
  contextLabel?: string | null;
  highlightUnexplored?: boolean;
  node: RecentTreeNodeView;
  onNavigateSession?: (target: MemoryNavigationTarget) => void;
  onStatusChange?: (
    node: RecentTreeNodeView,
    nextStatus: MemoryNodeStatus,
  ) => void;
  showParentContext?: boolean;
  t: Translator;
  updating?: boolean;
}

export function MemoryNodeCard({
  compactTime = false,
  contextLabel,
  highlightUnexplored = false,
  node,
  onNavigateSession,
  onStatusChange,
  showParentContext = false,
  t,
  updating = false,
}: MemoryNodeCardProps) {
  const parsedAnchor = parseNodeAnchor(node.id);
  const navTarget = buildNodeNavigationTarget(node);
  const sessionIdForChip =
    node.sourceSessionId?.trim() || parsedAnchor?.shortId || "";
  const turnLabel =
    node.turnIndex != null
      ? `T${node.turnIndex}`
      : parsedAnchor
        ? parsedAnchor.subIndex !== undefined
          ? `T${parsedAnchor.turnNumber}.${parsedAnchor.subIndex}`
          : `T${parsedAnchor.turnNumber}`
        : null;
  const timeText = compactTime
    ? formatClockTime(node.recordedAt)
    : formatShortTime(node.recordedAt);
  const isUnavailable = node.sessionAvailable === false;
  const isUnexplored = node.status === "unexplored";
  const isDismissed = node.status === "dismissed";

  return (
    <article
      className={clsx(
        "group relative flex flex-col gap-2.5 rounded-2xl border p-3.5 transition-[transform,border-color,background-color,box-shadow] duration-200",
        highlightUnexplored && isUnexplored
          ? "border-status-conflict/35 bg-[linear-gradient(145deg,rgb(var(--theme-card-bg)/0.88),rgb(var(--theme-card-header)/0.78))] shadow-[0_8px_24px_rgb(var(--theme-glow)/0.08)]"
          : "border-theme-card-border/75 bg-theme-card/72 hover:border-theme-nav-active-border/55 hover:bg-theme-card/90",
        isDismissed && "opacity-70",
      )}
      data-node-id={node.id}
      data-node-status={node.status}
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex min-w-0 flex-wrap items-center gap-1.5">
          <Badge tone={getStatusBadgeTone(node.status)}>
            {t(`memory.recent.status.${node.status}`)}
          </Badge>

          {contextLabel && <Badge tone="neutral">{contextLabel}</Badge>}

          {turnLabel && (
            <span className="inline-flex items-center gap-1 rounded-full border border-theme-control-border/75 bg-theme-control/75 px-2.5 py-0.5 font-mono text-caption text-on-surface-variant">
              {turnLabel}
            </span>
          )}

          {sessionIdForChip && (
            <SessionShortIdChip sessionId={sessionIdForChip} t={t} />
          )}

          {node.sourceAgent && (
            <span className="inline-flex items-center gap-1 rounded-full border border-theme-control-border/70 bg-theme-control/60 px-2 py-0.5 text-caption text-on-surface-muted">
              <Terminal size={11} />
              <span>{node.sourceAgent}</span>
            </span>
          )}
        </div>

        {timeText && (
          <time
            className="shrink-0 font-mono text-caption text-outline"
            dateTime={node.recordedAt ?? undefined}
          >
            {timeText}
          </time>
        )}
      </div>

      {showParentContext && node.parentSummary && (
        <div className="flex items-start gap-1.5 rounded-xl border border-theme-card-border/50 bg-theme-control/45 px-2.5 py-1.5 text-caption text-on-surface-muted">
          <CornerDownRight className="mt-0.5 shrink-0 text-outline" size={12} />
          <TruncatedText lines={2}>
            {t("memory.recent.parentContext", { summary: node.parentSummary })}
          </TruncatedText>
        </div>
      )}

      <p className="text-body-sm leading-relaxed text-on-surface">
        {node.summary}
      </p>

      {(node.sessionTitle ||
        isUnavailable ||
        onStatusChange ||
        onNavigateSession) && (
        <div className="flex flex-wrap items-center justify-between gap-2 pt-0.5">
          <div className="min-w-0 flex-1">
            {node.sessionTitle && (
              <TruncatedText
                as="p"
                className="text-caption text-on-surface-muted"
              >
                {node.sessionTitle}
              </TruncatedText>
            )}
            {isUnavailable && (
              <p className="text-caption text-outline">
                {t("memory.recent.sessionUnavailableHint")}
              </p>
            )}
          </div>

          <div className="flex flex-wrap items-center gap-1.5">
            {onStatusChange && isUnexplored && (
              <>
                <Button
                  aria-label={`${t("memory.recent.action.markCompleted")}: ${node.id}`}
                  disabled={updating}
                  onClick={() => onStatusChange(node, "completed")}
                  size="sm"
                  type="button"
                  variant="secondary"
                >
                  <CheckCircle2 size={13} />
                  <span>{t("memory.recent.action.markCompleted")}</span>
                </Button>
                <Button
                  aria-label={`${t("memory.recent.action.dismiss")}: ${node.id}`}
                  disabled={updating}
                  onClick={() => onStatusChange(node, "dismissed")}
                  size="sm"
                  type="button"
                  variant="ghost"
                >
                  <EyeOff size={13} />
                  <span>{t("memory.recent.action.dismiss")}</span>
                </Button>
              </>
            )}

            {onStatusChange &&
              (node.status === "completed" || node.status === "continued") && (
                <>
                  <Button
                    aria-label={`${t("memory.recent.action.markUnexplored")}: ${node.id}`}
                    disabled={updating}
                    onClick={() => onStatusChange(node, "unexplored")}
                    size="sm"
                    type="button"
                    variant="ghost"
                  >
                    <GitBranch size={13} />
                    <span>{t("memory.recent.action.markUnexplored")}</span>
                  </Button>
                  <Button
                    aria-label={`${t("memory.recent.action.dismiss")}: ${node.id}`}
                    disabled={updating}
                    onClick={() => onStatusChange(node, "dismissed")}
                    size="sm"
                    type="button"
                    variant="ghost"
                  >
                    <EyeOff size={13} />
                    <span>{t("memory.recent.action.dismiss")}</span>
                  </Button>
                </>
              )}

            {onStatusChange && isDismissed && (
              <Button
                aria-label={`${t("memory.recent.action.restore")}: ${node.id}`}
                disabled={updating}
                onClick={() => onStatusChange(node, "unexplored")}
                size="sm"
                type="button"
                variant="outline"
              >
                <RotateCcw size={13} />
                <span>{t("memory.recent.action.restore")}</span>
              </Button>
            )}

            {onNavigateSession && navTarget && (
              <Button
                aria-label={`${t("memory.recent.jumpToTurn", {
                  turn: navTarget.turn_id ?? "?",
                })}: ${node.id}`}
                disabled={isUnavailable}
                onClick={() => onNavigateSession(navTarget)}
                size="sm"
                title={
                  isUnavailable
                    ? t("memory.recent.sessionUnavailableHint")
                    : (node.sessionTitle ?? navTarget.session_id ?? undefined)
                }
                type="button"
                variant="outline"
              >
                <span>
                  {t("memory.recent.jumpToTurn", {
                    turn: navTarget.turn_id ?? "?",
                  })}
                </span>
                <ExternalLink size={12} />
              </Button>
            )}
          </div>
        </div>
      )}
    </article>
  );
}
