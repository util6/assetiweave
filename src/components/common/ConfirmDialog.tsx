import { AlertTriangle, X } from "lucide-react";
import type { ReactNode } from "react";
import { DialogFrame } from "../foundation/DialogFrame";
import { useI18n } from "../../i18n/I18nProvider";
import { cn } from "../../lib/utils";
import { iconButtonRecipe } from "../../theme/recipes";
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
      className="flex max-h-[92vh] max-w-xl flex-col"
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
      headerActions={
        <button
          aria-label={t("common.close")}
          className={cn(iconButtonRecipe({ size: "sm" }))}
          disabled={busy}
          onClick={onClose}
          title={t("common.close")}
          type="button"
        >
          <X size={17} />
        </button>
      }
      icon={<AlertTriangle size={18} />}
      iconClassName={
        danger
          ? "border-status-remove/35 bg-status-remove/15 text-status-remove"
          : "border-status-update/30 bg-status-update/15 text-status-update"
      }
      onBackdropClick={busy ? undefined : onClose}
      title={title}
    >
      <p className="text-body-sm leading-6 text-on-surface-variant">{message}</p>
      {children}
    </DialogFrame>
  );
}
