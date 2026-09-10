import {
  ChevronDown,
  ChevronRight,
  LoaderCircle,
  Sparkles,
} from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type { AgentSessionItemView } from "../../types/agentSession";

export interface AgentSessionThinkingProps {
  item: AgentSessionItemView;
  expanded: boolean;
  onToggle: () => void;
  testIdPrefix?: string;
}

export function AgentSessionThinking({
  expanded,
  item,
  onToggle,
  testIdPrefix = "agent-session",
}: AgentSessionThinkingProps) {
  const { t } = useI18n();

  const isRunning = item.state === "streaming" || item.state === "pending";
  const title =
    item.kind === "processing"
      ? t("agentSession.item.processing") || "处理中"
      : t("agentSession.thinking.title") || "思考过程";
  const text = item.text || item.status || "";
  const isProcessingOnly = item.kind === "processing" && !item.text;

  return (
    <div
      className="rounded-xl border border-theme-card-border/50 bg-theme-control/15 text-body-sm shadow-sm"
      data-testid={`${testIdPrefix}-thinking-${item.id}`}
    >
      <button
        aria-expanded={expanded}
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-on-surface-variant outline-none transition-colors hover:bg-theme-control/25 focus-visible:ring-2 focus-visible:ring-primary-strong/45"
        data-testid={`${testIdPrefix}-thinking-toggle-${item.id}`}
        disabled={isProcessingOnly}
        onClick={onToggle}
        type="button"
      >
        <span className="shrink-0 text-primary">
          {isRunning ? (
            <LoaderCircle className="animate-spin" size={14} />
          ) : (
            <Sparkles size={14} />
          )}
        </span>
        <span className="truncate font-medium">{title}</span>
        <span className="text-caption text-outline">{item.state}</span>
        {!isProcessingOnly ? (
          <span className="ml-auto text-outline">
            {expanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
          </span>
        ) : null}
      </button>

      {expanded && !isProcessingOnly && text ? (
        <div
          className="border-t border-theme-card-border/30 px-3.5 py-2.5 text-on-surface-variant"
          data-testid={`${testIdPrefix}-thinking-content-${item.id}`}
        >
          <p className="whitespace-pre-wrap break-words text-caption leading-relaxed">
            {text}
          </p>
        </div>
      ) : null}
    </div>
  );
}
