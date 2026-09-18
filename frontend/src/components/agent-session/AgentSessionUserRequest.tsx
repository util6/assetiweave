import { User } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type { AgentSessionItemView } from "../../types/agentSession";

export interface AgentSessionUserRequestProps {
  item: AgentSessionItemView;
  testIdPrefix?: string;
}

export function AgentSessionUserRequest({
  item,
  testIdPrefix = "agent-session",
}: AgentSessionUserRequestProps) {
  const { t } = useI18n();

  const isMemoryJob =
    item.id.includes("memory") ||
    (item.text && item.text.startsWith("[Memory]"));
  const sourceBadge = isMemoryJob ? (
    <span className="rounded bg-theme-control/60 px-1.5 py-0.5 text-label-caps uppercase text-primary">
      {t("agentSession.source.memory") || "Memory Agent 输入"}
    </span>
  ) : null;

  return (
    <div
      className="ml-auto flex max-w-[85%] flex-col items-end gap-1"
      data-testid={`${testIdPrefix}-user-request-${item.id}`}
    >
      <div className="flex items-center gap-1.5 text-caption text-outline">
        {sourceBadge}
        <span>{t("agentSession.item.user") || "用户"}</span>
        <span className="text-caption text-outline">{item.state}</span>
        <User size={13} />
      </div>

      <div className="rounded-2xl rounded-tr-sm border border-theme-nav-active-border/30 bg-theme-nav-active/15 px-4 py-2.5 text-body-sm text-on-surface shadow-sm">
        <p className="whitespace-pre-wrap break-words">{item.text || ""}</p>

        {item.code ? (
          <p className="mt-1 break-words text-caption text-status-remove">
            {item.code}
          </p>
        ) : null}

        {item.truncation ? (
          <div
            className="mt-2 rounded border border-status-conflict/30 bg-status-conflict/10 px-2 py-1 text-caption text-status-conflict"
            data-testid={`${testIdPrefix}-user-request-truncation-${item.id}`}
          >
            <span>{t("agentSession.truncated") || "已截断"}: </span>
            <span>
              {t("agentSession.truncatedDetail", {
                retained: String(item.truncation.retainedBytes),
                original: String(item.truncation.originalBytes),
              }) ||
                `保留 ${item.truncation.retainedBytes} / 原始 ${item.truncation.originalBytes} 字节`}
            </span>
          </div>
        ) : null}
      </div>
    </div>
  );
}
