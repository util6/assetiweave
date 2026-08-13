import { listen } from "@tauri-apps/api/event";
import { Database, Minimize2, Power } from "lucide-react";
import { useEffect, useState } from "react";
import { useI18n } from "../i18n/I18nProvider";
import { DialogFrame } from "../components/foundation/DialogFrame";
import { Button } from "../components/ui/button";
import { cancelAppClosePrompt, completeAppClose } from "../services/appLifecycle";
import { runWindowAction } from "../services/windowChrome";

const APP_CLOSE_REQUESTED_EVENT = "app-close-requested";

export function AppClosePrompt() {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const [backupDatabase, setBackupDatabase] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void listen(APP_CLOSE_REQUESTED_EVENT, () => {
      if (cancelled) {
        return;
      }
      setBusy(false);
      setBackupDatabase(true);
      setError("");
      setOpen(true);
    })
      .then((removeListener) => {
        if (cancelled) {
          removeListener();
        } else {
          unlisten = removeListener;
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  async function handleConfirmClose() {
    setBusy(true);
    setError("");
    try {
      await completeAppClose(backupDatabase);
      setBusy(false);
      setOpen(false);
    } catch (closeError) {
      setBusy(false);
      setError(errorMessage(closeError));
    }
  }

  async function handleDismiss() {
    if (busy) {
      return;
    }

    setBusy(true);
    setError("");
    try {
      await cancelAppClosePrompt();
      setBusy(false);
      setOpen(false);
    } catch (dismissError) {
      setBusy(false);
      setError(errorMessage(dismissError));
    }
  }

  async function handleMinimize() {
    setBusy(true);
    setError("");
    try {
      await cancelAppClosePrompt();
      await runWindowAction("minimize");
      setBusy(false);
      setOpen(false);
    } catch (minimizeError) {
      setBusy(false);
      setError(errorMessage(minimizeError));
    }
  }

  if (!open) {
    return null;
  }

  return (
    <DialogFrame
      busy={busy}
      contentClassName="grid gap-4"
      footer={
        <>
          <Button disabled={busy} onClick={() => void handleMinimize()} type="button" variant="outline">
            <Minimize2 size={16} />
            {t("app.close.minimize")}
          </Button>
          <Button disabled={busy} onClick={() => void handleConfirmClose()} type="button">
            <Power size={16} />
            {t("app.close.confirm")}
          </Button>
        </>
      }
      icon={<Power size={18} />}
      iconClassName="border-status-update/30 bg-status-update/15 text-status-update"
      closeLabel={t("common.close")}
      onBackdropClick={() => void handleDismiss()}
      onClose={() => void handleDismiss()}
      size="sm"
      title={t("app.close.title")}
    >
      <p className="text-body-sm leading-6 text-on-surface-variant">{t("app.close.message")}</p>
      <label className="flex items-center gap-3 rounded-lg border border-theme-control-border bg-theme-control/60 px-3 py-3 text-body-sm text-on-surface">
        <input
          aria-label={t("app.close.backupDatabase")}
          checked={backupDatabase}
          className="size-4 accent-primary-strong"
          disabled={busy}
          onChange={(event) => setBackupDatabase(event.target.checked)}
          type="checkbox"
        />
        <span className="flex min-w-0 items-center gap-2">
          <Database aria-hidden="true" size={16} />
          {t("app.close.backupDatabase")}
        </span>
      </label>
      {error ? <p className="text-body-sm text-status-remove" role="alert">{error}</p> : null}
    </DialogFrame>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
