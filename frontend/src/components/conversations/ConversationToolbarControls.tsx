import type { Translator } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import { Switch } from "../ui/switch";
import type {
  ConversationContentType,
  ConversationContentVisibility,
} from "./ConversationContentCards";

export type ConversationSyncPhase =
  | "preparing"
  | "importing"
  | "refreshing"
  | "completed"
  | "failed";

export interface ConversationSyncProgressState {
  phase: ConversationSyncPhase;
  sourceLabel: string;
  failedStep?: 1 | 2 | 3;
}

const contentFilterOptions: Array<{
  accentClass: string;
  type: ConversationContentType;
}> = [
  { type: "answer", accentClass: "bg-primary" },
  { type: "tool", accentClass: "bg-status-update" },
  { type: "command", accentClass: "bg-status-conflict" },
  { type: "code", accentClass: "bg-primary-strong" },
  { type: "result", accentClass: "bg-status-create" },
];

export function ConversationContentFilter({
  onChange,
  t,
  visibility,
}: {
  onChange: (type: ConversationContentType, checked: boolean) => void;
  t: Translator;
  visibility: ConversationContentVisibility;
}) {
  return (
    <div
      aria-label={t("conversation.content.filterAria")}
      className="flex flex-wrap items-center justify-end gap-2"
      role="group"
    >
      <span className="text-label-caps text-on-surface-muted">
        {t("conversation.content.visible")}
      </span>
      {contentFilterOptions.map((option) => {
        const label = t(`conversation.content.${option.type}` as TranslationKey);
        return (
          <label
            className="inline-flex h-10 items-center gap-2 rounded-xl border border-theme-control-border bg-theme-control/75 px-2.5 text-body-sm text-on-surface-variant shadow-[var(--theme-shadow-control-inset)]"
            key={option.type}
          >
            <span className={`size-2 rounded-full ${option.accentClass}`} />
            <span>{label}</span>
            <Switch
              aria-label={t("conversation.content.toggle", { type: label })}
              checked={visibility[option.type]}
              onCheckedChange={(checked) => onChange(option.type, checked)}
            />
          </label>
        );
      })}
    </div>
  );
}

export function ConversationSyncProgress({
  state,
  t,
}: {
  state: ConversationSyncProgressState;
  t: Translator;
}) {
  const step = syncStep(state);
  const title = t(`conversation.sync.phase.${state.phase}` as TranslationKey);
  const description = t(`conversation.sync.description.${state.phase}` as TranslationKey);
  const failed = state.phase === "failed";
  const completed = state.phase === "completed";

  return (
    <section
      aria-live="polite"
      className={`mt-4 rounded-xl border px-4 py-3 ${
        failed
          ? "border-status-remove/40 bg-status-remove/10"
          : completed
            ? "border-status-create/40 bg-status-create/10"
            : "border-status-update/35 bg-status-update/[0.08]"
      }`}
      role="status"
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className={`text-body-sm font-semibold ${failed ? "text-status-remove" : completed ? "text-status-create" : "text-on-surface"}`}>
            {title}
          </p>
          <p className="mt-1 text-body-sm text-on-surface-variant">{description}</p>
        </div>
        <div className="shrink-0 text-right">
          <p className="text-label-caps text-on-surface-variant">
            {t("conversation.sync.stage", { current: step, total: 4 })}
          </p>
          <p className="mt-1 text-code-sm text-on-surface-muted">
            {t("conversation.sync.scope", { source: state.sourceLabel })}
          </p>
        </div>
      </div>
      <div
        aria-label={title}
        aria-valuemax={4}
        aria-valuemin={1}
        aria-valuenow={step}
        aria-valuetext={t("conversation.sync.stage", { current: step, total: 4 })}
        className="mt-3 h-2 overflow-hidden rounded-full bg-theme-control"
        role="progressbar"
      >
        <div
          className={`h-full rounded-full transition-[width] duration-500 ${
            failed
              ? "bg-status-remove"
              : completed
                ? "bg-status-create"
                : "animate-pulse bg-status-update"
          }`}
          style={{ width: `${step * 25}%` }}
        />
      </div>
    </section>
  );
}

function syncStep(state: ConversationSyncProgressState) {
  if (state.phase === "preparing") return 1;
  if (state.phase === "importing") return 2;
  if (state.phase === "refreshing") return 3;
  if (state.phase === "completed") return 4;
  return state.failedStep ?? 2;
}
