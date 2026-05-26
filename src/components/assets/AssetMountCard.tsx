import clsx from "clsx";
import { Check } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, TargetProfile } from "../../types";
import { abbreviateHomePath } from "../../utils/path";

export function AssetMountCard({
  asset,
  profile,
  selected,
  shortcut,
  onToggle,
}: {
  asset: Asset;
  profile: TargetProfile;
  selected: boolean;
  shortcut?: AppShortcut;
  onToggle: (profileId: string) => void;
}) {
  const { t } = useI18n();
  const supported = profile.supported_kinds.includes(asset.kind);
  const accentColor = shortcut?.accentColor ?? "#8c909f";
  const displayIcon = shortcut?.displayIcon ?? profile.name.slice(0, 1).toUpperCase();

  return (
    <button
      aria-pressed={selected}
      className={clsx(
        "group relative min-h-[76px] overflow-hidden rounded-xl border px-3 py-2.5 text-left transition-all",
        "bg-surface-high/80 hover:-translate-y-px hover:bg-surface-highest/80",
        selected
          ? "border-status-create/70 bg-status-create/12 shadow-[0_0_0_1px_rgba(16,185,129,0.25),0_16px_34px_rgba(16,185,129,0.16)]"
          : "border-border hover:border-outline-variant",
        !supported && "opacity-60",
      )}
      onClick={() => onToggle(profile.id)}
      type="button"
    >
      {selected && <span className="absolute inset-x-3 top-0 h-px bg-status-create/80" aria-hidden="true" />}
      <div className="flex min-w-0 items-start gap-3">
        <span
          className={clsx(
            "grid size-9 shrink-0 place-items-center rounded-full border text-[13px] font-bold transition-transform group-hover:scale-105",
            selected && "shadow-[0_0_18px_rgba(16,185,129,0.2)]",
          )}
          style={{
            borderColor: selected ? "#10b981" : `${accentColor}55`,
            backgroundColor: selected ? "rgba(16,185,129,0.16)" : `${accentColor}18`,
            color: selected ? "#10b981" : accentColor,
          }}
          aria-hidden="true"
        >
          {displayIcon}
        </span>

        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center justify-between gap-2">
            <p className="overflow-hidden text-ellipsis whitespace-nowrap text-body-sm font-bold text-on-surface">{profile.name}</p>
            <span
              className={clsx(
                "grid size-5 shrink-0 place-items-center rounded-full border transition-colors",
                selected ? "border-status-create bg-status-create text-background" : "border-border bg-surface-high text-transparent",
              )}
              aria-hidden="true"
            >
              <Check size={13} />
            </span>
          </div>
          <p className="mt-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-code-sm text-on-surface-variant">
            {abbreviateHomePath(profile.target_paths[0] ?? "")}
          </p>
          <div className="mt-2 flex min-w-0 items-center justify-between gap-2">
            <span
              className={clsx(
                "inline-flex min-w-0 items-center gap-1.5 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm",
                supported ? "text-status-create" : "text-status-conflict",
              )}
            >
              <span className={clsx("size-1.5 shrink-0 rounded-full", supported ? "bg-status-create" : "bg-status-conflict")} aria-hidden="true" />
              {t(supported ? "mount.supported" : "mount.unsupported")}
            </span>
            <span className="shrink-0 font-mono text-code-sm uppercase text-outline">{profile.app_kind}</span>
          </div>
        </div>
      </div>
    </button>
  );
}
