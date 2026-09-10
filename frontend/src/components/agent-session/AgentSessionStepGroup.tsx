import {
  CheckCircle2,
  CheckSquare,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  LoaderCircle,
  XCircle,
} from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type {
  AgentSessionItemView,
  AgentSessionStepGroupStatus,
} from "../../types/agentSession";
import { AgentSessionItem } from "./AgentSessionItem";

export interface AgentSessionStepGroupProps {
  groupId: string;
  items: AgentSessionItemView[];
  logicalCount: number;
  status: AgentSessionStepGroupStatus;
  expanded: boolean;
  onToggle: () => void;
  testIdPrefix?: string;
}

export function AgentSessionStepGroup({
  expanded,
  groupId,
  items,
  logicalCount,
  onToggle,
  status,
  testIdPrefix = "agent-session",
}: AgentSessionStepGroupProps) {
  const { t } = useI18n();

  const title = t("agentSession.viewSteps") || "查看步骤";
  const icon = getStatusIcon(status);

  return (
    <div
      className="rounded-xl border border-theme-card-border/60 bg-theme-card/45 shadow-sm"
      data-testid={`${testIdPrefix}-step-group-${groupId}`}
    >
      <button
        aria-expanded={expanded}
        className="flex w-full items-center gap-2.5 px-3.5 py-2.5 text-left text-body-sm font-medium text-on-surface outline-none transition-colors hover:bg-theme-control/25 focus-visible:ring-2 focus-visible:ring-primary-strong/45"
        data-testid={`${testIdPrefix}-step-group-toggle-${groupId}`}
        onClick={onToggle}
        type="button"
      >
        <span className="shrink-0 text-primary">{icon}</span>
        <span className="truncate">
          {title} · {logicalCount}
        </span>
        <span className="text-label-caps uppercase text-on-surface-variant/80">
          {status}
        </span>
        <span className="ml-auto text-outline">
          {expanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
        </span>
      </button>

      {expanded ? (
        <div
          className="border-t border-theme-card-border/40 px-3 py-2.5"
          data-testid={`${testIdPrefix}-step-group-content-${groupId}`}
        >
          <ol className="grid gap-2">
            {items.map((item) => (
              <AgentSessionItem
                item={item}
                key={item.id}
                testIdPrefix={testIdPrefix}
              />
            ))}
          </ol>
        </div>
      ) : null}
    </div>
  );
}

function getStatusIcon(status: AgentSessionStepGroupStatus) {
  switch (status) {
    case "failed":
      return <CircleAlert className="text-status-remove" size={15} />;
    case "running":
      return <LoaderCircle className="animate-spin text-primary" size={15} />;
    case "cancelled":
      return <XCircle className="text-status-conflict" size={15} />;
    case "succeeded":
      return <CheckCircle2 className="text-status-create" size={15} />;
    default:
      return <CheckSquare size={15} />;
  }
}
