import { RefreshCw, X } from "lucide-react";
import type { Translator } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import type { ConversationSyncTaskSnapshot } from "../../services/conversations";
import type { ConversationRecordKind } from "../../types";
import {
  DEFAULT_CONVERSATION_CONTENT_CARD_COLORS,
  type ConversationContentCardColorSettings,
} from "../../store/settings/AppSettingsProvider";
import { ToolbarCluster } from "../common/DataToolbar";
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
  advice?: string;
  phase: ConversationSyncPhase;
  sourceLabel: string;
  failedStep?: 1 | 2 | 3;
  summary?: string;
  taskId?: string;
}

export function ConversationBackgroundTaskIndicator({
  task,
  t,
}: {
  task: ConversationSyncTaskSnapshot | null;
  t: Translator;
}) {
  if (task?.status !== "running") {
    return null;
  }

  return (
    <section
      aria-live="polite"
      className="pointer-events-auto flex w-[min(24rem,calc(100vw-2.5rem))] items-center gap-3 rounded-xl border border-status-update/40 bg-theme-card/95 px-4 py-3 text-on-surface shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.28)] backdrop-blur"
      role="status"
    >
      <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-status-update/15 text-status-update">
        <RefreshCw className="animate-spin" size={17} />
      </span>
      <span className="min-w-0">
        <span className="block text-body-sm font-semibold">
          {t("conversation.sync.background.title")}
        </span>
        <span className="mt-0.5 block text-code-sm text-on-surface-variant">
          {t("conversation.sync.background.description")}
        </span>
      </span>
    </section>
  );
}

const contentFilterOptions: ConversationContentType[] = ["answer", "tool", "command", "code", "result"];

export function ConversationContentFilter({
  colors = DEFAULT_CONVERSATION_CONTENT_CARD_COLORS,
  onChange,
  t,
  visibility,
}: {
  colors?: ConversationContentCardColorSettings;
  onChange: (type: ConversationContentType, checked: boolean) => void;
  t: Translator;
  visibility: ConversationContentVisibility;
}) {
  return (
    <ToolbarCluster ariaLabel={t("conversation.content.filterAria")} className="justify-start">
      <span className="mr-1 whitespace-nowrap text-label-caps text-on-surface-muted">
        {t("conversation.content.visible")}
      </span>
      {contentFilterOptions.map((type) => {
        const label = t(`conversation.content.${type}` as TranslationKey);
        return (
          <label
            className="inline-flex min-h-8 shrink-0 items-center gap-2 whitespace-nowrap rounded-lg px-1.5 text-body-sm text-on-surface-variant transition-colors hover:bg-theme-control-hover/70"
            key={type}
          >
            <span className="size-2 rounded-full" style={{ backgroundColor: colors[type] }} />
            <span className="whitespace-nowrap">{label}</span>
            <Switch
              aria-label={t("conversation.content.toggle", { type: label })}
              checked={visibility[type]}
              onCheckedChange={(checked) => onChange(type, checked)}
            />
          </label>
        );
      })}
    </ToolbarCluster>
  );
}

export function ConversationSyncProgress({
  onDismiss,
  recordKind = "session",
  state,
  t,
}: {
  onDismiss?: () => void;
  recordKind?: ConversationRecordKind;
  state: ConversationSyncProgressState;
  t: Translator;
}) {
  const step = syncStep(state);
  const title = t(syncPhaseKey(recordKind, state.phase));
  const description = t(syncDescriptionKey(recordKind, state.phase));
  const failed = state.phase === "failed";
  const completed = state.phase === "completed";
  const completedWithAdvice = completed && Boolean(state.advice);
  const scopeLabel = t(
    recordKind === "web"
      ? "conversation.sync.web.scope"
      : "conversation.sync.scope",
    { source: state.sourceLabel },
  );

  return (
    <section
      aria-live="polite"
      className={`mt-4 rounded-xl border px-4 py-3 ${
        failed
          ? "border-status-remove/40 bg-status-remove/10"
          : completedWithAdvice
            ? "border-status-update/40 bg-status-update/[0.08]"
            : completed
            ? "border-status-create/40 bg-status-create/10"
            : "border-status-update/35 bg-status-update/[0.08]"
      }`}
      role="status"
    >
      <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_auto] md:items-start">
        <div className="min-w-0">
          <p className={`text-body-sm font-semibold ${failed ? "text-status-remove" : completedWithAdvice ? "text-status-update" : completed ? "text-status-create" : "text-on-surface"}`}>
            {title}
          </p>
          <p className="mt-1 max-w-3xl text-body-sm text-on-surface-variant">{description}</p>
          {state.summary ? (
            <p className="mt-2 max-w-4xl break-words text-body-sm text-on-surface">{state.summary}</p>
          ) : null}
          {state.advice ? (
            <p className="mt-2 max-w-4xl break-words text-body-sm font-medium text-status-update">
              {state.advice}
            </p>
          ) : null}
        </div>
        <div className="flex min-w-0 items-start justify-between gap-3 md:justify-end md:text-right">
          <div className="min-w-0">
            <p className="text-label-caps text-on-surface-variant">
              {t("conversation.sync.stage", { current: step, total: 4 })}
            </p>
            <p className="mt-1 max-w-full break-words text-left text-code-sm text-on-surface-muted md:max-w-72 md:text-right">
              {scopeLabel}
            </p>
          </div>
          {onDismiss ? (
            <button
              aria-label={t("conversation.sync.dismiss")}
              className="grid size-7 place-items-center rounded-lg text-on-surface-muted transition-colors hover:bg-theme-control-hover hover:text-on-surface"
              onClick={onDismiss}
              title={t("conversation.sync.dismiss")}
              type="button"
            >
              <X size={15} />
            </button>
          ) : null}
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
              : completedWithAdvice
                ? "bg-status-update"
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

function syncPhaseKey(recordKind: ConversationRecordKind, phase: ConversationSyncPhase): TranslationKey {
  return (
    recordKind === "web"
      ? `conversation.sync.web.phase.${phase}`
      : `conversation.sync.phase.${phase}`
  ) as TranslationKey;
}

function syncDescriptionKey(recordKind: ConversationRecordKind, phase: ConversationSyncPhase): TranslationKey {
  return (
    recordKind === "web"
      ? `conversation.sync.web.description.${phase}`
      : `conversation.sync.description.${phase}`
  ) as TranslationKey;
}
