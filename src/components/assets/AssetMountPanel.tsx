import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, AssetMountStatus, TargetProfile } from "../../types";
import { AssetMountCard } from "./AssetMountCard";

export function AssetMountPanel({
  appShortcuts,
  asset,
  mountBlockedReason,
  mountStatuses,
  profiles,
  onToggle,
}: {
  appShortcuts: AppShortcut[];
  asset: Asset;
  mountBlockedReason?: string;
  mountStatuses: AssetMountStatus[];
  profiles: TargetProfile[];
  onToggle: (profileId: string) => void;
}) {
  const { t } = useI18n();
  const enabledProfiles = profiles.filter((profile) => profile.enabled);
  const statusByProfileId = new Map(mountStatuses.map((status) => [status.profile_id, status]));
  const mountedCount = mountStatuses.filter((status) => status.state === "mounted").length;

  return (
    <div className="border-t border-border/60 bg-surface/60 px-4 pb-4 pt-3" onClick={(event) => event.stopPropagation()}>
      <div className="mb-3 flex items-center justify-between gap-4">
        <div className="min-w-0">
          <span className="text-label-caps uppercase text-outline">{t("mount.title")}</span>
          <p className="mt-1 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm text-on-surface-variant">
            {mountBlockedReason ? t("mount.blockedAppSource") : t("mount.description")}
          </p>
        </div>
        <span className="rounded-md border border-border bg-surface-high px-2.5 py-1 font-mono text-body-sm text-primary">
          {t("mount.selected", { count: mountedCount })}
        </span>
      </div>

      {enabledProfiles.length === 0 ? (
        <div className="rounded-lg border border-border bg-surface-high px-3 py-3 text-body-sm text-on-surface-variant">{t("mount.empty")}</div>
      ) : (
        <div className="grid grid-cols-4 gap-2.5 max-[980px]:grid-cols-2 max-[720px]:grid-cols-1">
          {enabledProfiles.map((profile) => (
            <AssetMountCard
              asset={asset}
              key={profile.id}
              mountBlockedReason={mountBlockedReason}
              mountStatus={statusByProfileId.get(profile.id)}
              onToggle={onToggle}
              profile={profile}
              shortcut={appShortcuts.find((shortcut) => shortcut.profileId === profile.id)}
            />
          ))}
        </div>
      )}
    </div>
  );
}
