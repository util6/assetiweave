import clsx from "clsx";
import { Check } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
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
  const accentColor = shortcut?.accentColor ?? "#8c909f";
  const displayIcon = shortcut?.displayIcon ?? profile.name.slice(0, 1).toUpperCase();
  const disabled = Boolean(mountBlockedReason);
  const displayState = getMountDisplayState(mountStatus);
  const mounted = displayState === "mounted";
  const targetDir = mountStatus?.target_dir ?? profile.target_paths[0] ?? "";

  return (
    <button
      aria-pressed={mounted}
      className={clsx(
        "group relative min-h-[76px] overflow-hidden rounded-xl border px-3 py-2.5 text-left transition-all",
        "bg-surface-high/80 hover:-translate-y-px hover:bg-surface-highest/80",
        mounted
          ? "border-status-create/70 bg-status-create/12 shadow-[0_0_0_1px_rgba(16,185,129,0.25),0_16px_34px_rgba(16,185,129,0.16)]"
          : mountCardStateClass(displayState),
        !supported && "opacity-60",
        disabled && "cursor-not-allowed opacity-55 hover:translate-y-0 hover:border-border hover:bg-surface-high/80",
      )}
      disabled={disabled}
      onClick={() => onToggle(profile.id)}
      title={mountBlockedReason}
      type="button"
    >
      {mounted && <span className="absolute inset-x-3 top-0 h-px bg-status-create/80" aria-hidden="true" />}
      <div className="flex min-w-0 items-start gap-3">
        <span
          className={clsx(
            "relative grid size-9 shrink-0 place-items-center overflow-hidden rounded-full border text-[13px] font-bold transition-transform group-hover:scale-105",
            mounted && "shadow-[0_0_18px_rgba(16,185,129,0.2)]",
          )}
          style={{
            borderColor: mounted ? "#10b981" : mountCardRingColor(displayState, accentColor),
            backgroundColor: mounted ? "rgba(16,185,129,0.16)" : `${accentColor}18`,
            color: mounted ? "#10b981" : accentColor,
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
                mounted ? "border-status-create bg-status-create text-background" : "border-border bg-surface-high text-transparent",
              )}
              aria-hidden="true"
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
                disabled || !supported ? "text-status-conflict" : mountCardStateTextClass(displayState),
              )}
            >
              <span
                className={clsx(
                  "size-1.5 shrink-0 rounded-full",
                  disabled || !supported ? "bg-status-conflict" : mountCardStateDotClass(displayState),
                )}
                aria-hidden="true"
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
  return "border-border hover:border-outline-variant";
}

function mountCardStateTextClass(state: MountDisplayState) {
  if (state === "mounted") return "text-status-create";
  if (state === "conflict" || state === "broken") return "text-status-remove";
  return "text-on-surface-variant";
}

function mountCardStateDotClass(state: MountDisplayState) {
  if (state === "mounted") return "bg-status-create";
  if (state === "conflict" || state === "broken") return "bg-status-remove";
  return "bg-outline";
}

function mountCardRingColor(state: MountDisplayState, accentColor: string) {
  if (state === "conflict" || state === "broken") return "#f43f5e";
  return accentColor;
}
