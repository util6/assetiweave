import { AlertTriangle, CheckCircle2, DownloadCloud, ExternalLink, RefreshCw, RotateCw } from "lucide-react";
import { DialogFrame } from "../../components/foundation/DialogFrame";
import { Button } from "../../components/ui/button";
import { useI18n } from "../../i18n/I18nProvider";
import { cn } from "../../lib/utils";
import { useAppUpdater } from "./AppUpdateProvider";

export function AppUpdateDialog() {
  const { locale, t } = useI18n();
  const { checkForUpdates, closeDialog, dialogMode, dialogOpen, downloadAndInstall, openReleases, restartApp, state } = useAppUpdater();

  if (!dialogOpen || !state.supported) {
    return null;
  }

  const introMode = dialogMode === "intro";
  const busy = !introMode && (state.status === "checking" || state.status === "downloading" || state.status === "installing");
  const canClose = introMode || state.status !== "installing";
  const hasUpdate = Boolean(state.info);
  const retryText = state.retryAttempt && state.retryTotal
    ? t("update.retrying", { attempt: state.retryAttempt, total: state.retryTotal })
    : "";

  function handleClose() {
    if (canClose) {
      closeDialog();
    }
  }

  return (
    <DialogFrame
      busy={!canClose}
      closeLabel={t("common.close")}
      contentClassName="space-y-4"
      description={introMode ? t("update.intro.description") : t("update.dialog.description")}
      footer={
        introMode ? (
          <Button onClick={handleClose} type="button">
            {t("update.action.acknowledge")}
          </Button>
        ) : (
          <>
          <Button disabled={!canClose} onClick={handleClose} type="button" variant="outline">
            {state.status === "ready" ? t("update.action.later") : t("common.close")}
          </Button>
          {state.status === "upToDate" ? (
            <Button onClick={() => void checkForUpdates("manual")} type="button">
              <RefreshCw size={16} />
              {t("update.action.checkAgain")}
            </Button>
          ) : state.status === "ready" ? (
            <Button onClick={() => void restartApp()} type="button">
              <RotateCw size={16} />
              {t("update.action.restart")}
            </Button>
          ) : state.status === "available" || state.status === "error" ? (
            <>
              {state.status === "error" && (
                <Button onClick={() => void openReleases()} type="button" variant="outline">
                  <ExternalLink size={16} />
                  {t("update.action.releasePage")}
                </Button>
              )}
              <Button onClick={() => void (hasUpdate ? downloadAndInstall() : checkForUpdates("manual"))} type="button">
                {state.status === "error" && !hasUpdate ? <RefreshCw size={16} /> : <DownloadCloud size={16} />}
                {state.status === "error" && !hasUpdate ? t("update.action.retry") : t("update.action.install")}
              </Button>
            </>
          ) : (
            <Button disabled type="button">
              <RefreshCw className="animate-spin" size={16} />
              {state.status === "downloading" ? t("update.action.downloading") : state.status === "installing" ? t("update.action.installing") : t("update.action.checking")}
            </Button>
          )}
          </>
        )
      }
      icon={introMode ? <CheckCircle2 size={20} /> : getDialogIcon(state.status)}
      iconClassName={cn(
        "border-status-update/30 bg-status-update/15 text-status-update",
        (introMode || state.status === "ready") && "border-status-create/30 bg-status-create/15 text-status-create",
        !introMode && state.status === "error" && "border-status-remove/30 bg-status-remove/15 text-status-remove",
      )}
      onClose={handleClose}
      size="md"
      title={introMode ? t("update.intro.title") : t("update.dialog.title")}
    >
      {introMode ? (
        <CurrentVersionIntro currentVersion={state.currentVersion} />
      ) : (
        <div className="space-y-3">
        <div className="rounded-lg border border-theme-card-border bg-theme-card/75 p-3">
          <p className="text-body-md font-semibold text-on-surface">{getStatusTitle(state.status, t)}</p>
          <p className="mt-1 text-body-sm text-on-surface-variant">{getStatusDescription(state.status, t)}</p>
          {retryText && (
            <p className="mt-2 inline-flex items-center gap-2 text-body-sm text-status-update">
              <RefreshCw className="animate-spin" size={14} />
              {retryText}
            </p>
          )}
        </div>

        {state.info && (
          <div className="grid gap-2 rounded-lg border border-theme-card-border bg-theme-panel/70 p-3 text-body-sm">
            <div className="flex items-center justify-between gap-3">
              <span className="text-on-surface-variant">{t("update.currentVersion")}</span>
              <span className="font-semibold text-on-surface">v{state.info.currentVersion}</span>
            </div>
            <div className="flex items-center justify-between gap-3">
              <span className="text-on-surface-variant">{t("update.latestVersion")}</span>
              <span className="font-semibold text-status-update">v{state.info.version}</span>
            </div>
            {state.info.date && (
              <div className="flex items-center justify-between gap-3">
                <span className="text-on-surface-variant">{t("update.publishedAt")}</span>
                <span className="font-medium text-on-surface">{formatDate(state.info.date, locale)}</span>
              </div>
            )}
          </div>
        )}

        {(state.status === "downloading" || state.status === "installing" || state.status === "ready") && (
          <div className="space-y-2">
            <div className="h-2 overflow-hidden rounded-full bg-theme-control">
              <div className="h-full rounded-full bg-status-update transition-[width]" style={{ width: `${Math.max(0, Math.min(100, state.progress))}%` }} />
            </div>
            <div className="flex justify-between text-body-xs text-on-surface-variant">
              <span>{state.status === "installing" ? t("update.installing") : t("update.downloading")}</span>
              <span>{Math.round(state.progress)}%</span>
            </div>
          </div>
        )}

        {state.error && (
          <div className="rounded-lg border border-status-remove/30 bg-status-remove/10 p-3 text-body-sm text-status-remove">
            {state.error}
          </div>
        )}

        {state.info?.notes && (
          <div className="max-h-48 overflow-auto rounded-lg border border-theme-card-border bg-theme-panel/70 p-3">
            <h3 className="text-body-sm font-semibold text-on-surface">{t("update.releaseNotes")}</h3>
            <p className="mt-2 whitespace-pre-wrap text-body-sm leading-6 text-on-surface-variant">{state.info.notes}</p>
          </div>
        )}
      </div>
      )}
      {busy && <span className="sr-only">{t("common.loading")}</span>}
    </DialogFrame>
  );
}

