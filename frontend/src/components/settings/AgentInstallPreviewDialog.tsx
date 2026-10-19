import { AlertTriangle, Download, PackageCheck } from "lucide-react";
import { useI18n, type Translator } from "../../i18n/I18nProvider";
import type { AgentCatalogItem } from "./agentCatalog";
import type {
  AgentDistributionCandidate,
  AgentInstallPreview,
} from "../../services/agentRuntime";
import { Badge } from "../foundation/Badge";
import { DialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";

export function AgentInstallPreviewDialog({
  agent,
  busy = false,
  onClose,
  onConfirm,
  onSelectDistribution,
  preview,
}: {
  agent: AgentCatalogItem;
  busy?: boolean;
  onClose: () => void;
  onConfirm: () => void;
  onSelectDistribution: (distributionId: string) => void;
  preview: AgentInstallPreview;
}) {
  const { t } = useI18n();
  const candidates = [
    preview.selectedDistribution,
    ...preview.alternatives,
  ].filter(
    (candidate, index, all) =>
      all.findIndex(
        (item) => item.distributionId === candidate.distributionId,
      ) === index,
  );

  return (
    <DialogFrame
      closeLabel={t("common.close")}
      contentClassName="grid gap-4"
      description={t("settings.agents.installPreviewDescription")}
      footer={
        <div className="flex w-full justify-end gap-2">
          <Button
            disabled={busy}
            onClick={onClose}
            type="button"
            variant="outline"
          >
            {t("common.cancel")}
          </Button>
          <Button
            disabled={busy || preview.conflicts.length > 0}
            onClick={onConfirm}
            type="button"
          >
            {busy
              ? t("settings.agents.installing")
              : t("settings.agents.confirmInstall")}
          </Button>
        </div>
      }
      icon={<PackageCheck size={18} />}
      onClose={onClose}
      size="lg"
      title={`${actionLabel(preview.action, t)} · ${agent.name}`}
    >
      <div className="grid gap-3 text-body-sm">
        <div className="flex flex-wrap items-center gap-2">
          {preview.action === "update" && preview.currentInstallation ? (
            <div className="flex flex-wrap items-center gap-1.5">
              <Badge tone="neutral">
                {t("settings.agents.currentVersion")} v
                {preview.currentInstallation.version}
              </Badge>
              <span
                aria-hidden="true"
                className="select-none text-body-xs font-semibold text-outline"
              >
                ➔
              </span>
              <Badge tone="primary">
                {t("settings.agents.upgradeTo")} v{preview.targetVersion}
              </Badge>
            </div>
          ) : preview.action === "reinstall" && preview.currentInstallation ? (
            <div className="flex flex-wrap items-center gap-1.5">
              <Badge tone="neutral">
                {t("settings.agents.currentVersion")} v
                {preview.currentInstallation.version}
              </Badge>
              <Badge tone="primary">
                {t("settings.agents.reinstallVersion")} v{preview.targetVersion}
              </Badge>
            </div>
          ) : (
            <Badge tone="primary">
              {t("settings.agents.installVersion")} v{preview.targetVersion}
            </Badge>
          )}
          <Badge tone="neutral">{preview.ownership}</Badge>
          {preview.downloadSize !== null ? (
            <Badge tone="neutral">
              <Download size={13} /> {formatBytes(preview.downloadSize)}
            </Badge>
          ) : null}
        </div>

        <section
          aria-label={t("settings.agents.distributionOptions")}
          className="grid gap-2"
        >
          <p className="text-label-caps uppercase text-outline">
            {t("settings.agents.distributionOptions")}
          </p>
          {candidates.map((candidate) => (
            <DistributionOption
              candidate={candidate}
              key={candidate.distributionId}
              onSelect={() => onSelectDistribution(candidate.distributionId)}
              selected={
                candidate.distributionId ===
                preview.selectedDistribution.distributionId
              }
              t={t}
            />
          ))}
        </section>

        {preview.runtimeRequirements.length > 0 ? (
          <p className="rounded-xl border border-theme-card-border bg-theme-control/55 px-3 py-2 text-on-surface-variant">
            {preview.runtimeRequirements.join(" · ")}
          </p>
        ) : null}
        {preview.conflicts.length > 0 ? (
          <p className="flex gap-2 rounded-xl border border-status-remove/35 bg-status-remove/10 px-3 py-2 text-status-remove">
            <AlertTriangle className="mt-0.5 shrink-0" size={16} />
            <span>{preview.conflicts.join(" · ")}</span>
          </p>
        ) : null}
        {preview.warnings.length > 0 ? (
          <ul className="grid gap-1 rounded-xl border border-status-update/35 bg-status-update/10 px-3 py-2 text-on-surface-variant">
            {preview.warnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
        ) : null}
      </div>
    </DialogFrame>
  );
}

function DistributionOption({
  candidate,
  onSelect,
  selected,
  t,
}: {
  candidate: AgentDistributionCandidate;
  onSelect: () => void;
  selected: boolean;
  t: Translator;
}) {
  return (
    <button
      className={`flex items-center justify-between gap-3 rounded-xl border px-3 py-2.5 text-left transition-colors ${
        selected
          ? "border-theme-nav-active-border bg-theme-nav-active/12"
          : "border-theme-card-border bg-theme-control/45 hover:border-theme-nav-active-border/55"
      } ${candidate.selectable ? "" : "cursor-not-allowed opacity-55"}`}
      disabled={!candidate.selectable || selected}
      onClick={onSelect}
      type="button"
    >
      <span className="min-w-0">
        <span className="flex flex-wrap items-center gap-2 font-semibold text-on-surface">
          <span>{candidate.distributionType}</span>
          {candidate.recommended ? (
            <Badge tone="primary">{t("settings.agents.recommended")}</Badge>
          ) : null}
        </span>
        <span className="mt-1 flex flex-wrap items-center gap-1.5 text-body-xs text-on-surface-variant">
          {candidate.resolvedVersion ? (
            <span>
              {t("settings.agents.versionLabel")} v{candidate.resolvedVersion}
            </span>
          ) : null}
          {candidate.requiredRuntime ? (
            <>
              {candidate.resolvedVersion ? (
                <span aria-hidden="true">·</span>
              ) : null}
              <span>{candidate.requiredRuntime}</span>
            </>
          ) : null}
          {!candidate.resolvedVersion && !candidate.requiredRuntime ? (
            <span>
              {candidate.reasonCode ||
                t("settings.agents.distributionUnavailable")}
            </span>
          ) : null}
        </span>
      </span>
      {selected ? (
        <Badge tone="primary">{t("settings.agents.selected")}</Badge>
      ) : (
        <Badge tone={candidate.selectable ? "create" : "neutral"}>
          {candidate.selectable
            ? t("settings.agents.select")
            : t("settings.agents.unavailable")}
        </Badge>
      )}
    </button>
  );
}

function actionLabel(action: string, t: Translator) {
  if (action === "update") return t("settings.agents.update");
  if (action === "reinstall") return t("settings.agents.reinstall");
  return t("settings.agents.install");
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KiB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}
