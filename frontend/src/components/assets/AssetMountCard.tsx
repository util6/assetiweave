import clsx from "clsx";
import { Check } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import { DEFAULT_ENTITY_ACCENT_HEX } from "../../theme/themes";
import type { TranslationKey } from "../../i18n/messages";
import type { AppShortcut, Asset, AssetMountStatus, TargetProfile } from "../../types";
import { getMountDisplayState, type MountDisplayState } from "../../utils/mountState";
import { abbreviateHomePath } from "../../utils/path";
import { AppShortcutIcon } from "../apps/AppShortcutIcon";

export function AssetMountCard({
  asset,
  mountBlockedReason,
  mountStatus,
  profile,
  shortcut,
  onToggle,
}: {
  asset: Asset;
  mountBlockedReason?: string;
  mountStatus?: AssetMountStatus;
  profile: TargetProfile;
  shortcut?: AppShortcut;
  onToggle: (profileId: string) => void;
}) {
  const { t } = useI18n();
  const supported = profile.supported_kinds.includes(asset.kind);
  const accentColor = shortcut?.accentColor ?? DEFAULT_ENTITY_ACCENT_HEX;
  const displayIcon = shortcut?.displayIcon ?? profile.name.slice(0, 1).toUpperCase();
  const disabled = Boolean(mountBlockedReason);
  const displayState = getMountDisplayState(mountStatus);
  const mounted = displayState === "mounted";
  const targetDir = mountStatus?.display_target_dir ?? mountStatus?.target_dir ?? profile.target_paths[0] ?? "";

  return (
    <button
      aria-pressed={mounted}
      className={clsx(
        "group relative min-h-[76px] overflow-hidden rounded-xl border px-3 py-2.5 text-left transition-all",
        "bg-theme-control/80 hover:-translate-y-px hover:bg-theme-control-hover/80",
        mounted ? "hover:brightness-[1.03]" : mountCardStateClass(displayState),
        !supported && "opacity-60",
        disabled && "cursor-not-allowed opacity-55 hover:translate-y-0 hover:border-theme-control-border hover:bg-theme-control/80",
      )}
      disabled={disabled}
      onClick={() => onToggle(profile.id)}
      style={
        mounted
          ? {
              backgroundColor: `${accentColor}1f`,
              borderColor: `${accentColor}b3`,
              boxShadow: `0 0 0 1px ${accentColor}40, 0 16px 34px ${accentColor}29`,
            }
          : undefined
      }
      title={mountBlockedReason}
      type="button"
    >
      {mounted && (
        <span
          className="absolute inset-x-3 top-0 h-px"
          aria-hidden="true"
          style={{ backgroundColor: `${accentColor}cc` }}
        />
      )}
      <div className="flex min-w-0 items-start gap-3">
        <span
          className="relative grid size-9 shrink-0 place-items-center overflow-hidden rounded-full border text-[13px] font-bold transition-transform group-hover:scale-105"
          style={{
            borderColor: mounted ? accentColor : mountCardRingColor(displayState, accentColor),
            backgroundColor: mounted ? `${accentColor}29` : `${accentColor}18`,
            boxShadow: mounted ? `0 0 18px ${accentColor}33` : undefined,
            color: accentColor,
          }}
          aria-hidden="true"
        >
          <MountCardStateRing color={mountCardRingColor(displayState, accentColor)} state={displayState} />
          <AppShortcutIcon appKind={profile.app_kind} className="size-4" displayIcon={displayIcon} iconSvg={shortcut?.iconSvg} />
        </span>

        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center justify-between gap-2">
            <p className="overflow-hidden text-ellipsis whitespace-nowrap text-body-sm font-bold text-on-surface">{profile.name}</p>
            <span
              className={clsx(
                "grid size-5 shrink-0 place-items-center rounded-full border transition-colors",
                mounted ? "text-background" : "border-theme-control-border bg-theme-control text-transparent",
              )}
              aria-hidden="true"
              style={mounted ? { backgroundColor: accentColor, borderColor: accentColor } : undefined}
            >
              <Check size={13} />
            </span>
          </div>
          <p className="mt-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-code-sm text-on-surface-variant">
            {abbreviateHomePath(targetDir)}
          </p>
          <div className="mt-2 flex min-w-0 items-center gap-2">
            <span
              className={clsx(
                "inline-flex min-w-0 items-center gap-1.5 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm",
                disabled || !supported ? "text-status-conflict" : !mounted && mountCardStateTextClass(displayState),
              )}
              style={mounted && !disabled && supported ? { color: accentColor } : undefined}
            >
              <span
                className={clsx(
                  "size-1.5 shrink-0 rounded-full",
                  disabled || !supported ? "bg-status-conflict" : !mounted && mountCardStateDotClass(displayState),
                )}
                aria-hidden="true"
                style={mounted && !disabled && supported ? { backgroundColor: accentColor } : undefined}
              />
              {disabled ? t("mount.blocked") : supported ? t(`mount.display.${displayState}` as TranslationKey) : t("mount.unsupported")}
            </span>
          </div>
        </div>
      </div>
    </button>
  );
}

function MountCardStateRing({ color, state }: { color: string; state: MountDisplayState }) {
  if (state === "mounted" || state === "not_mounted") return null;

  return (
    <span
      className="absolute inset-[4px] rounded-full border border-t-transparent opacity-90 animate-spin motion-reduce:animate-none"
      style={{ borderBottomColor: color, borderLeftColor: color, borderRightColor: color }}
    />
  );
}

function mountCardStateClass(state: MountDisplayState) {
  if (state === "conflict" || state === "broken") return "border-status-remove/50 bg-status-remove/10 hover:border-status-remove/65";
  return "border-theme-control-border hover:border-theme-nav-active-border";
}

function mountCardStateTextClass(state: MountDisplayState) {
  if (state === "conflict" || state === "broken") return "text-status-remove";
  return "text-on-surface-variant";
}

function mountCardStateDotClass(state: MountDisplayState) {
  if (state === "conflict" || state === "broken") return "bg-status-remove";
  return "bg-outline";
}

function mountCardRingColor(state: MountDisplayState, accentColor: string) {
  if (state === "conflict" || state === "broken") return "rgb(var(--color-status-remove))";
  return accentColor;
}