function CurrentVersionIntro({ currentVersion }: { currentVersion?: string }) {
  const { t } = useI18n();
  const highlights = [
    t("update.intro.highlight.navigation"),
    t("update.intro.highlight.updater"),
    t("update.intro.highlight.backgroundTasks"),
  ];

  return (
    <div className="space-y-4">
      <div className="rounded-lg border border-theme-card-border bg-theme-card/75 p-3">
        <p className="text-body-md font-semibold text-on-surface">{t("update.intro.currentTitle")}</p>
        <p className="mt-1 text-body-sm text-on-surface-variant">{t("update.intro.currentDescription")}</p>
      </div>

      <div className="grid gap-2 rounded-lg border border-theme-card-border bg-theme-panel/70 p-3 text-body-sm">
        <div className="flex items-center justify-between gap-3">
          <span className="text-on-surface-variant">{t("update.currentVersion")}</span>
          <span className="font-semibold text-status-create">{currentVersion ? `v${currentVersion}` : t("common.none")}</span>
        </div>
      </div>

      <div className="rounded-lg border border-theme-card-border bg-theme-panel/70 p-3">
        <h3 className="text-body-sm font-semibold text-on-surface">{t("update.intro.highlights")}</h3>
        <ul className="mt-2 grid gap-2 text-body-sm leading-6 text-on-surface-variant">
          {highlights.map((highlight) => (
            <li className="flex gap-2" key={highlight}>
              <CheckCircle2 className="mt-0.5 shrink-0 text-status-create" size={15} />
              <span>{highlight}</span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

function getDialogIcon(status: string) {
  if (status === "ready" || status === "upToDate") {
    return <CheckCircle2 size={20} />;
  }
  if (status === "error") {
    return <AlertTriangle size={20} />;
  }
  if (status === "available") {
    return <DownloadCloud size={20} />;
  }
  return <RefreshCw className="animate-spin" size={20} />;
}

function getStatusTitle(status: string, t: (key: "update.status.checking" | "update.status.available" | "update.status.upToDate" | "update.status.downloading" | "update.status.installing" | "update.status.ready" | "update.status.error") => string) {
  if (status === "checking") return t("update.status.checking");
  if (status === "available") return t("update.status.available");
  if (status === "upToDate") return t("update.status.upToDate");
  if (status === "downloading") return t("update.status.downloading");
  if (status === "installing") return t("update.status.installing");
  if (status === "ready") return t("update.status.ready");
  if (status === "error") return t("update.status.error");
  return t("update.status.checking");
}

function getStatusDescription(status: string, t: (key: "update.description.checking" | "update.description.available" | "update.description.upToDate" | "update.description.downloading" | "update.description.installing" | "update.description.ready" | "update.description.error") => string) {
  if (status === "checking") return t("update.description.checking");
  if (status === "available") return t("update.description.available");
  if (status === "upToDate") return t("update.description.upToDate");
  if (status === "downloading") return t("update.description.downloading");
  if (status === "installing") return t("update.description.installing");
  if (status === "ready") return t("update.description.ready");
  if (status === "error") return t("update.description.error");
  return t("update.description.checking");
}

function formatDate(value: string, locale: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat(locale === "zh" ? "zh-CN" : "en", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}
