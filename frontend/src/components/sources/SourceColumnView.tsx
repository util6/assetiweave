import clsx from "clsx";
import { FolderOpen, Pencil, Trash2 } from "lucide-react";
import { sourceKindLabel, sourceOriginLabel, translateScanStatus } from "../../i18n/domain";
import { useI18n } from "../../i18n/I18nProvider";
import { useAppSettings } from "../../store/settings/AppSettingsProvider";
import type { AppShortcut, Asset, AssetMountStatus, Source, TargetProfile } from "../../types";
import { getAssetMountSummaryState } from "../../utils/mountState";
import { isDirectMountBlockedSource } from "../../utils/mountPolicy";
import { abbreviateHomePath, displayAssetPath } from "../../utils/path";
import { kindBadgeClass } from "../../utils/styles";
import { MountStatePill } from "../assets/MountStatePill";
import { QuickMountButtons } from "../assets/QuickMountButtons";
import { SkillBackupBadge } from "../assets/SkillBackupBadge";
import { ResizableColumns } from "../layout/ResizableColumns";
import { SourceBulkMountControls } from "./SourceBulkMountControls";

export function SourceColumnView({
  appShortcuts,
  assetsBySourceId,
  busy,
  mountStatusesByAssetId,
  onAssetReveal,
  onDelete,
  onDeleteAsset,
  onEdit,
  onEditAsset,
  onReveal,
  onSelectSource,
  onSetSourceMountProfile,
  onToggleMount,
  profiles,
  selectedSource,
  sources,
}: {
  appShortcuts: AppShortcut[];
  assetsBySourceId: Map<string, Asset[]>;
  busy: boolean;
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  onAssetReveal: (path: string) => void;
  onDelete: (source: Source) => void;
  onDeleteAsset: (asset: Asset) => void;
  onEdit: (source: Source) => void;
  onEditAsset: (asset: Asset) => void;
  onReveal: (path: string) => void;
  onSelectSource: (sourceId: string) => void;
  onSetSourceMountProfile: (assetIds: string[], profileId: string, enabled: boolean) => void;
  onToggleMount: (assetId: string, profileId: string) => void;
  profiles: TargetProfile[];
  selectedSource: Source;
  sources: Source[];
}) {
  const { t } = useI18n();
  const { settings } = useAppSettings();
  const selectedAssets = assetsBySourceId.get(selectedSource.id) ?? [];
  const mountBlockedReason = isDirectMountBlockedSource(selectedSource) ? t("mount.blocked") : undefined;
  const hasVisibleMountShortcuts = appShortcuts.some((shortcut) => shortcut.enabled);

  return (
    <ResizableColumns
      ariaLabel={t("layout.resizeColumns")}
      className="min-h-[560px] overflow-hidden rounded-xl border border-theme-card-border bg-theme-card/70 shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.18)]"
      columns={[
        { defaultWeight: 0.72 },
        { defaultWeight: 0.9, minWidthScale: 1.1 },
        { defaultWeight: 1.45, minWidthScale: 1.45 },
      ]}
      handleClassName="max-[1120px]:hidden"
      minimumWidth={settings.columnMinWidth}
      responsiveClassName="max-[1120px]:w-full max-[1120px]:grid-cols-[minmax(240px,0.8fr)_minmax(0,1.2fr)]"
      scrollBarLabel={t("layout.scrollColumns")}
      scrollLeftLabel={t("layout.scrollColumnsLeft")}
      scrollRightLabel={t("layout.scrollColumnsRight")}
      storageKey="assetiweave.sourceColumns.v2"
    >
      <section className="flex min-h-0 flex-col border-r border-theme-card-border bg-theme-card-header/35">
        <ColumnHeader title={t("source.column.sources")} meta={t("source.column.sourceCount", { count: sources.length })} />
        <div className="min-h-0 overflow-y-auto py-1" role="listbox" aria-label={t("source.column.sources")}>
          {sources.map((source) => {
            const sourceAssets = assetsBySourceId.get(source.id) ?? [];
            const active = source.id === selectedSource.id;
            return (
              <button
                aria-label={t("source.column.selectSource", { name: source.name })}
                aria-selected={active}
                className={clsx(
                  "flex min-h-[68px] w-full items-start gap-3 border-l-2 px-3 py-3 text-left transition-colors",
                  active
                    ? "border-theme-nav-active-border bg-theme-nav-active/55 text-on-surface"
                    : "border-transparent text-on-surface-variant hover:bg-theme-control-hover hover:text-on-surface",
                )}
                key={source.id}
                onClick={() => onSelectSource(source.id)}
                role="option"
                type="button"
              >
                <span
                  className={clsx("mt-1 size-2 shrink-0 rounded-full", source.enabled ? "bg-status-create" : "bg-outline")}
                  aria-hidden="true"
                />
                <span className="min-w-0 flex-1">
                  <span className="block overflow-hidden text-ellipsis whitespace-nowrap font-mono text-code-md font-semibold">
                    {source.name}
                  </span>
                  <span className="mt-1 block overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm text-outline">
                    {abbreviateHomePath(source.root_path)}
                  </span>
                  <span className="mt-1 text-body-sm text-on-surface-variant">{t("source.assetCount", { count: sourceAssets.length })}</span>
                </span>
              </button>
            );
          })}
        </div>
      </section>

      <section className="flex min-h-0 flex-col border-r border-theme-card-border max-[1120px]:border-r-0">
        <ColumnHeader
          title={selectedSource.name}
          meta={t("source.assetCount", { count: selectedAssets.length })}
          actionLabel={t("source.action.reveal")}
          onAction={() => onReveal(selectedSource.root_path)}
        />
        <div className="min-h-0 overflow-y-auto">
          {selectedAssets.length === 0 ? (
            <div className="px-4 py-5 text-body-sm text-on-surface-variant">{t("source.emptySkills")}</div>
          ) : (
            selectedAssets.map((asset) => {
              const mountStatuses = mountStatusesByAssetId.get(asset.id) ?? [];
              return (
                <article
                  className="grid min-h-[88px] grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-b border-theme-card-border px-4 py-3 last:border-b-0 hover:bg-theme-card-header/70"
                  key={asset.id}
                >
                  <div className="min-w-0">
                    <div className="flex min-w-0 items-center gap-2">
                      <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-code-md font-semibold text-on-surface">
                        {asset.name}
                      </span>
                      <span className={kindBadgeClass(asset.kind)}>{t("assetKind.skill")}</span>
                      <SkillBackupBadge asset={asset} />
                      <MountStatePill compact state={getAssetMountSummaryState(mountStatuses)} />
                    </div>
                    <button
                      className="mt-1 block max-w-full overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm text-on-surface-variant transition-colors hover:text-primary"
                      onClick={() => onAssetReveal(asset.absolute_path)}
                      title={t("asset.revealPath")}
                      type="button"
                    >
                      {displayAssetPath(asset)}
                    </button>
                  </div>
                  <div className="inline-flex w-fit max-w-full shrink-0 flex-wrap items-center justify-end gap-2 rounded-xl border border-theme-control-border bg-theme-control/55 p-1.5 shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.38)]">
                    <QuickMountButtons
                      asset={asset}
                      mountBlockedReason={mountBlockedReason}
                      mountStatuses={mountStatuses}
                      profiles={profiles}
                      shortcuts={appShortcuts}
                      onToggle={(profileId) => onToggleMount(asset.id, profileId)}
                    />
                    {hasVisibleMountShortcuts && <span className="h-6 w-px bg-theme-control-border/80" aria-hidden="true" />}
                    <ColumnAssetIconButton label={t("asset.edit")} onClick={() => onEditAsset(asset)}>
                      <Pencil size={16} />
                    </ColumnAssetIconButton>
                    <ColumnAssetIconButton danger label={t("asset.delete")} onClick={() => onDeleteAsset(asset)}>
                      <Trash2 size={16} />
                    </ColumnAssetIconButton>
                  </div>
                </article>
              );
            })
          )}
        </div>
      </section>

      <section className="flex min-h-0 flex-col bg-theme-card-header/35 max-[1120px]:col-span-2 max-[1120px]:border-t max-[1120px]:border-theme-card-border">
        <ColumnHeader title={t("source.column.mountTargets")} meta={translateScanStatus(selectedSource.last_scan_status, t)} />
        <div className="min-h-0 overflow-y-auto p-4">
          <div className="mb-4 flex flex-wrap items-center gap-2">
            <button
              className="inline-flex h-9 items-center justify-center gap-2 rounded-lg border border-theme-control-border bg-theme-control px-3 text-body-sm font-semibold text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-primary disabled:cursor-not-allowed disabled:opacity-50"
              disabled={busy}
              onClick={() => onEdit(selectedSource)}
              type="button"
            >
              <Pencil size={15} />
              {t("source.action.edit")}
            </button>
            <button
              className="inline-flex h-9 items-center justify-center gap-2 rounded-lg border border-status-remove/45 bg-status-remove/10 px-3 text-body-sm font-semibold text-status-remove transition-colors hover:bg-status-remove/15 disabled:cursor-not-allowed disabled:opacity-50"
              disabled={busy || isProtectedSource(selectedSource)}
              onClick={() => onDelete(selectedSource)}
              type="button"
              title={isProtectedSource(selectedSource) ? t("source.delete.protected") : t("source.action.delete")}
            >
              <Trash2 size={15} />
              {isProtectedSource(selectedSource) ? t("source.delete.protected") : t("source.action.delete")}
            </button>
          </div>
          <SourceBulkMountControls
            appShortcuts={appShortcuts}
            assets={selectedAssets}
            busy={busy}
            mountStatusesByAssetId={mountStatusesByAssetId}
            onSetSourceMountProfile={onSetSourceMountProfile}
            profiles={profiles}
            source={selectedSource}
            variant="panel"
          />

          <div className="mt-4 space-y-3 rounded-xl border border-theme-card-border bg-theme-card/65 p-3">
            <SourceDetailRow label={t("source.field.kind")} value={sourceKindLabel(selectedSource.kind, t)} />
            <SourceDetailRow label={t("source.field.rootPath")} value={abbreviateHomePath(selectedSource.root_path)} mono />
            <SourceDetailRow label={t("source.field.origin")} value={sourceOriginLabel(selectedSource.source_origin, t)} />
            <RuleList label={t("source.rules.include")} rules={selectedSource.include_globs} />
            <RuleList label={t("source.rules.exclude")} rules={selectedSource.exclude_globs} />
          </div>
        </div>
      </section>
    </ResizableColumns>
  );
}

