import { RefreshCw } from "lucide-react";
import type { Translator } from "../../i18n/I18nProvider";
import type { ConversationSyncTaskSnapshot } from "../../services/conversations";
import {
  DEFAULT_CONVERSATION_CONTENT_CARD_COLORS,
  type ConversationContentCardColorSettings,
} from "../../store/settings/settingsSchema";

import { ToolbarCluster } from "../common/DataToolbar";
import { Switch } from "../ui/switch";
import type {
  ConversationContentType,
  ConversationContentVisibility,
} from "./ConversationContentCards";
import {
  conversationCardColor,
  conversationCardLabel,
} from "./ConversationContentCards";
import {
  isRedundantConversationCardKind,
  useConversationCardKindRegistry,
} from "./ConversationCardKindRegistry";

export function ConversationBackgroundTaskIndicator({
  task,
  t,
}: {
  task: ConversationSyncTaskSnapshot | null;
  t: Translator;
}) {
  if (task?.status !== "running" && task?.status !== "cancelling") {
    return null;
  }

  return (
    <section
      aria-live="polite"
      className="ui-task-indicator pointer-events-auto flex w-[min(24rem,calc(100vw-2.5rem))] items-center gap-3 rounded-2xl border px-4 py-3 text-on-surface"
      role="status"
    >
      <span className="ui-task-indicator-icon grid size-9 shrink-0 place-items-center rounded-xl text-status-update">
        <RefreshCw className="animate-spin" size={17} />
      </span>
      <span className="min-w-0">
        <span className="block text-body-sm font-semibold">
          {t(
            task.record_kind === "web"
              ? "conversation.sync.background.webTitle"
              : "conversation.sync.background.title",
          )}
        </span>
        <span className="mt-0.5 block text-code-sm text-on-surface-variant">
          {t("conversation.sync.background.description")}
        </span>
      </span>
    </section>
  );
}

const contentFilterOptions: ConversationContentType[] = [
  "answer",
  "tool",
  "command",
  "code",
  "result",
];

export function ConversationContentFilter({
  availableTypes,
  colors = DEFAULT_CONVERSATION_CONTENT_CARD_COLORS,
  onChange,
  t,
  visibility,
}: {
  availableTypes: readonly ConversationContentType[];
  colors?: ConversationContentCardColorSettings;
  onChange: (type: ConversationContentType, checked: boolean) => void;
  t: Translator;
  visibility: ConversationContentVisibility;
}) {
  const { definitions } = useConversationCardKindRegistry();
  const types = Array.from(new Set(availableTypes))
    .filter(
      (type) => !isRedundantConversationCardKind(type, definitions.get(type)),
    )
    .sort(compareConversationContentTypes);
  if (types.length === 0) return null;

  return (
    <ToolbarCluster
      ariaLabel={t("conversation.content.filterAria")}
      className="justify-start"
    >
      <span className="mr-1 whitespace-nowrap text-label-caps text-on-surface-muted">
        {t("conversation.content.visible")}
      </span>
      {types.map((type) => {
        const label =
          definitions.get(type)?.label ?? conversationCardLabel(type, t);
        return (
          <label
            className="inline-flex min-h-8 shrink-0 items-center gap-2 whitespace-nowrap rounded-xl px-1.5 text-body-sm text-on-surface-variant transition-[background-color,color] duration-200 hover:bg-theme-control-hover/70"
            key={type}
          >
            <span
              className="size-2 rounded-full"
              style={{ backgroundColor: conversationCardColor(type, colors) }}
            />
            <span className="whitespace-nowrap">{label}</span>
            <Switch
              aria-label={t("conversation.content.toggle", { type: label })}
              checked={visibility[type] ?? true}
              onCheckedChange={(checked) => onChange(type, checked)}
            />
          </label>
        );
      })}
    </ToolbarCluster>
  );
}

function compareConversationContentTypes(
  left: ConversationContentType,
  right: ConversationContentType,
) {
  const leftIndex = contentFilterOptions.indexOf(left);
  const rightIndex = contentFilterOptions.indexOf(right);
  if (leftIndex >= 0 && rightIndex >= 0) return leftIndex - rightIndex;
  if (leftIndex >= 0) return -1;
  if (rightIndex >= 0) return 1;
  return 0;
}
