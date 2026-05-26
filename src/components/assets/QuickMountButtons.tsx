import clsx from "clsx";
import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, TargetProfile } from "../../types";

export function QuickMountButtons({
  asset,
  profiles,
  shortcuts,
  selectedProfileIds,
  onToggle,
}: {
  asset: Asset;
  profiles: TargetProfile[];
  shortcuts: AppShortcut[];
  selectedProfileIds: string[];
  onToggle: (profileId: string) => void;
}) {
  const { t } = useI18n();

  return (
    <div className="flex min-w-0 items-center justify-end gap-1.5">
      {shortcuts
        .filter((shortcut) => shortcut.enabled)
        .map((shortcut) => {
          const profile = profiles.find((candidate) => candidate.id === shortcut.profileId);
          const selected = selectedProfileIds.includes(shortcut.profileId);
          const supported = profile?.supported_kinds.includes(asset.kind) ?? true;
          return (
            <button
              className={clsx(
                "grid size-8 place-items-center rounded-full border text-[13px] font-bold transition-all",
                selected ? "shadow-glow" : "opacity-55 hover:opacity-100",
                !supported && "grayscale",
              )}
              key={shortcut.profileId}
              onClick={() => onToggle(shortcut.profileId)}
              style={{
                borderColor: selected ? shortcut.accentColor : `${shortcut.accentColor}55`,
                backgroundColor: selected ? `${shortcut.accentColor}24` : "transparent",
                color: shortcut.accentColor,
              }}
              title={t(selected ? "mount.unmount" : "mount.mountTo", { profile: shortcut.profileName })}
              type="button"
            >
              {shortcut.displayIcon}
            </button>
          );
        })}
    </div>
  );
}
