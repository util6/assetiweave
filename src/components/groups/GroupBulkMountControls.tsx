import clsx from "clsx";
import { CheckCheck } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, AssetGroupDetail, AssetMountStatus, TargetProfile } from "../../types";
import { AppShortcutIconForShortcut } from "../apps/AppShortcutIcon";

export function GroupBulkMountControls({
  appShortcuts,
  assets,
  busy,
  detail,
  mountStatusesByAssetId,
  onSetGroupMountProfile,
  profiles,
  variant = "inline",
}: {
  appShortcuts: AppShortcut[];
  assets: Asset[];
  busy: boolean;
  detail: AssetGroupDetail;
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  onSetGroupMountProfile: (profileId: string, enabled: boolean) => void | Promise<void>;
  profiles: TargetProfile[];
  variant?: "inline" | "panel";
}) {
  const { t } = useI18n();
  const skillAssets = assets.filter((asset) => asset.kind === "skill");
  const availableShortcuts = appShortcuts
    .filter((shortcut) => shortcut.enabled)
    .filter((shortcut) =>
      profiles.some(
        (profile) =>
          profile.id === shortcut.profileId &&
          profile.enabled &&
          profile.supported_kinds.includes("skill"),
      ),
    );

  if (variant === "inline" && (skillAssets.length === 0 || availableShortcuts.length === 0)) {
    return null;
  }

  if (variant === "panel" && (skillAssets.length === 0 || availableShortcuts.length === 0 || !detail.group.enabled)) {
    return (
      <div className="rounded-xl border border-border bg-surface-lowest/35 px-3 py-3">
        <div className="flex items-center gap-2 text-label-caps uppercase text-outline">
          <CheckCheck size={15} />
          <span>{t("group.mount.title")}</span>
        </div>
        <p className="mt-2 text-body-sm text-on-surface-variant">
          {!detail.group.enabled
            ? t("group.mount.disabled")
            : skillAssets.length === 0
              ? t("group.mount.empty")
              : t("group.mount.noApps")}
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
            <span>{t("group.mount.title")}</span>
          </div>
          <span className="shrink-0 rounded-md border border-border bg-surface-high px-2 py-0.5 font-mono text-body-sm text-primary">
            {t("group.mount.summary", { count: skillAssets.length })}
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
          const label = t(allMounted ? "group.mount.unmount" : "group.mount.mount", {
            profile: shortcut.profileName,
          });

          return (
            <button
              aria-label={label}
              aria-pressed={allMounted}
              className={clsx(
                "inline-flex min-w-0 items-center rounded-lg border text-left transition-all disabled:cursor-not-allowed disabled:opacity-45",
                variant === "panel" ? "h-12 gap-2 px-2.5" : "h-8 gap-1.5 px-2",
                allMounted
                  ? "border-status-create/70 bg-status-create/12 text-status-create shadow-[inset_0_1px_0_rgba(255,255,255,0.04)]"
                  : hasPartialMounts
                    ? "border-primary/55 bg-primary/10 text-primary hover:bg-primary/15"
                    : "border-border bg-surface-high/70 text-on-surface-variant hover:border-outline-variant hover:text-on-surface",
              )}
              disabled={busy || !detail.group.enabled || skillAssets.length === 0}
              key={shortcut.profileId}
              onClick={() => void onSetGroupMountProfile(shortcut.profileId, !allMounted)}
              title={!detail.group.enabled ? t("group.mount.disabled") : label}
              type="button"
            >
              <AppShortcutIconForShortcut className="size-4 shrink-0" shortcut={shortcut} />
              <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm font-semibold">
                {variant === "panel"
                  ? shortcut.profileName
                  : t("group.mount.progress", { selected: mountedCount, total: skillAssets.length })}
              </span>
              {variant === "panel" && (
                <span className="ml-auto shrink-0 font-mono text-body-sm">
                  {t("group.mount.progress", { selected: mountedCount, total: skillAssets.length })}
                </span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}
