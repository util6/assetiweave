import { ArrowRight, GitFork } from "lucide-react";
import type { Translator } from "../../i18n/I18nProvider";
import { Badge } from "../foundation/Badge";
import { Button } from "../ui/button";
import type { SubagentDispatchPortalViewModel } from "./conversationSubagents";

/**
 * 正文 `Process` 序列里的子代理传送门卡片。
 *
 * 只消费父会话详情已内联的锚点元数据：不展开长篇委派 Prompt，也不为渲染
 * 卡片额外请求任何子会话详情（严格零 N+1）。点击「查看子会话」才按需下钻。
 */
export function SubagentPortalCard({
  onOpen,
  portal,
  t,
}: {
  onOpen?: (portal: SubagentDispatchPortalViewModel) => void;
  portal: SubagentDispatchPortalViewModel;
  t: Translator;
}) {
  const role =
    portal.agentRole?.trim() || t("conversation.subagent.roleFallback");
  const title = portal.title.trim();
  const openLabel = t("conversation.subagent.openSession", {
    title: title || role,
  });

  return (
    <div
      className="flex min-w-0 flex-wrap items-center gap-3 rounded-2xl border border-primary/25 bg-theme-card/55 px-3.5 py-2.5 shadow-[var(--theme-shadow-control-inset)] transition-[border-color,background-color] duration-150 hover:border-primary/40 hover:bg-theme-card/70"
      data-subagent-portal={portal.partId}
      data-subagent-unresolved={portal.unresolved ? "true" : undefined}
    >
      <span className="grid size-8 shrink-0 place-items-center rounded-xl border border-theme-control-border bg-theme-control text-primary">
        <GitFork aria-hidden="true" size={15} />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <div className="flex min-w-0 items-center gap-2">
          <Badge
            className="shrink-0 px-2 py-0.5 text-xs normal-case tracking-normal"
            title={role}
            tone="primary"
          >
            {role}
          </Badge>
          <span
            className="truncate text-body-sm font-semibold text-on-surface"
            title={title || role}
          >
            {title || t("conversation.subagent.untitled")}
          </span>
        </div>
        {portal.reportSummary ? (
          <p
            className="truncate text-body-sm text-on-surface-variant"
            data-subagent-report
            title={portal.reportSummary}
          >
            <span className="mr-1.5 text-on-surface-muted">
              {t("conversation.subagent.reportLabel")}
            </span>
            {portal.reportSummary}
          </p>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {typeof portal.stepCount === "number" ? (
          <span className="inline-flex h-6 items-center rounded-full border border-theme-control-border bg-theme-control/70 px-2 font-mono text-code-sm text-on-surface-variant">
            {t("conversation.subagent.stepCount", { count: portal.stepCount })}
          </span>
        ) : null}
        <SubagentPortalStatus status={portal.status} t={t} />
        {portal.unresolved ? (
          <Badge
            className="px-2 py-0.5 text-xs normal-case tracking-normal"
            title={t("conversation.subagent.unresolvedHint")}
            tone="neutral"
          >
            {t("conversation.subagent.unresolved")}
          </Badge>
        ) : onOpen ? (
          <Button
            aria-label={openLabel}
            className="h-8 rounded-xl px-3"
            onClick={() => onOpen(portal)}
            title={openLabel}
            type="button"
            variant="outline"
          >
            {t("conversation.subagent.open")}
            <ArrowRight aria-hidden="true" className="size-3.5" />
          </Button>
        ) : null}
      </div>
    </div>
  );
}

function SubagentPortalStatus({
  status,
  t,
}: {
  status: string;
  t: Translator;
}) {
  const normalized = status.trim().toLowerCase();
  const running = normalized === "running";
  const failed = normalized === "failed" || normalized === "error";
  const label = running
    ? t("conversation.turn.status.running")
    : failed
      ? t("conversation.subagent.status.failed")
      : t("conversation.turn.status.completed");
  return (
    <span
      className="inline-flex h-6 items-center gap-1.5 rounded-full border border-theme-control-border bg-theme-control/70 px-2 text-code-sm text-on-surface-variant"
      data-subagent-status={
        running ? "running" : failed ? "failed" : "completed"
      }
    >
      <span
        className={`size-1.5 rounded-full ${
          running
            ? "bg-status-update"
            : failed
              ? "bg-status-remove"
              : "bg-status-create"
        }`}
      />
      {label}
    </span>
  );
}
