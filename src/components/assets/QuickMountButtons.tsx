import clsx from "clsx";
import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, TargetProfile } from "../../types";

export function QuickMountButtons({
  asset,
  mountBlockedReason,
  profiles,
  shortcuts,
  selectedProfileIds,
  onToggle,
}: {
  asset: Asset;
  mountBlockedReason?: string;
  profiles: TargetProfile[];
  shortcuts: AppShortcut[];
  selectedProfileIds: string[];
  onToggle: (profileId: string) => void;
}) {
  const { t } = useI18n();

  return (
    <div className="flex min-w-0 items-center justify-end gap-1">
      {shortcuts
        .filter((shortcut) => shortcut.enabled)
        .map((shortcut) => {
          const profile = profiles.find((candidate) => candidate.id === shortcut.profileId);
          const selected = selectedProfileIds.includes(shortcut.profileId);
          const supported = profile?.supported_kinds.includes(asset.kind) ?? true;
          const disabled = Boolean(mountBlockedReason);
          const label = mountBlockedReason ?? t(selected ? "mount.unmount" : "mount.mountTo", { profile: shortcut.profileName });
          const button = (
            <button
              className={clsx(
                "grid size-8 place-items-center rounded-lg border text-[13px] font-bold transition-all",
                selected ? "shadow-glow ring-1 ring-white/10" : "opacity-60 hover:opacity-100",
                disabled && "pointer-events-none cursor-not-allowed opacity-40 hover:opacity-40",
                !supported && "grayscale",
              )}
              aria-label={label}
              disabled={disabled}
              onClick={() => onToggle(shortcut.profileId)}
              style={{
                borderColor: selected ? shortcut.accentColor : `${shortcut.accentColor}55`,
                backgroundColor: selected ? `${shortcut.accentColor}24` : "transparent",
                color: shortcut.accentColor,
              }}
              title={label}
              type="button"
            >
              {shortcut.displayIcon}
            </button>
          );

          return disabled ? (
            <span className="inline-grid cursor-not-allowed" key={shortcut.profileId} title={label}>
              {button}
            </span>
          ) : (
            <span className="inline-grid" key={shortcut.profileId}>
              {button}
            </span>
          );
        })}
    </div>
  );
}
