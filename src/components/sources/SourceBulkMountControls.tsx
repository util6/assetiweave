import clsx from "clsx";
import { CheckCheck } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, Source, TargetProfile } from "../../types";
import { isDirectMountBlockedSource } from "../../utils/mountPolicy";
import { AppShortcutIconForShortcut } from "../apps/AppShortcutIcon";

export function SourceBulkMountControls({
  appShortcuts,
  assets,
  busy,
  onSetSourceMountProfile,
  profiles,
  selectedMounts,
  source,
  variant = "inline",
}: {
  appShortcuts: AppShortcut[];
  assets: Asset[];
  busy: boolean;
  onSetSourceMountProfile: (assetIds: string[], profileId: string, enabled: boolean) => void;
  profiles: TargetProfile[];
  selectedMounts: Record<string, string[]>;
  source: Source;
  variant?: "inline" | "panel";
}) {
  const { t } = useI18n();
  const skillAssets = assets.filter((asset) => asset.kind === "skill");
  const assetIds = skillAssets.map((asset) => asset.id);
  const blocked = isDirectMountBlockedSource(source);
  const availableShortcuts = appShortcuts
    .filter((shortcut) => shortcut.enabled)
    .filter((shortcut) => profiles.some((profile) => profile.id === shortcut.profileId && profile.enabled));

  if (variant === "inline" && (skillAssets.length === 0 || availableShortcuts.length === 0)) {
    return null;
  }

  if (variant === "panel" && (skillAssets.length === 0 || availableShortcuts.length === 0 || blocked)) {
    return (
      <div className="rounded-xl border border-border bg-surface-lowest/35 px-3 py-3">
        <div className="flex items-center gap-2 text-label-caps uppercase text-outline">
          <CheckCheck size={15} />
          <span>{t("source.bulk.title")}</span>
        </div>
        <p className="mt-2 text-body-sm text-on-surface-variant">
          {blocked
            ? t("mount.blockedAppSource")
            : skillAssets.length === 0
              ? t("source.bulk.empty")
              : t("source.bulk.noApps")}
        </p>
      </div>
    );
  }

  return (
    <div
      className={clsx(
        variant === "panel"
          ? "rounded-xl border border-border bg-surface-lowest/35 p-3"
          : "flex min-w-0 items-center gap-1.5 rounded-xl border border-border/70 bg-surface-lowest/35 p-1.5",
      )}
    >
      {variant === "panel" && (
        <div className="mb-3 flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-2 text-label-caps uppercase text-outline">
            <CheckCheck size={15} />
            <span>{t("source.bulk.title")}</span>
          </div>
          <span className="shrink-0 rounded-md border border-border bg-surface-high px-2 py-0.5 font-mono text-body-sm text-primary">
            {t("source.bulk.summary", { count: skillAssets.length })}
          </span>
        </div>
      )}

      <div className={clsx(variant === "panel" ? "grid grid-cols-2 gap-2 max-[1180px]:grid-cols-1" : "flex items-center gap-1")}>
        {availableShortcuts.map((shortcut) => {
          const selectedCount = skillAssets.filter((asset) => (selectedMounts[asset.id] ?? []).includes(shortcut.profileId)).length;
          const allSelected = skillAssets.length > 0 && selectedCount === skillAssets.length;
          const label = t(allSelected ? "source.bulk.unselectAll" : "source.bulk.selectAll", {
            profile: shortcut.profileName,
          });

          return (
            <button
              aria-label={label}
              aria-pressed={allSelected}
              className={clsx(
                "inline-flex min-w-0 items-center rounded-lg border text-left transition-all disabled:cursor-not-allowed disabled:opacity-45",
                variant === "panel" ? "h-12 gap-2 px-2.5" : "h-8 gap-1.5 px-2",
                allSelected
                  ? "border-status-create/70 bg-status-create/12 text-status-create shadow-[inset_0_1px_0_rgba(255,255,255,0.04)]"
                  : selectedCount > 0
                    ? "border-primary/55 bg-primary/10 text-primary hover:bg-primary/15"
                    : "border-border bg-surface-high/70 text-on-surface-variant hover:border-outline-variant hover:text-on-surface",
              )}
              disabled={busy || blocked || skillAssets.length === 0}
              key={shortcut.profileId}
              onClick={() => onSetSourceMountProfile(assetIds, shortcut.profileId, !allSelected)}
              title={blocked ? t("mount.blockedAppSource") : label}
              type="button"
            >
              <AppShortcutIconForShortcut className="size-4 shrink-0" shortcut={shortcut} />
              <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm font-semibold">
                {variant === "panel" ? shortcut.profileName : t("source.bulk.progress", { selected: selectedCount, total: skillAssets.length })}
              </span>
              {variant === "panel" && (
                <span className="ml-auto shrink-0 font-mono text-body-sm">
                  {t("source.bulk.progress", { selected: selectedCount, total: skillAssets.length })}
                </span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}