function ColumnAssetIconButton({
  children,
  danger = false,
  label,
  onClick,
}: {
  children: React.ReactNode;
  danger?: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      aria-label={label}
      className={clsx(
        "grid size-8 place-items-center rounded-lg text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-primary",
        danger && "hover:text-status-remove",
      )}
      onClick={onClick}
      title={label}
      type="button"
    >
      {children}
    </button>
  );
}

function isProtectedSource(source: Source) {
  return source.id === "assetiweave-library-skills" || source.source_origin === "assetiweave_library";
}

function ColumnHeader({
  actionLabel,
  meta,
  onAction,
  title,
}: {
  actionLabel?: string;
  meta: string;
  onAction?: () => void;
  title: string;
}) {
  return (
    <header className="flex min-h-14 items-center justify-between gap-3 border-b border-theme-card-border bg-theme-card-header/70 px-4 py-3">
      <div className="min-w-0">
        <h3 className="overflow-hidden text-ellipsis whitespace-nowrap text-body-md font-semibold text-on-surface">{title}</h3>
        <p className="mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm text-outline">{meta}</p>
      </div>
      {onAction && actionLabel && (
        <button
          aria-label={actionLabel}
          className="grid size-8 shrink-0 place-items-center rounded-lg text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-primary"
          onClick={onAction}
          title={actionLabel}
          type="button"
        >
          <FolderOpen size={16} />
        </button>
      )}
    </header>
  );
}

function SourceDetailRow({ label, mono = false, value }: { label: string; mono?: boolean; value: string }) {
  return (
    <div className="min-w-0">
      <div className="text-label-caps uppercase text-outline">{label}</div>
      <div className={clsx("mt-1 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm text-on-surface", mono && "font-mono")}>
        {value}
      </div>
    </div>
  );
}

function RuleList({ label, rules }: { label: string; rules: string[] }) {
  const { t } = useI18n();

  return (
    <div className="min-w-0">
      <div className="text-label-caps uppercase text-outline">{label}</div>
      {rules.length === 0 ? (
        <div className="mt-1 text-body-sm text-on-surface-variant">{t("source.rules.empty")}</div>
      ) : (
        <div className="mt-1 flex flex-wrap gap-1.5">
          {rules.map((rule) => (
            <span
              className="max-w-full overflow-hidden text-ellipsis whitespace-nowrap rounded-md border border-theme-control-border bg-theme-control px-2 py-0.5 font-mono text-body-sm text-on-surface-variant"
              key={rule}
            >
              {rule}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
