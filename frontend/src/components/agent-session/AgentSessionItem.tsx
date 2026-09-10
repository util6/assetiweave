import {
  Activity,
  Bot,
  CheckCircle2,
  CircleAlert,
  FileText,
  LoaderCircle,
  MessageSquare,
  MoreHorizontal,
  Sparkles,
  Wrench,
  XCircle,
} from "lucide-react";
import { useState } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import type {
  AgentSessionItemKind,
  AgentSessionItemView,
} from "../../types/agentSession";

export interface AgentSessionItemProps {
  item: AgentSessionItemView;
  testIdPrefix?: string;
}

export function AgentSessionItem({
  item,
  testIdPrefix = "agent-session",
}: AgentSessionItemProps) {
  const { t } = useI18n();
  const isUser = item.kind === "user_message";
  const isCollapsible = item.kind === "tool" || item.kind === "thinking";
  const [detailsOpen, setDetailsOpen] = useState(
    item.kind === "tool" && ["streaming", "failed"].includes(item.state),
  );

  const label = getItemLabel(item.kind, t);
  const icon = getItemIcon(item.kind);
  const isTool = item.kind === "tool";
  const hasToolInput =
    isTool && item.toolInput !== undefined && item.toolInput !== null;
  const hasToolOutput =
    isTool && item.toolOutput !== undefined && item.toolOutput !== null;
  const toolTitle = item.toolName || item.text || getItemStatusText(item, t);
  const detail = item.text || item.status || getItemStatusText(item, t);

  const tone =
    item.kind === "error" || item.state === "failed"
      ? "border-status-remove/35 bg-status-remove/10"
      : isUser
        ? "border-theme-nav-active-border/35 bg-theme-nav-active/10"
        : item.kind === "final_result"
          ? "border-status-create/35 bg-status-create/10"
          : item.kind === "cancelled"
            ? "border-status-conflict/35 bg-status-conflict/10"
            : "border-theme-card-border/65 bg-theme-card/55";

  return (
    <li
      className={`rounded-xl border px-3.5 py-3 ${tone}`}
      data-testid={`${testIdPrefix}-session-item-${item.id}`}
    >
      <div className="flex items-start gap-3">
        <span className="grid size-8 shrink-0 place-items-center rounded-lg border border-theme-control-border/70 bg-theme-control/70 text-primary">
          {icon}
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className="text-label-caps uppercase text-on-surface-variant">
              {label}
            </span>
            <span className="text-caption text-outline">
              {item.delivery === "replay"
                ? (t("agentSession.replay") || t("team.chat.replay"))
                : (t("agentSession.live") || t("team.chat.live"))}
            </span>
            <span className="ml-auto text-caption text-outline">
              {item.state}
            </span>
          </div>

          {isCollapsible ? (
            <details
              className="group mt-1 rounded-lg border border-theme-control-border/45 bg-theme-control/25 px-2.5 py-1.5"
              data-testid={`${testIdPrefix}-session-item-details-${item.id}`}
              onToggle={(event) => setDetailsOpen(event.currentTarget.open)}
              open={detailsOpen}
            >
              <summary className="cursor-pointer list-none rounded-md text-body-sm text-on-surface outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/45 [&::-webkit-details-marker]:hidden">
                <span className="inline-flex items-center gap-2">
                  <span className="text-label-caps uppercase text-on-surface-variant">
                    {label}
                  </span>
                  <span className="truncate">{isTool ? toolTitle : detail}</span>
                </span>
              </summary>

              {isTool && hasToolInput ? (
                <div
                  className="mt-2"
                  data-testid={`${testIdPrefix}-tool-input-${item.id}`}
                >
                  <span className="text-label-caps uppercase font-medium text-on-surface-variant">
                    {t("agentSession.tool.input") || "Input"}
                  </span>
                  <pre className="mt-1 max-h-60 overflow-auto rounded bg-theme-control/40 p-2 text-caption font-mono text-on-surface">
                    {formatPayload(item.toolInput)}
                  </pre>
                </div>
              ) : null}

              {isTool && hasToolOutput ? (
                <div
                  className="mt-2"
                  data-testid={`${testIdPrefix}-tool-output-${item.id}`}
                >
                  <span className="text-label-caps uppercase font-medium text-on-surface-variant">
                    {t("agentSession.tool.output") || "Output"}
                  </span>
                  <pre className="mt-1 max-h-60 overflow-auto rounded bg-theme-control/40 p-2 text-caption font-mono text-on-surface">
                    {formatPayload(item.toolOutput)}
                  </pre>
                </div>
              ) : null}

              {!hasToolInput && !hasToolOutput && detail && detail !== item.toolName ? (
                <p className="mt-2 whitespace-pre-wrap break-words text-body-sm text-on-surface">
                  {detail}
                </p>
              ) : null}

              {item.code ? (
                <p className="mt-1 break-words text-caption text-status-remove">
                  {item.code}
                </p>
              ) : null}
            </details>
          ) : (
            <p className="mt-1 whitespace-pre-wrap break-words text-body-sm text-on-surface">
              {detail}
            </p>
          )}

          {!isCollapsible && item.code && item.text ? (
            <p className="mt-1 break-words text-caption text-status-remove">
              {item.code}
            </p>
          ) : null}
        </div>
      </div>
    </li>
  );
}

