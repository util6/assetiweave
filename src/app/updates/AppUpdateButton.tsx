import { AlertCircle, CheckCircle2, DownloadCloud, RefreshCw } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import { cn } from "../../lib/utils";
import { useAppUpdater, type AppUpdateStatus } from "./AppUpdateProvider";

const attentionStatuses = new Set<AppUpdateStatus>(["available", "ready", "error"]);

export function AppUpdateButton() {
  const { t } = useI18n();
  const { checkForUpdates, openDialog, state } = useAppUpdater();

  if (!state.supported) {
    return null;
  }

  const busy = state.status === "checking" || state.status === "downloading" || state.status === "installing";
  const label = getUpdateButtonLabel(state.status, t);
  const Icon = getUpdateButtonIcon(state.status);

  function handleClick() {
    if (state.status === "idle" || state.status === "upToDate") {
      void checkForUpdates("manual");
      return;
    }

    openDialog();
  }

  return (
    <button
      aria-label={label}
      className={cn(
        "relative grid size-9 place-items-center rounded-xl border border-theme-control-border bg-theme-control/95 text-theme-control-fg shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)] transition-colors hover:bg-theme-control-hover hover:text-on-surface",
        attentionStatuses.has(state.status) && "border-status-update/45 bg-status-update/15 text-status-update",
        state.status === "error" && "border-status-remove/45 bg-status-remove/15 text-status-remove",
      )}
      onClick={handleClick}
      title={label}
      type="button"
    >
      <Icon className={busy ? "animate-spin" : undefined} size={18} />
      {attentionStatuses.has(state.status) && (
        <span className="absolute right-1.5 top-1.5 size-2 rounded-full bg-current shadow-[0_0_0_2px_rgb(var(--theme-toolbar))]" />
      )}
    </button>
  );
}

function getUpdateButtonIcon(status: AppUpdateStatus) {
  if (status === "available") {
    return DownloadCloud;
  }
  if (status === "ready") {
    return CheckCircle2;
  }
  if (status === "error") {
    return AlertCircle;
  }
  return RefreshCw;
}

function getUpdateButtonLabel(status: AppUpdateStatus, t: (key: "update.button.check" | "update.button.checking" | "update.button.available" | "update.button.downloading" | "update.button.installing" | "update.button.ready" | "update.button.error") => string) {
  if (status === "checking") {
    return t("update.button.checking");
  }
  if (status === "available") {
    return t("update.button.available");
  }
  if (status === "downloading") {
    return t("update.button.downloading");
  }
  if (status === "installing") {
    return t("update.button.installing");
  }
  if (status === "ready") {
    return t("update.button.ready");
  }
  if (status === "error") {
    return t("update.button.error");
  }
  return t("update.button.check");
}
