import { X } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type { TranslationKey, TranslationParams } from "../../i18n/messages";

export type NotificationTone = "success" | "info" | "warning" | "error";

export interface NotificationMessage {
  id: string;
  tone: NotificationTone;
  message?: string;
  messageKey?: TranslationKey;
  messageParams?: TranslationParams;
}

const toneClass: Record<NotificationTone, string> = {
  success: "border-status-create/50 bg-status-create/12 text-status-create",
  info: "border-status-update/50 bg-status-update/12 text-status-update",
  warning: "border-status-conflict/50 bg-status-conflict/12 text-status-conflict",
  error: "border-status-remove/50 bg-status-remove/12 text-status-remove",
};

export function NotificationBanner({
  notification,
  onDismiss,
}: {
  notification: NotificationMessage | null;
  onDismiss: (id: string) => void;
}) {
  const { t } = useI18n();

  if (!notification) {
    return <div className="h-0 shrink-0" aria-hidden="true" />;
  }

  const message = notification.messageKey ? t(notification.messageKey, notification.messageParams) : (notification.message ?? "");

  return (
    <section className="shrink-0 px-8 py-3" aria-live="polite" aria-label={t("notification.aria")}>
      <div className={`flex min-h-12 items-center justify-between rounded-xl border px-4 py-2.5 ${toneClass[notification.tone]}`}>
        <p className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-body-md font-medium">{message}</p>
        <button
          className="ml-4 grid size-8 shrink-0 place-items-center rounded-lg transition-colors hover:bg-white/10"
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
