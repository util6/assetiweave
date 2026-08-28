import { AlertTriangle, CheckCircle2, Info, X, XCircle } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type { TranslationKey, TranslationParams } from "../../i18n/messages";
import { iconButtonRecipe } from "../../theme/recipes";

export type NotificationTone = "success" | "info" | "warning" | "error";

export interface NotificationMessage {
  id: string;
  tone: NotificationTone;
  message?: string;
  messageKey?: TranslationKey;
  messageParams?: TranslationParams;
}

const toneClass: Record<NotificationTone, string> = {
  success: "border-status-create/50 text-status-create",
  info: "border-status-update/50 text-status-update",
  warning: "border-status-conflict/50 text-status-conflict",
  error: "border-status-remove/50 text-status-remove",
};

const toneIcon = {
  success: CheckCircle2,
  info: Info,
  warning: AlertTriangle,
  error: XCircle,
} satisfies Record<NotificationTone, typeof CheckCircle2>;

export function NotificationBanner({
  notification,
  onDismiss,
}: {
  notification: NotificationMessage | null;
  onDismiss: (id: string) => void;
}) {
  const { t } = useI18n();

  if (!notification) {
    return null;
  }

  const message = notification.messageKey ? t(notification.messageKey, notification.messageParams) : (notification.message ?? "");
  const Icon = toneIcon[notification.tone];

  return (
    <section
      className="aurora-notification-shell pointer-events-none absolute inset-x-0 top-[calc(var(--app-toolbar-top)-var(--app-window-titlebar-height))] z-30 flex justify-end px-[var(--app-page-x)] py-3"
      aria-live={notification.tone === "error" || notification.tone === "warning" ? "assertive" : "polite"}
      aria-label={t("notification.aria")}
    >
      <div
        aria-atomic="true"
        className={`aurora-notification pointer-events-auto flex w-full max-w-[min(42rem,100%)] items-start gap-3 rounded-2xl border px-3.5 py-2.5 ${toneClass[notification.tone]}`}
        data-tone={notification.tone}
        role={notification.tone === "error" || notification.tone === "warning" ? "alert" : "status"}
      >
        <span className="grid size-8 shrink-0 place-items-center rounded-xl bg-current/10" aria-hidden="true">
          <Icon size={17} />
        </span>
        <p className="max-h-[min(40vh,18rem)] min-w-0 flex-1 overflow-y-auto whitespace-pre-wrap break-words text-body-md font-medium leading-relaxed">
          {message}
        </p>
        <button
          className={iconButtonRecipe({ className: "ml-4 shrink-0" })}
          onClick={() => onDismiss(notification.id)}
          aria-label={t("notification.close")}
          type="button"
        >
          <X size={17} />
        </button>
      </div>
    </section>
  );
}
