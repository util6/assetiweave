import { AlertTriangle, CheckCheck } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "../ui/button";
import { DialogFrame } from "../foundation/DialogFrame";
import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, SkillGroupExclusiveMountPreview } from "../../types";
import { AppShortcutIconForShortcut } from "../apps/AppShortcutIcon";
import type { GroupMountMode } from "./GroupExclusiveMountControls";

export function SkillGroupExclusiveMountDialog({
  busy,
  mode,
  onClose,
  onConfirm,
  preview,
  shortcut,
}: {
  busy: boolean;
  mode: GroupMountMode;
  onClose: () => void;
  onConfirm: () => void | Promise<void>;
  preview: SkillGroupExclusiveMountPreview | null;
  shortcut: AppShortcut | null;
}) {
  const { t } = useI18n();

  if (!preview || !shortcut) {
    return null;
  }

  const footer = (
    <div className="flex items-center justify-end gap-2">
      <Button disabled={busy} onClick={onClose} type="button" variant="outline">
        {t("group.dialog.cancel")}
      </Button>
      <Button disabled={busy} onClick={() => void onConfirm()} type="button">
        <CheckCheck size={16} />
        {t("group.exclusive.confirm")}
      </Button>
    </div>
  );

  return (
    <DialogFrame
      busy={busy}
      closeLabel={t("group.dialog.close")}
      footer={footer}
      icon={<CheckCheck size={18} />}
      iconClassName="border-status-conflict/25 bg-status-conflict/15 text-status-conflict"
      onClose={onClose}
      size="xl"
      title={t(mode === "exclusive" ? "group.exclusive.dialogTitle" : "group.exclusive.additiveDialogTitle")}
    >
      <div className="grid gap-4">
        <section className="grid gap-3 rounded-xl border border-theme-card-border bg-theme-card/65 p-3">
          <div className="flex items-center gap-2 text-label-caps uppercase text-outline">
            <CheckCheck size={15} />
            <span>{t("group.exclusive.summaryTitle")}</span>
          </div>
          <div className="grid grid-cols-2 gap-2 max-[720px]:grid-cols-1">
            <SummaryItem
              label={t("group.exclusive.targetProfile")}
              value={
                <span className="inline-flex min-w-0 items-center gap-2">
                  <AppShortcutIconForShortcut className="size-4 shrink-0" shortcut={shortcut} />
                  <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap">{shortcut.profileName}</span>
                </span>
              }
            />
            <SummaryItem label={t("group.exclusive.groupCount")} value={preview.group_ids.length} />
            <SummaryItem
              label={t(mode === "exclusive" ? "group.exclusive.finalSkillCount" : "group.exclusive.selectedSkillCount")}
              value={preview.selected_skill_ids.length}
            />
            <SummaryItem label={t("group.exclusive.mountCount")} value={preview.mount_count} tone="create" />
            <SummaryItem label={t("group.exclusive.keepCount")} value={preview.keep_count} tone="keep" />
            <SummaryItem label={t("group.exclusive.unmountCount")} value={preview.unmount_count} tone="remove" />
            <SummaryItem label={t("group.exclusive.skippedCount")} value={preview.skipped_count} tone="risk" />
          </div>
        </section>

        <section className="grid gap-2 rounded-xl border border-theme-card-border bg-theme-card/65 p-3">
          <div className="flex items-center gap-2 text-label-caps uppercase text-outline">
            <AlertTriangle size={15} />
            <span>{t("group.exclusive.detailsTitle")}</span>
          </div>
          <ExclusiveDetailSection count={preview.keep_count} items={preview.keep} title={t("group.exclusive.keepSection")} />
          <ExclusiveDetailSection count={preview.mount_count} items={preview.mount} title={t("group.exclusive.mountSection")} />
          <ExclusiveDetailSection count={preview.unmount_count} items={preview.unmount} title={t("group.exclusive.unmountSection")} />
          <ExclusiveDetailSection
            count={preview.skipped_count}
            items={preview.skipped}
            risk
            title={t("group.exclusive.skippedSection")}
          />
        </section>

      </div>
    </DialogFrame>
  );
}

function SummaryItem({
  label,
  tone,
  value,
}: {
  label: string;
  tone?: "create" | "keep" | "remove" | "risk";
  value: number | ReactNode;
}) {
  const valueClass =
    tone === "create"
      ? "text-status-create"
      : tone === "remove"
        ? "text-status-remove"
        : tone === "risk"
          ? "text-status-conflict"
          : tone === "keep"
            ? "text-primary"
            : "text-on-surface";

  return (
    <div className="min-w-0 rounded-lg border border-theme-control-border bg-theme-control px-3 py-2">
      <div className="text-label-caps uppercase text-outline">{label}</div>
      <div className={`mt-1 min-w-0 font-mono text-body-sm font-semibold ${valueClass}`}>{value}</div>
    </div>
  );
}

function ExclusiveDetailSection({
  count,
  items,
  risk = false,
  title,
}: {
  count: number;
  items: Array<{ asset_id: string; name: string; reason?: string }>;
  risk?: boolean;
  title: string;
}) {
  const { t } = useI18n();

  return (
    <details className="group rounded-lg border border-theme-control-border bg-theme-control/80">
      <summary className="flex min-h-10 cursor-pointer items-center justify-between gap-3 px-3 py-2 text-body-sm font-semibold text-on-surface-variant marker:text-outline hover:text-on-surface">
        <span>{title}</span>
        <span className="rounded-md border border-theme-card-border bg-theme-card px-2 py-0.5 font-mono text-body-sm text-primary">
          {count}
        </span>
      </summary>
      <div className="border-t border-theme-control-border px-3 py-2">
        {items.length === 0 ? (
          <div className="text-body-sm text-on-surface-variant">{t("group.exclusive.emptySection")}</div>
        ) : (
          <ul className="grid gap-1.5">
            {items.map((item) => (
              <li className="min-w-0 rounded-md bg-theme-card-header/50 px-2 py-1.5" key={item.asset_id}>
                <div className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm text-on-surface">
                  {item.name}
                </div>
                {risk && item.reason && (
                  <div className="mt-1 text-body-sm text-status-conflict">{item.reason}</div>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>
    </details>
  );
}