function formatPayload(value: unknown): string {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function getItemLabel(
  kind: AgentSessionItemKind,
  t: ReturnType<typeof useI18n>["t"],
): string {
  switch (kind) {
    case "user_message":
      return t("agentSession.item.user") || t("team.chat.item.user");
    case "assistant_text":
      return t("agentSession.item.assistant") || t("team.chat.item.assistant");
    case "processing":
      return t("agentSession.item.processing") || t("team.chat.item.processing");
    case "thinking":
      return t("agentSession.item.thinking") || t("team.chat.item.thinking");
    case "tool":
      return t("agentSession.item.tool") || t("team.chat.item.tool");
    case "task":
      return t("agentSession.item.task") || t("team.chat.item.task");
    case "notice":
      return t("agentSession.item.notice") || t("team.chat.item.notice");
    case "final_result":
      return t("agentSession.item.result") || t("team.chat.item.result");
    case "cancelled":
      return t("agentSession.item.cancelled") || t("team.chat.item.cancelled");
    case "error":
      return t("agentSession.item.error") || t("team.chat.item.error");
  }
}

function getItemStatusText(
  item: AgentSessionItemView,
  t: ReturnType<typeof useI18n>["t"],
): string {
  if (item.kind === "processing") {
    return (
      t("agentSession.item.processingActive") ||
      t("team.chat.item.processingActive")
    );
  }
  if (item.kind === "tool") {
    return (
      t("agentSession.item.toolActivity") ||
      t("team.chat.item.toolActivity")
    );
  }
  if (item.kind === "task") {
    return (
      t("agentSession.item.taskActivity") ||
      t("team.chat.item.taskActivity")
    );
  }
  if (item.kind === "error") {
    return item.code || t("agentSession.item.error") || t("team.chat.item.error");
  }
  return t("agentSession.item.noText") || t("team.chat.item.noText");
}

function getItemIcon(kind: AgentSessionItemKind) {
  switch (kind) {
    case "user_message":
      return <MessageSquare size={15} />;
    case "assistant_text":
      return <Sparkles size={15} />;
    case "processing":
      return <LoaderCircle size={15} />;
    case "thinking":
      return <MoreHorizontal size={15} />;
    case "tool":
      return <Wrench size={15} />;
    case "task":
      return <FileText size={15} />;
    case "notice":
      return <Activity size={15} />;
    case "final_result":
      return <CheckCircle2 size={15} />;
    case "cancelled":
      return <XCircle size={15} />;
    case "error":
      return <CircleAlert size={15} />;
    default:
      return <Bot size={15} />;
  }
}
