import clsx from "clsx";
import { Check } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type { Asset, TargetProfile } from "../../types";
import { abbreviateHomePath } from "../../utils/path";

export function AssetMountCard({
  asset,
  profile,
  selected,
  onToggle,
}: {
  asset: Asset;
  profile: TargetProfile;
  selected: boolean;
  onToggle: (profileId: string) => void;
}) {
  const { t } = useI18n();
  const supported = profile.supported_kinds.includes(asset.kind);

  return (
    <button
      className={clsx(
        "min-h-16 rounded-lg border bg-surface-high px-3 py-2.5 text-left transition-all",
        selected ? "border-status-create/70 bg-status-create/12 shadow-glow" : "border-border hover:border-outline-variant hover:bg-surface-highest",
        !supported && "opacity-60",
      )}
      onClick={() => onToggle(profile.id)}
      type="button"
    >
      <div className="flex items-center justify-between gap-2">
        <div className="min-w-0">
          <p className="overflow-hidden text-ellipsis whitespace-nowrap text-body-sm font-bold text-on-surface">{profile.name}</p>
          <p className="mt-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-code-sm text-on-surface-variant">
            {abbreviateHomePath(profile.target_paths[0] ?? "")}
          </p>
        </div>
        <span
          className={clsx(
            "grid size-6 shrink-0 place-items-center rounded-full border transition-colors",
            selected ? "border-status-create bg-status-create text-background" : "border-border text-transparent",
          )}
        >
          <Check size={15} />
        </span>
      </div>
      <p className={clsx("mt-2 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm", supported ? "text-status-create" : "text-status-conflict")}>
        {t(supported ? "mount.supported" : "mount.unsupported")}
      </p>
    </button>
  );
}
