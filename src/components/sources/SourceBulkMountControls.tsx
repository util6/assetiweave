import clsx from "clsx";
import { CheckCheck } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, AssetMountStatus, Source, TargetProfile } from "../../types";
import { isDirectMountBlockedSource } from "../../utils/mountPolicy";
import { AppShortcutIconForShortcut } from "../apps/AppShortcutIcon";

export function SourceBulkMountControls({
  appShortcuts,
  assets,
  busy,
  mountStatusesByAssetId,
  onSetSourceMountProfile,
  profiles,
  source,
  variant = "inline",
}: {
  appShortcuts: AppShortcut[];
  assets: Asset[];
  busy: boolean;
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  onSetSourceMountProfile: (assetIds: string[], profileId: string, enabled: boolean) => void;
  profiles: TargetProfile[];
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
      <div className="rounded-xl border border-theme-card-border bg-theme-card/65 px-3 py-3 shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.34)]">
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
          ? "rounded-xl border border-theme-card-border bg-theme-card/65 p-3 shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.34)]"
          : "flex min-w-0 items-center gap-1.5 rounded-xl border border-theme-control-border bg-theme-control/60 p-1.5 shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.3)]",
      )}
    >
      {variant === "panel" && (
        <div className="mb-3 flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-2 text-label-caps uppercase text-outline">
            <CheckCheck size={15} />
            <span>{t("source.bulk.title")}</span>
          </div>
          <span className="shrink-0 rounded-md border border-theme-control-border bg-theme-control px-2 py-0.5 font-mono text-body-sm text-primary">
            {t("source.bulk.summary", { count: skillAssets.length })}
          </span>
        </div>
      )}

      <div className={clsx(variant === "panel" ? "grid grid-cols-2 gap-2 max-[1180px]:grid-cols-1" : "flex items-center gap-1")}>
        {availableShortcuts.map((shortcut) => {
          const mountedCount = skillAssets.filter((asset) => {
            const mountStatus = (mountStatusesByAssetId.get(asset.id) ?? []).find(
              (status) => status.profile_id === shortcut.profileId,
            );
            return mountStatus?.state === "mounted";
          }).length;
          const allMounted = skillAssets.length > 0 && mountedCount === skillAssets.length;
          const hasPartialMounts = mountedCount > 0 && !allMounted;
          const label = t(allMounted ? "source.bulk.unselectAll" : "source.bulk.selectAll", {
            profile: shortcut.profileName,
          });

          return (
            <button
              aria-label={label}
              aria-pressed={allMounted}
              className={clsx(
                "inline-flex min-w-0 items-center rounded-lg border text-left transition-all hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-45",
                variant === "panel" ? "h-12 gap-2 px-2.5" : "h-8 gap-1.5 px-2",
                allMounted && "shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.28)]",
              )}
              disabled={busy || blocked || skillAssets.length === 0}
              key={shortcut.profileId}
              onClick={() => onSetSourceMountProfile(assetIds, shortcut.profileId, !allMounted)}
              style={bulkMountButtonStyle(shortcut.accentColor, allMounted, hasPartialMounts)}
              title={blocked ? t("mount.blockedAppSource") : label}
              type="button"
            >
              <AppShortcutIconForShortcut className="size-4 shrink-0" shortcut={shortcut} />
              <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm font-semibold">
                {variant === "panel" ? shortcut.profileName : t("source.bulk.progress", { selected: mountedCount, total: skillAssets.length })}
              </span>
              {variant === "panel" && (
                <span className="ml-auto shrink-0 font-mono text-body-sm">
                  {t("source.bulk.progress", { selected: mountedCount, total: skillAssets.length })}
                </span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}

function bulkMountButtonStyle(accentColor: string, allMounted: boolean, hasPartialMounts: boolean) {
  return {
    backgroundColor: allMounted ? `${accentColor}24` : hasPartialMounts ? `${accentColor}18` : `${accentColor}10`,
    borderColor: allMounted ? accentColor : hasPartialMounts ? `${accentColor}99` : `${accentColor}66`,
    color: accentColor,
  };
}
