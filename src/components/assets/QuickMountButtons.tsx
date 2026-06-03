import clsx from "clsx";
import { useI18n } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import type { AppShortcut, Asset, AssetMountStatus, TargetProfile } from "../../types";
import { getMountDisplayState, type MountDisplayState } from "../../utils/mountState";
import { AppShortcutIconForShortcut } from "../apps/AppShortcutIcon";

export function QuickMountButtons({
  asset,
  mountBlockedReason,
  mountStatuses,
  profiles,
  shortcuts,
  onToggle,
}: {
  asset: Asset;
  mountBlockedReason?: string;
  mountStatuses: AssetMountStatus[];
  profiles: TargetProfile[];
  shortcuts: AppShortcut[];
  onToggle: (profileId: string) => void;
}) {
  const { t } = useI18n();
  const statusByProfileId = new Map(mountStatuses.map((status) => [status.profile_id, status]));
  const visibleShortcuts = shortcuts.filter((shortcut) => shortcut.enabled);

  if (visibleShortcuts.length === 0) return null;

  return (
    <div className="flex w-fit max-w-full flex-wrap items-center justify-end gap-1">
      {visibleShortcuts.map((shortcut) => {
        const profile = profiles.find((candidate) => candidate.id === shortcut.profileId);
        const mountState = getMountDisplayState(statusByProfileId.get(shortcut.profileId));
        const mounted = mountState === "mounted";
        const supported = profile?.supported_kinds.includes(asset.kind) ?? true;
        const disabled = Boolean(mountBlockedReason);
        const label = mountBlockedReason ?? mountActionLabel(mountState, shortcut.profileName, t);
        const ringColor = mountStateRingColor(mountState, shortcut.accentColor);
        const button = (
          <button
            className={clsx(
              "relative grid size-8 place-items-center overflow-hidden rounded-lg border text-[13px] font-bold transition-all",
              mounted ? "shadow-glow ring-1 ring-theme-nav-active-border/30" : "opacity-60 hover:opacity-100",
              (mountState === "conflict" || mountState === "broken") && "opacity-90",
              disabled && "pointer-events-none cursor-not-allowed opacity-40 hover:opacity-40",
              !supported && "grayscale",
            )}
            aria-label={label}
            disabled={disabled}
            onClick={() => onToggle(shortcut.profileId)}
            style={{
              borderColor: mounted ? shortcut.accentColor : `${ringColor}88`,
              backgroundColor: mounted ? `${shortcut.accentColor}24` : mountStateBackgroundColor(mountState, ringColor),
              color: shortcut.accentColor,
            }}
            title={label}
            type="button"
          >
            <MountButtonStateRing color={ringColor} state={mountState} />
            <AppShortcutIconForShortcut className="size-4" shortcut={shortcut} />
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

function mountActionLabel(
  state: MountDisplayState,
  profileName: string,
  t: (key: TranslationKey, params?: Record<string, string | number>) => string,
) {
  if (state === "mounted") return t("mount.unmount", { profile: profileName });
  if (state === "broken" || state === "conflict") {
    return t("mount.action.repair", { profile: profileName });
  }
  return t("mount.mountTo", { profile: profileName });
}

function MountButtonStateRing({ color, state }: { color: string; state: MountDisplayState }) {
  if (state === "mounted" || state === "not_mounted") return null;

  return (
    <span
      aria-hidden="true"
      className="pointer-events-none absolute inset-[4px] rounded-md border border-t-transparent opacity-90 animate-spin motion-reduce:animate-none"
      style={{ borderBottomColor: color, borderLeftColor: color, borderRightColor: color }}
    />
  );
}

function mountStateRingColor(state: MountDisplayState, accentColor: string) {
  if (state === "conflict" || state === "broken") return "rgb(var(--color-status-remove))";
  return accentColor;
}

function mountStateBackgroundColor(state: MountDisplayState, ringColor: string) {
  if (state === "conflict" || state === "broken") {
    return "rgb(var(--color-status-remove) / 0.12)";
  }
  return "transparent";
}
