import { Bot, Check, Copy } from "lucide-react";
import { useState } from "react";
import { MarkdownContent } from "../conversations/ConversationMarkdown";
import { useI18n } from "../../i18n/I18nProvider";
import type { AgentSessionItemView } from "../../types/agentSession";

export interface AgentSessionAssistantTextProps {
  item: AgentSessionItemView;
  testIdPrefix?: string;
}

export function AgentSessionAssistantText({
  item,
  testIdPrefix = "agent-session",
}: AgentSessionAssistantTextProps) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);

  const text = item.text || "";

  const handleCopy = async () => {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Ignore clipboard write failure
    }
  };

  return (
    <div
      className="group relative flex flex-col gap-1.5 py-1 text-on-surface"
      data-testid={`${testIdPrefix}-assistant-text-${item.id}`}
    >
      <div className="flex items-center gap-2 text-caption text-outline">
        <span className="flex items-center gap-1 font-medium text-primary">
          <Bot size={14} />
          <span>{t("agentSession.item.assistant") || "Agent"}</span>
        </span>
        {item.state === "streaming" ? (
          <span className="inline-flex items-center gap-1 text-caption text-primary">
            <span className="size-1.5 animate-ping rounded-full bg-primary" />
            <span>{item.state}</span>
          </span>
        ) : null}

        {text ? (
          <button
            aria-label="Copy message"
            className="ml-auto opacity-0 transition-opacity hover:text-on-surface group-hover:opacity-100 focus-visible:opacity-100"
            onClick={handleCopy}
            type="button"
          >
            {copied ? <Check size={13} className="text-status-create" /> : <Copy size={13} />}
          </button>
        ) : null}
      </div>

      <div className="prose prose-sm max-w-none text-body-sm leading-relaxed text-on-surface">
        {text ? (
          <MarkdownContent value={text} />
        ) : (
          <p className="italic text-outline">
            {t("agentSession.item.noText") || "该活动没有可显示正文。"}
          </p>
        )}
      </div>

      {item.truncation ? (
        <div
          className="mt-1 rounded border border-status-conflict/30 bg-status-conflict/10 px-2 py-1 text-caption text-status-conflict"
          data-testid={`${testIdPrefix}-assistant-text-truncation-${item.id}`}
        >
          <span>{t("agentSession.truncated") || "已截断"}: </span>
          <span>
            {t("agentSession.truncatedDetail", {
              retained: String(item.truncation.retainedBytes),
              original: String(item.truncation.originalBytes),
            }) || `保留 ${item.truncation.retainedBytes} / 原始 ${item.truncation.originalBytes} 字节`}
          </span>
        </div>
      ) : null}
    </div>
  );
}
