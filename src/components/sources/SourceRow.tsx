import clsx from "clsx";
import { ChevronDown, ChevronRight, FolderOpen, Power, Trash2 } from "lucide-react";
import { AssetRow } from "../assets/AssetRow";
import { sourceKindLabel, sourceOriginLabel, translateScanStatus } from "../../i18n/domain";
import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, AssetMountStatus, Source, TargetProfile } from "../../types";
import { abbreviateHomePath } from "../../utils/path";
import { SourceBulkMountControls } from "./SourceBulkMountControls";

export function SourceRow({
  appShortcuts,
  assets,
  mountStatusesByAssetId,
  busy,
  expanded,
  expandedAssetIds,
  onDelete,
  onAssetReveal,
  onReveal,
  onSetSourceMountProfile,
  onToggleAsset,
  onToggleExpanded,
  onToggleMount,
  onToggle,
  profiles,
  selectedMounts,
  source,
}: {
  appShortcuts: AppShortcut[];
  assets: Asset[];
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  busy: boolean;
  expanded: boolean;
  expandedAssetIds: Set<string>;
  onDelete: () => void;
  onAssetReveal: (path: string) => void;
  onReveal: () => void;
  onSetSourceMountProfile: (assetIds: string[], profileId: string, enabled: boolean) => void;
  onToggleAsset: (assetId: string) => void;
  onToggleExpanded: () => void;
  onToggleMount: (assetId: string, profileId: string) => void;
  onToggle: () => void;
  profiles: TargetProfile[];
  selectedMounts: Record<string, string[]>;
  source: Source;
}) {
  const { t } = useI18n();
  const statusTone = source.last_scan_status?.startsWith("error:")
    ? "conflict"
    : source.last_scan_status?.startsWith("ok:")
      ? "create"
      : "idle";

  return (
    <article className={clsx("border-b border-border last:border-b-0", expanded && "bg-surface-low/35")}>
      <div className="grid min-h-20 grid-cols-[minmax(0,1fr)_auto] items-center gap-4 px-4 py-3.5 hover:bg-surface-low/70">
        <div className="min-w-0">
          <div className="flex min-w-0 items-center gap-2">
            <span
              className={clsx(
                "size-2 rounded-full",
                source.enabled ? "bg-status-create shadow-[0_0_12px_rgba(16,185,129,0.45)]" : "bg-outline",
              )}
              aria-hidden="true"
            />
            <h3 className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-code-md text-on-surface">{source.name}</h3>
            <span className="rounded-md border border-border bg-surface-high px-2 py-0.5 text-label-caps uppercase text-on-surface-variant">
              {sourceKindLabel(source.kind, t)}
            </span>
            <span
              className={clsx(
                "rounded-md border px-2 py-0.5 text-label-caps uppercase",
                source.source_origin === "app_target" || source.source_origin === "app_local"
                  ? "border-status-conflict/30 bg-status-conflict/10 text-status-conflict"
                  : "border-border bg-surface-high text-on-surface-variant",
              )}
            >
              {sourceOriginLabel(source.source_origin, t)}
            </span>
            <span
              className={clsx(
                "rounded-md px-2 py-0.5 text-label-caps uppercase",
                statusTone === "create" && "bg-status-create/15 text-status-create",
                statusTone === "conflict" && "bg-status-conflict/15 text-status-conflict",
                statusTone === "idle" && "bg-surface-highest text-outline",
              )}
            >
              {translateScanStatus(source.last_scan_status, t)}
            </span>
          </div>

          <button
            className="mt-2 max-w-full overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm text-on-surface-variant transition-colors hover:text-primary"
            onClick={onReveal}
            title={t("source.action.reveal")}
            type="button"
          >
            {abbreviateHomePath(source.root_path)}
          </button>
        </div>

        <div className="flex items-start gap-3 max-[1180px]:flex-col max-[1180px]:items-end">
          <SourceBulkMountControls
            appShortcuts={appShortcuts}
            assets={assets}
            busy={busy}
            onSetSourceMountProfile={onSetSourceMountProfile}
            profiles={profiles}
            selectedMounts={selectedMounts}
            source={source}
          />
          <div className="flex items-start gap-1.5">
            <SourceIconButton
              disabled={busy}
              label={t(source.enabled ? "source.action.disable" : "source.action.enable")}
              onClick={onToggle}
            >
              <Power size={16} />
            </SourceIconButton>
            <SourceIconButton label={t("source.action.reveal")} onClick={onReveal}>
              <FolderOpen size={16} />
            </SourceIconButton>
            <SourceIconButton disabled={busy} label={t("source.action.delete")} onClick={onDelete} danger>
              <Trash2 size={16} />
            </SourceIconButton>
            <SourceIconButton label={t(expanded ? "source.action.collapse" : "source.action.expand")} onClick={onToggleExpanded}>
              {expanded ? <ChevronDown size={17} /> : <ChevronRight size={17} />}
            </SourceIconButton>
          </div>
        </div>
      </div>

      {expanded && (
        <div className="border-t border-border bg-surface-lowest/20 py-2 pl-8 pr-3">
          <div className="border-l border-outline-variant/70 pl-3">
            {assets.length === 0 ? (
              <div className="px-4 py-4 text-body-sm text-on-surface-variant">{t("source.emptySkills")}</div>
            ) : (
              <div className="overflow-hidden rounded-xl border border-border bg-surface-card/35">
                {assets.map((asset) => (
                  <AssetRow
                    appShortcuts={appShortcuts}
                    asset={asset}
                    expanded={expandedAssetIds.has(asset.id)}
                    key={asset.id}
                    onRevealPath={onAssetReveal}
                    onToggleExpanded={() => onToggleAsset(asset.id)}
                    onToggleMount={(profileId) => onToggleMount(asset.id, profileId)}
                    profiles={profiles}
                    selectedProfileIds={selectedMounts[asset.id] ?? []}
                    source={source}
                    mountStatuses={mountStatusesByAssetId.get(asset.id) ?? []}
                  />
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </article>
  );
}

function SourceIconButton({
  children,
  danger = false,
  disabled = false,
  label,
  onClick,
}: {
  children: React.ReactNode;
  danger?: boolean;
  disabled?: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      aria-label={label}
      className={clsx(
        "grid size-8 place-items-center rounded-lg text-on-surface-variant transition-colors hover:bg-surface-highest hover:text-primary disabled:cursor-not-allowed disabled:opacity-45",
        danger && "hover:text-status-remove",
      )}
      disabled={disabled}
      onClick={onClick}
      title={label}
      type="button"
    >
      {children}
    </button>
  );
}
