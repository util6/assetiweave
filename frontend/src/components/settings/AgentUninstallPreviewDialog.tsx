import { AlertTriangle, PackageX } from "lucide-react";
import { useState } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import type { AgentUninstallPreview } from "../../services/agentRuntime";
import type { AgentCatalogItem } from "./agentCatalog";
import { Badge } from "../foundation/Badge";
import { DialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";

export function AgentUninstallPreviewDialog({
  agent,
  busy = false,
  onClose,
  onConfirm,
  preview,
}: {
  agent: AgentCatalogItem;
  busy?: boolean;
  onClose: () => void;
  onConfirm: (clearCapabilityAssignments: string[]) => void;
  preview: AgentUninstallPreview;
}) {
  const { t } = useI18n();
  const [selectedAssignments, setSelectedAssignments] = useState(() => new Set<string>());
  const blockingConflicts = preview.conflicts.filter((conflict) => !conflict.startsWith("assignment:"));
  const canConfirm = blockingConflicts.length === 0
    && preview.capabilityAssignments.every((assignment) => selectedAssignments.has(assignment));

  return (
    <DialogFrame
      closeLabel={t("common.close")}
      contentClassName="grid gap-4"
      description={t("settings.agents.uninstallPreviewDescription")}
      footer={(
        <div className="flex w-full justify-end gap-2">
          <Button disabled={busy} onClick={onClose} type="button" variant="outline">
            {t("common.cancel")}
          </Button>
          <Button
            disabled={busy || !canConfirm}
            onClick={() => onConfirm([...selectedAssignments])}
            type="button"
            variant="destructive"
          >
            {busy ? t("settings.agents.installing") : t("settings.agents.confirmUninstall")}
          </Button>
        </div>
      )}
      icon={<PackageX size={18} />}
      onClose={onClose}
      size="lg"
      title={`${t("settings.agents.uninstall")} · ${agent.name}`}
    >
      <div className="grid gap-3 text-body-sm">
        <div className="flex flex-wrap items-center gap-2">
          <Badge tone="neutral">v{preview.currentInstallation.version}</Badge>
          <Badge tone="neutral">{preview.ownership}</Badge>
          <Badge tone={preview.currentInstallation.enabled ? "create" : "neutral"}>
            {preview.currentInstallation.enabled ? t("settings.agents.enabled") : t("settings.agents.disabled")}
          </Badge>
        </div>

        <div className="grid gap-1">
          <span className="text-label-caps uppercase text-outline">{t("settings.agents.installPath")}</span>
          <code className="rounded-lg border border-theme-control-border bg-theme-control px-3 py-2 text-code-sm text-on-surface">
            {preview.targetPath || t("settings.agents.externalRuntime")}
          </code>
        </div>

        {preview.capabilityAssignments.length > 0 ? (
          <section className="grid gap-2" aria-label={t("settings.agents.assignmentsToClear")}>
            <p className="text-label-caps uppercase text-outline">{t("settings.agents.assignmentsToClear")}</p>
            <p className="text-body-sm text-on-surface-variant">{t("settings.agents.assignmentsToClearDescription")}</p>
            {preview.capabilityAssignments.map((assignment) => (
              <label className="flex items-center gap-2 rounded-lg border border-theme-card-border bg-theme-control/55 px-3 py-2" key={assignment}>
                <input
                  checked={selectedAssignments.has(assignment)}
                  disabled={busy}
                  onChange={(event) => {
                    setSelectedAssignments((current) => {
                      const next = new Set(current);
                      if (event.target.checked) next.add(assignment);
                      else next.delete(assignment);
                      return next;
                    });
                  }}
                  type="checkbox"
                />
                <span className="text-on-surface">{assignment}</span>
              </label>
            ))}
          </section>
        ) : null}

        {blockingConflicts.length > 0 ? (
          <p className="flex gap-2 rounded-xl border border-status-remove/35 bg-status-remove/10 px-3 py-2 text-status-remove">
            <AlertTriangle className="mt-0.5 shrink-0" size={16} />
            <span>{blockingConflicts.join(" · ")}</span>
          </p>
        ) : null}
        {preview.warnings.length > 0 ? (
          <ul className="grid gap-1 rounded-xl border border-status-update/35 bg-status-update/10 px-3 py-2 text-on-surface-variant">
            {preview.warnings.map((warning) => <li key={warning}>{warning}</li>)}
          </ul>
        ) : null}
      </div>
    </DialogFrame>
  );
}
