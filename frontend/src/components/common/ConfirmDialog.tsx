import { AlertTriangle } from "lucide-react";
import type { ReactNode } from "react";
import { DialogFrame } from "../foundation/DialogFrame";
import { useI18n } from "../../i18n/I18nProvider";
import { Button } from "../ui/button";

export function ConfirmDialog({
  busy,
  cancelLabel,
  children,
  confirmLabel,
  message,
  onClose,
  onConfirm,
  open,
  title,
  tone = "default",
}: {
  busy: boolean;
  cancelLabel?: string;
  children?: ReactNode;
  confirmLabel?: string;
  message: string;
  onClose: () => void;
  onConfirm: () => void;
  open: boolean;
  title: string;
  tone?: "default" | "danger";
}) {
  const { t } = useI18n();

  if (!open) {
    return null;
  }

  const danger = tone === "danger";

  return (
    <DialogFrame
      busy={busy}
      closeLabel={t("common.close")}
      contentClassName="grid gap-4"
      footer={
        <>
          <Button disabled={busy} onClick={onClose} type="button" variant="outline">
            {cancelLabel ?? t("common.cancel")}
          </Button>
          <Button disabled={busy} onClick={onConfirm} type="button" variant={danger ? "destructive" : "default"}>
            {confirmLabel ?? t("common.confirm")}
          </Button>
        </>
      }
      icon={<AlertTriangle size={18} />}
      iconClassName={
        danger
          ? "border-status-remove/35 bg-status-remove/15 text-status-remove"
          : "border-status-update/30 bg-status-update/15 text-status-update"
      }
      onClose={onClose}
      size="md"
      title={title}
    >
      <p className="text-body-sm leading-6 text-on-surface-variant">{message}</p>
      {children}
    </DialogFrame>
  );
}
