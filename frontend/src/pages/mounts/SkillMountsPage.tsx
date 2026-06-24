import clsx from "clsx";
import {
  Archive,
  Boxes,
  ChevronDown,
  ChevronRight,
  Columns3,
  FolderOpen,
  LayoutList,
  Pencil,
  Plus,
  RefreshCw,
  Settings,
  Trash2,
  X,
} from "lucide-react";
import { useEffect, useId, useMemo, useState, type FormEvent, type ReactNode } from "react";
import { useSkillBackup } from "../../app/backgroundTasks/SkillBackupProvider";
import { AssetToolbar, type AssetToolbarViewMode } from "../../components/assets/AssetToolbar";
import { MountStatePill } from "../../components/assets/MountStatePill";
import { QuickMountButtons } from "../../components/assets/QuickMountButtons";
import { SkillBackupBadge } from "../../components/assets/SkillBackupBadge";
import { AppShortcutIconForShortcut } from "../../components/apps/AppShortcutIcon";
import { SkillBackupLibraryDialog } from "../../components/backup/SkillBackupLibraryDialog";
import {
  isSkillBackupRunning,
  SkillBackupButtonContent,
} from "../../components/backup/SkillBackupProgress";
import { ConfirmDialog } from "../../components/common/ConfirmDialog";
import { PageMetrics } from "../../components/common/PageMetrics";
import { PathPickerInput } from "../../components/common/PathPickerInput";
import { DialogFrame } from "../../components/foundation/DialogFrame";
import { EmptyState as FoundationEmptyState } from "../../components/foundation/EmptyState";
import { PageHeader } from "../../components/foundation/PageHeader";
import { Panel as FoundationPanel } from "../../components/foundation/Panel";
import { ResizableColumns } from "../../components/layout/ResizableColumns";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { Switch } from "../../components/ui/switch";
import { assetKindLabel, sourceOriginLabel } from "../../i18n/domain";
import { useI18n } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import { ManualHelpButton } from "../../manuals/ManualHelpButton";
import { useAppSettings } from "../../store/settings/AppSettingsProvider";
import { DEFAULT_ENTITY_ACCENT_HEX } from "../../theme/themes";
import {
  createProfile,
  deleteProfile,
  listSkillGroups,
  selectTargetDirectory,
  type SkillBackupTaskSnapshot,
  updateProfile,
} from "../../services/catalog";
import type {
  AppKind,
  AppShortcut,
  Asset,
  AssetGroupDetail,
  AssetMountStatus,
  Source,
  TargetProfile,
} from "../../types";
import { getAssetMountSummaryState, groupMountStatusesByAssetId } from "../../utils/mountState";
import { isDirectMountBlockedSource } from "../../utils/mountPolicy";
import { abbreviateHomePath, displayAssetPath } from "../../utils/path";
import { isDefaultAppProfileId } from "../../utils/defaultApps";
import { buildTargetProfileInput, defaultAppShortcut, hasProfileIdConflict, targetProfileFromInput } from "../../utils/profile";
import { groupMemberAssetIds } from "../../utils/skillGroups";
import { kindBadgeClass } from "../../utils/styles";

type SkillMountViewMode = Extract<AssetToolbarViewMode, "list" | "columns">;
type MountScopeKind = "source" | "group";

interface MountScope {
  assetIds: string[];
  blockedReason?: string;
  description: string;
  id: string;
  kind: MountScopeKind;
  name: string;
}

interface SkillMountsPageProps {
  appShortcuts: AppShortcut[];
  assetMountStatuses: AssetMountStatus[];
  assets: Asset[];
  onNotifyError: (message: string) => void;
  onCatalogRefresh: () => Promise<void>;
  onManualOpen: () => void;
  onOpenSettings: () => void;
  onRefreshMountStatus: () => Promise<void>;
  onRefreshProfiles: () => Promise<void>;
  onRevealPath: (path: string) => void;
  onSaveAppShortcuts: (shortcuts: AppShortcut[]) => Promise<AppShortcut[]>;
  onSetSkillMountProfiles: (assetIds: string[], profileId: string, enabled: boolean) => Promise<void>;
  onToggleMount: (assetId: string, profileId: string) => void | Promise<void>;
  profiles: TargetProfile[];
  refreshingMountStatus: boolean;
  sources: Source[];
}

const appKinds: AppKind[] = ["custom", "codex", "claude", "cursor", "opencode", "gemini", "antigravity", "openclaw"];

interface PendingDefaultPathChange {
  editingProfile: TargetProfile;
  values: AppProfileDialogValues;
}

export function SkillMountsPage({
  appShortcuts,
  assetMountStatuses,
  assets,
  onNotifyError,
  onCatalogRefresh,
  onManualOpen,
  onOpenSettings,
  onRefreshMountStatus,
  onRefreshProfiles,
  onRevealPath,
  onSaveAppShortcuts,
  onSetSkillMountProfiles,
  onToggleMount,
  profiles,
  refreshingMountStatus,
  sources,
}: SkillMountsPageProps) {
  const { t } = useI18n();
  const { startBackup, task: backupTask } = useSkillBackup();
  const [query, setQuery] = useState("");
  const [viewMode, setViewMode] = useState<SkillMountViewMode>("list");
  const [groups, setGroups] = useState<AssetGroupDetail[]>([]);
  const [expandedProfileIds, setExpandedProfileIds] = useState<Set<string>>(new Set());
  const [selectedProfileId, setSelectedProfileId] = useState<string | null>(null);
  const [selectedScopeId, setSelectedScopeId] = useState<string | null>(null);
  const [dialogProfile, setDialogProfile] = useState<TargetProfile | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [backupDialogOpen, setBackupDialogOpen] = useState(false);
  const [deletingProfile, setDeletingProfile] = useState<TargetProfile | null>(null);
  const [pendingDefaultPathChange, setPendingDefaultPathChange] = useState<PendingDefaultPathChange | null>(null);
  const [busy, setBusy] = useState(false);

  const skillAssets = useMemo(() => assets.filter((asset) => asset.kind === "skill"), [assets]);
  const skillAssetById = useMemo(() => new Map(skillAssets.map((asset) => [asset.id, asset])), [skillAssets]);
  const sourceById = useMemo(() => new Map(sources.map((source) => [source.id, source])), [sources]);
  const mountStatusesByAssetId = useMemo(() => groupMountStatusesByAssetId(assetMountStatuses), [assetMountStatuses]);
  const scopes = useMemo(
    () => buildMountScopes({ groups, skillAssetById, sources, t }),
    [groups, skillAssetById, sources, t],
  );
  const filteredProfiles = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    if (!normalizedQuery) {
      return profiles;
    }

    return profiles.filter((profile) =>
      [profile.name, profile.id, profile.app_kind, profile.target_paths.join(" ")]
        .join(" ")
        .toLowerCase()
        .includes(normalizedQuery),
    );
  }, [profiles, query]);
  const selectedProfile = useMemo(
    () => filteredProfiles.find((profile) => profile.id === selectedProfileId) ?? filteredProfiles[0] ?? null,
    [filteredProfiles, selectedProfileId],
  );
  const selectedScope = useMemo(
    () => scopes.find((scope) => scope.id === selectedScopeId) ?? scopes[0] ?? null,
    [scopes, selectedScopeId],
  );

  useEffect(() => {
    void refreshGroups();
  }, []);

  async function refreshGroups() {
    setBusy(true);
    try {
      setGroups(await listSkillGroups());
    } catch (error) {
      onNotifyError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function handleSaveProfile(
    values: AppProfileDialogValues,
    editingProfile: TargetProfile | null,
    options: { confirmedDefaultPathChange?: boolean } = {},
  ) {
    if (
      editingProfile &&
      isDefaultAppProfileId(editingProfile.id) &&
      !options.confirmedDefaultPathChange &&
      hasTargetPathChanged(editingProfile, values.targetPath)
    ) {
      setPendingDefaultPathChange({ editingProfile, values });
      return;
    }

    setBusy(true);
    try {
      const input = buildTargetProfileInput(values, editingProfile);
      const existingProfile = profiles.find((profile) => profile.id === input.id);
      if (!editingProfile && input.id && hasProfileIdConflict(input.id, profiles)) {
        input.include = existingProfile?.include ?? input.include;
        input.exclude = existingProfile?.exclude ?? input.exclude;
        input.safety = existingProfile?.safety ?? input.safety;
        input.supported_kinds = existingProfile?.supported_kinds ?? input.supported_kinds;
      }

      const savedProfile =
        editingProfile || existingProfile
          ? await updateProfile({ ...(existingProfile ?? editingProfile)!, ...targetProfileFromInput(input) })
          : await createProfile(input);
      const shortcut = defaultAppShortcut(savedProfile, {
        accentColor: values.accentColor,
        displayIcon: values.displayIcon.trim() || defaultAppShortcut(savedProfile).displayIcon,
        enabled: values.shortcutEnabled,
      });
      await onSaveAppShortcuts([
        ...appShortcuts.filter((candidate) => candidate.profileId !== shortcut.profileId),
        shortcut,
      ]);
      await onRefreshProfiles();
      setDialogOpen(false);
      setDialogProfile(null);
      setSelectedProfileId(savedProfile.id);
    } catch (error) {
      onNotifyError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function handleDeleteProfile(profile: TargetProfile) {
    if (isDefaultAppProfileId(profile.id)) {
      onNotifyError(t("appMount.deleteDialog.defaultBlocked", { name: profile.name }));
      setDeletingProfile(null);
      return;
    }

    setBusy(true);
    try {
      await deleteProfile(profile.id);
      await onSaveAppShortcuts(appShortcuts.filter((shortcut) => shortcut.profileId !== profile.id));
      await onRefreshProfiles();
      setSelectedProfileId((current) => (current === profile.id ? null : current));
      setDeletingProfile(null);
    } catch (error) {
      onNotifyError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function handleBackupSkill(asset: Asset) {
    try {
      await startBackup([asset.id]);
    } catch (error) {
      onNotifyError(errorMessage(error));
    }
  }

  function toggleExpanded(profileId: string) {
    setExpandedProfileIds((current) => {
      const next = new Set(current);
      if (next.has(profileId)) {
        next.delete(profileId);
      } else {
        next.add(profileId);
      }
      return next;
    });
  }

  return (
    <section className="flex flex-1 flex-col gap-[var(--app-section-gap)] px-[var(--app-page-x)] py-[var(--app-page-y)]">
      <PageHeader
        actions={
          <PageMetrics
            metrics={[
              { label: t("appMount.metric.apps"), value: profiles.length },
              { label: t("appMount.metric.skills"), value: skillAssets.length },
            ]}
          />
        }
        eyebrow={t("appMount.page.subtitle")}
        icon={<Boxes size={21} />}
        title={t("appMount.page.title")}
        titleAction={<ManualHelpButton onOpen={onManualOpen} />}
      />

      <AssetToolbar
        actionGroups={[
          [
            {
              disabled: busy,
              icon: <Archive size={17} />,
              label: t("backup.action.open"),
              onClick: () => setBackupDialogOpen(true),
              text: t("backup.action.open"),
            },
            {
              disabled: busy,
              icon: <Plus size={17} />,
              label: t("appMount.action.import"),
              onClick: () => {
                setDialogProfile(null);
                setDialogOpen(true);
              },
              primary: true,
              text: t("appMount.action.import"),
            },
          ],
          [
            {
              disabled: busy || refreshingMountStatus,
              icon: <RefreshCw size={17} />,
              label: t("toolbar.refreshMountStatus"),
              onClick: () => void onRefreshMountStatus(),
            },
            { icon: <Settings size={17} />, label: t("toolbar.settings"), onClick: onOpenSettings },
          ],
        ]}
        ariaLabel={t("appMount.page.title")}
        onQueryChange={setQuery}
        onViewModeChange={setViewMode}
        query={query}
        searchClassName="w-72 max-[1160px]:w-full"
        searchPlaceholder={t("appMount.searchPlaceholder")}
        sticky
        stickyBleed
        viewAriaLabel={t("toolbar.view.aria")}
        viewMode={viewMode}
        viewOptions={[
          { icon: <LayoutList size={17} />, label: t("toolbar.view.list"), value: "list" },
          { icon: <Columns3 size={17} />, label: t("toolbar.view.columns"), value: "columns" },
        ]}
      />

      {filteredProfiles.length === 0 ? (
        <EmptyState>{t("appMount.empty")}</EmptyState>
      ) : viewMode === "columns" && selectedProfile && selectedScope ? (
        <AppMountColumnView
          appShortcuts={appShortcuts}
          backupTask={backupTask}
          busy={busy}
          mountStatusesByAssetId={mountStatusesByAssetId}
          onEditProfile={(profile) => {
            setDialogProfile(profile);
            setDialogOpen(true);
          }}
          onDeleteProfile={setDeletingProfile}
          onBackupSkill={handleBackupSkill}
          onRevealPath={onRevealPath}
          onSelectProfile={setSelectedProfileId}
          onSelectScope={setSelectedScopeId}
          onSetSkillMountProfiles={onSetSkillMountProfiles}
          onToggleMount={onToggleMount}
          profiles={filteredProfiles}
          scopes={scopes}
          selectedProfile={selectedProfile}
          selectedScope={selectedScope}
          skillAssetById={skillAssetById}
          sourceById={sourceById}
        />
      ) : (
        <FoundationPanel
          className="overflow-hidden"
          padding="none"
          aria-label={t("appMount.page.title")}
        >
          {filteredProfiles.map((profile) => (
            <AppMountRow
              appShortcuts={appShortcuts}
              backupTask={backupTask}
              busy={busy}
              expanded={expandedProfileIds.has(profile.id)}
              key={profile.id}
              mountStatusesByAssetId={mountStatusesByAssetId}
              onDelete={() => setDeletingProfile(profile)}
              onBackupSkill={handleBackupSkill}
              onEdit={() => {
                setDialogProfile(profile);
                setDialogOpen(true);
              }}
              onReveal={() => onRevealPath(profile.target_paths[0] ?? "")}
              onSetSkillMountProfiles={onSetSkillMountProfiles}
              onToggleExpanded={() => toggleExpanded(profile.id)}
              onToggleMount={onToggleMount}
              profile={profile}
              scopes={scopes}
              skillAssetById={skillAssetById}
              sourceById={sourceById}
            />
          ))}
        </FoundationPanel>
      )}

      <AppProfileDialog
        appShortcuts={appShortcuts}
        busy={busy}
        onClose={() => {
          setDialogOpen(false);
          setDialogProfile(null);
        }}
        onPickTargetPath={() => selectTargetDirectory(t("appMount.dialog.pickTarget"))}
        onSubmit={handleSaveProfile}
        open={dialogOpen}
        profile={dialogProfile}
      />
      <SkillBackupLibraryDialog
        onClose={() => setBackupDialogOpen(false)}
        onNotifyError={onNotifyError}
        onSaved={onCatalogRefresh}
        open={backupDialogOpen}
      />
      <ConfirmDialog
        busy={busy}
        confirmLabel={t("common.delete")}
        message={deletingProfile ? t("appMount.deleteDialog.message", { name: deletingProfile.name }) : ""}
        onClose={() => setDeletingProfile(null)}
        onConfirm={() => deletingProfile && void handleDeleteProfile(deletingProfile)}
        open={Boolean(deletingProfile)}
        title={t("appMount.deleteDialog.title")}
        tone="danger"
      >
        <FoundationPanel className="text-body-sm text-on-surface-variant" padding="sm" variant="muted">
          {deletingProfile && isDefaultAppProfileId(deletingProfile.id)
            ? t("appMount.deleteDialog.defaultDetail")
            : t("appMount.deleteDialog.detail")}
        </FoundationPanel>
      </ConfirmDialog>
      <ConfirmDialog
        busy={busy}
        confirmLabel={t("common.save")}
        message={
          pendingDefaultPathChange
            ? t("appMount.pathChangeDialog.message", { name: pendingDefaultPathChange.editingProfile.name })
            : ""
        }
        onClose={() => setPendingDefaultPathChange(null)}
        onConfirm={() => {
          if (!pendingDefaultPathChange) {
            return;
          }
          void handleSaveProfile(pendingDefaultPathChange.values, pendingDefaultPathChange.editingProfile, {
            confirmedDefaultPathChange: true,
          });
          setPendingDefaultPathChange(null);
        }}
        open={Boolean(pendingDefaultPathChange)}
        title={t("appMount.pathChangeDialog.title")}
      >
        <FoundationPanel className="text-body-sm text-on-surface-variant" padding="sm" variant="muted">
          {pendingDefaultPathChange
            ? t("appMount.pathChangeDialog.detail", {
                nextPath: abbreviateHomePath(pendingDefaultPathChange.values.targetPath.trim()),
                previousPath: abbreviateHomePath(pendingDefaultPathChange.editingProfile.target_paths[0] ?? ""),
              })
            : ""}
        </FoundationPanel>
      </ConfirmDialog>
    </section>
  );
}

function AppMountRow({
  appShortcuts,
  backupTask,
  busy,
  expanded,
  mountStatusesByAssetId,
  onBackupSkill,
  onDelete,
  onEdit,
  onReveal,
  onSetSkillMountProfiles,
  onToggleExpanded,
  onToggleMount,
  profile,
  scopes,
  skillAssetById,
  sourceById,
}: {
  appShortcuts: AppShortcut[];
  backupTask: SkillBackupTaskSnapshot | null;
  busy: boolean;
  expanded: boolean;
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  onBackupSkill: (asset: Asset) => Promise<void>;
  onDelete: () => void;
  onEdit: () => void;
  onReveal: () => void;
  onSetSkillMountProfiles: (assetIds: string[], profileId: string, enabled: boolean) => Promise<void>;
  onToggleExpanded: () => void;
  onToggleMount: (assetId: string, profileId: string) => void | Promise<void>;
  profile: TargetProfile;
  scopes: MountScope[];
  skillAssetById: Map<string, Asset>;
  sourceById: Map<string, Source>;
}) {
  const { t } = useI18n();
  const skillAssets = [...skillAssetById.values()];
  const counts = getProfileMountCounts(profile.id, skillAssets, mountStatusesByAssetId);
  const shortcut = shortcutForProfile(profile, appShortcuts);
  const summaryState = counts.conflict > 0 ? "conflict" : counts.broken > 0 ? "broken" : counts.mounted > 0 ? "mounted" : "not_mounted";
  const defaultApp = isDefaultAppProfileId(profile.id);

  return (
    <article className={clsx("border-b border-theme-card-border last:border-b-0", expanded && "bg-theme-card-header/45")}>
      <div className="grid min-h-20 grid-cols-[minmax(0,1fr)_auto] items-center gap-4 px-4 py-3.5 hover:bg-theme-card-header/70 max-[860px]:grid-cols-1">
        <div className="flex min-w-0 items-start gap-3">
          <span
            className="grid size-10 shrink-0 place-items-center rounded-xl border text-[13px] font-bold"
            style={{
              backgroundColor: `${shortcut.accentColor}18`,
              borderColor: `${shortcut.accentColor}66`,
              color: shortcut.accentColor,
            }}
          >
            <AppShortcutIconForShortcut className="size-5" shortcut={shortcut} />
          </span>
          <div className="min-w-0">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <h3 className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-code-md font-semibold text-on-surface">
                {profile.name}
              </h3>
              <span className="rounded-md border border-theme-control-border bg-theme-control px-2 py-0.5 text-label-caps uppercase text-on-surface-variant">
                {profile.app_kind}
              </span>
              <span
                className={clsx(
                  "rounded-md px-2 py-0.5 text-label-caps uppercase",
                  profile.enabled ? "bg-status-create/15 text-status-create" : "bg-theme-control-hover text-outline",
                )}
              >
                {profile.enabled ? t("appMount.status.enabled") : t("appMount.status.disabled")}
              </span>
              <MountStatePill compact state={summaryState} />
            </div>
            <button
              className="mt-2 block max-w-full overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm text-on-surface-variant transition-colors hover:text-primary"
              onClick={onReveal}
              title={t("appMount.action.reveal")}
              type="button"
            >
              {abbreviateHomePath(profile.target_paths[0] ?? "")}
            </button>
          </div>
        </div>

        <div className="flex items-center justify-end gap-3 max-[860px]:justify-start">
          <MountCountBadge count={counts.mounted} label={t("appMount.metric.mounted")} total={counts.total} />
          <MountCountBadge count={counts.conflict + counts.broken} label={t("appMount.metric.issues")} total={counts.total} />
          <IconAction disabled={busy} label={t("appMount.action.edit")} onClick={onEdit}>
            <Pencil size={16} />
          </IconAction>
          <IconAction label={t("appMount.action.reveal")} onClick={onReveal}>
            <FolderOpen size={16} />
          </IconAction>
          <IconAction danger disabled={busy || defaultApp} label={t("appMount.action.delete")} onClick={onDelete}>
            <Trash2 size={16} />
          </IconAction>
          <IconAction label={t(expanded ? "appMount.action.collapse" : "appMount.action.expand")} onClick={onToggleExpanded}>
            {expanded ? <ChevronDown size={17} /> : <ChevronRight size={17} />}
          </IconAction>
        </div>
      </div>

      {expanded && (
        <div className="border-t border-theme-card-border bg-theme-card-header/35 p-4">
          <AppMountWorkbench
            appShortcuts={appShortcuts}
            backupTask={backupTask}
            busy={busy}
            mountStatusesByAssetId={mountStatusesByAssetId}
            onBackupSkill={onBackupSkill}
            onSetSkillMountProfiles={onSetSkillMountProfiles}
            onToggleMount={onToggleMount}
            profile={profile}
            scopes={scopes}
            skillAssetById={skillAssetById}
            sourceById={sourceById}
          />
        </div>
      )}
    </article>
  );
}

function AppMountColumnView({
  appShortcuts,
  backupTask,
  busy,
  mountStatusesByAssetId,
  onBackupSkill,
  onEditProfile,
  onDeleteProfile,
  onRevealPath,
  onSelectProfile,
  onSelectScope,
  onSetSkillMountProfiles,
  onToggleMount,
  profiles,
  scopes,
  selectedProfile,
  selectedScope,
  skillAssetById,
  sourceById,
}: {
  appShortcuts: AppShortcut[];
  backupTask: SkillBackupTaskSnapshot | null;
  busy: boolean;
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  onBackupSkill: (asset: Asset) => Promise<void>;
  onEditProfile: (profile: TargetProfile) => void;
  onDeleteProfile: (profile: TargetProfile) => void;
  onRevealPath: (path: string) => void;
  onSelectProfile: (profileId: string) => void;
  onSelectScope: (scopeId: string) => void;
  onSetSkillMountProfiles: (assetIds: string[], profileId: string, enabled: boolean) => Promise<void>;
  onToggleMount: (assetId: string, profileId: string) => void | Promise<void>;
  profiles: TargetProfile[];
  scopes: MountScope[];
  selectedProfile: TargetProfile;
  selectedScope: MountScope;
  skillAssetById: Map<string, Asset>;
  sourceById: Map<string, Source>;
}) {
  const { t } = useI18n();
  const { settings } = useAppSettings();
  const scopeAssets = selectedScope.assetIds.flatMap((assetId) => {
    const asset = skillAssetById.get(assetId);
    return asset ? [asset] : [];
  });
  const shortcut = shortcutForProfile(selectedProfile, appShortcuts);
  const defaultSelectedApp = isDefaultAppProfileId(selectedProfile.id);

  return (
    <FoundationPanel className="overflow-visible" padding="none">
      <ResizableColumns
        ariaLabel={t("layout.resizeColumns")}
        className="min-h-[560px]"
        columns={[
          { defaultWeight: 0.7 },
          { defaultWeight: 0.85, minWidthScale: 1.1 },
          { defaultWeight: 1.5, minWidthScale: 1.5 },
        ]}
        handleClassName="max-[1120px]:hidden"
        minimumWidth={settings.columnMinWidth}
        responsiveClassName="max-[1120px]:w-full max-[1120px]:grid-cols-[minmax(240px,0.8fr)_minmax(0,1.2fr)]"
        scrollBarLabel={t("layout.scrollColumns")}
        scrollLeftLabel={t("layout.scrollColumnsLeft")}
        scrollRightLabel={t("layout.scrollColumnsRight")}
        storageKey="assetiweave.mountColumns.v2"
      >
        <section className="flex min-h-0 flex-col border-r border-theme-card-border bg-theme-card-header/35">
          <ColumnHeader meta={t("appMount.metric.appsWithCount", { count: profiles.length })} title={t("appMount.column.apps")} />
          <div className="min-h-0 overflow-y-auto py-1" role="listbox" aria-label={t("appMount.column.apps")}>
            {profiles.map((profile) => {
              const active = profile.id === selectedProfile.id;
              const profileShortcut = shortcutForProfile(profile, appShortcuts);
              return (
                <button
                  aria-label={t("appMount.column.selectApp", { name: profile.name })}
                  aria-selected={active}
                  className={clsx(
                    "flex min-h-[72px] w-full items-start gap-3 border-l-2 px-3 py-3 text-left transition-colors",
                    active
                      ? "border-primary bg-primary/10 text-on-surface"
                      : "border-transparent text-on-surface-variant hover:bg-theme-control-hover hover:text-on-surface",
                  )}
                  key={profile.id}
                  onClick={() => onSelectProfile(profile.id)}
                  role="option"
                  type="button"
                >
                  <span
                    className="mt-0.5 grid size-9 shrink-0 place-items-center rounded-lg border"
                    style={{
                      backgroundColor: `${profileShortcut.accentColor}18`,
                      borderColor: `${profileShortcut.accentColor}66`,
                      color: profileShortcut.accentColor,
                    }}
                  >
                    <AppShortcutIconForShortcut className="size-4" shortcut={profileShortcut} />
                  </span>
                  <span className="min-w-0">
                    <span className="block overflow-hidden text-ellipsis whitespace-nowrap font-mono text-code-md font-semibold">
                      {profile.name}
                    </span>
                    <span className="mt-1 block overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm text-outline">
                      {abbreviateHomePath(profile.target_paths[0] ?? "")}
                    </span>
                  </span>
                </button>
              );
            })}
          </div>
        </section>

        <section className="flex min-h-0 flex-col border-r border-theme-card-border max-[1120px]:border-r-0">
          <ColumnHeader
            actionIcon={<Pencil size={16} />}
            actionLabel={t("appMount.action.edit")}
            dangerActionIcon={<Trash2 size={16} />}
            dangerActionLabel={t("appMount.action.delete")}
            dangerActionDisabled={defaultSelectedApp}
            meta={abbreviateHomePath(selectedProfile.target_paths[0] ?? "")}
            onAction={() => onEditProfile(selectedProfile)}
            onDangerAction={() => onDeleteProfile(selectedProfile)}
            title={selectedProfile.name}
          />
          <ScopeList
            mountStatusesByAssetId={mountStatusesByAssetId}
            onSelectScope={onSelectScope}
            profile={selectedProfile}
            scopes={scopes}
            selectedScopeId={selectedScope.id}
            skillAssetById={skillAssetById}
          />
        </section>

        <section className="flex min-h-0 flex-col bg-theme-card-header/35 max-[1120px]:col-span-2 max-[1120px]:border-t max-[1120px]:border-theme-card-border">
          <ColumnHeader
            actionLabel={t("appMount.action.reveal")}
            actionIcon={<FolderOpen size={16} />}
            meta={t("appMount.scope.assetCount", { count: scopeAssets.length })}
            onAction={() => onRevealPath(selectedProfile.target_paths[0] ?? "")}
            title={selectedScope.name}
          />
          <div className="border-b border-theme-card-border p-3">
            <ScopeBatchActions
              busy={busy}
              mountStatusesByAssetId={mountStatusesByAssetId}
              onSetSkillMountProfiles={onSetSkillMountProfiles}
              profile={selectedProfile}
              scope={selectedScope}
              skillAssetById={skillAssetById}
            />
          </div>
          <SkillScopeAssetList
            backupTask={backupTask}
            busy={busy}
            mountStatusesByAssetId={mountStatusesByAssetId}
            onBackupSkill={onBackupSkill}
            onToggleMount={onToggleMount}
            profile={selectedProfile}
            shortcut={shortcut}
            skillAssets={scopeAssets}
            sourceById={sourceById}
          />
        </section>
      </ResizableColumns>
    </FoundationPanel>
  );
}

function AppMountWorkbench({
  appShortcuts,
  backupTask,
  busy,
  mountStatusesByAssetId,
  onBackupSkill,
  onSetSkillMountProfiles,
  onToggleMount,
  profile,
  scopes,
  skillAssetById,
  sourceById,
}: {
  appShortcuts: AppShortcut[];
  backupTask: SkillBackupTaskSnapshot | null;
  busy: boolean;
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  onBackupSkill: (asset: Asset) => Promise<void>;
  onSetSkillMountProfiles: (assetIds: string[], profileId: string, enabled: boolean) => Promise<void>;
  onToggleMount: (assetId: string, profileId: string) => void | Promise<void>;
  profile: TargetProfile;
  scopes: MountScope[];
  skillAssetById: Map<string, Asset>;
  sourceById: Map<string, Source>;
}) {
  const { t } = useI18n();
  const { settings } = useAppSettings();
  const [selectedScopeId, setSelectedScopeId] = useState<string | null>(null);
  const selectedScope = scopes.find((scope) => scope.id === selectedScopeId) ?? scopes[0] ?? null;
  const scopeAssets = selectedScope
    ? selectedScope.assetIds.flatMap((assetId) => {
        const asset = skillAssetById.get(assetId);
        return asset ? [asset] : [];
      })
    : [];

  if (!selectedScope) {
    return <EmptyState>{t("appMount.scope.empty")}</EmptyState>;
  }

  return (
    <FoundationPanel className="overflow-visible" padding="none" variant="muted">
      <ResizableColumns
        ariaLabel={t("layout.resizeColumns")}
        className="min-h-[420px]"
        columns={[
          { defaultWeight: 0.8, minWidthScale: 1.05 },
          { defaultWeight: 1.45, minWidthScale: 1.55 },
        ]}
        handleClassName="max-[960px]:hidden"
        minimumWidth={settings.columnMinWidth}
        responsiveClassName="max-[960px]:w-full max-[960px]:grid-cols-1"
        scrollBarLabel={t("layout.scrollColumns")}
        scrollLeftLabel={t("layout.scrollColumnsLeft")}
        scrollRightLabel={t("layout.scrollColumnsRight")}
        storageKey="assetiweave.mountWorkbenchColumns.v2"
      >
        <section className="flex min-h-0 flex-col border-r border-theme-card-border max-[960px]:border-r-0 max-[960px]:border-b">
          <ColumnHeader meta={t("appMount.scope.count", { count: scopes.length })} title={t("appMount.column.scopes")} />
          <ScopeList
            mountStatusesByAssetId={mountStatusesByAssetId}
            onSelectScope={setSelectedScopeId}
            profile={profile}
            scopes={scopes}
            selectedScopeId={selectedScope.id}
            skillAssetById={skillAssetById}
          />
        </section>
        <section className="flex min-h-0 flex-col">
          <ColumnHeader meta={t("appMount.scope.assetCount", { count: scopeAssets.length })} title={selectedScope.name} />
          <div className="border-b border-theme-card-border p-3">
            <ScopeBatchActions
              busy={busy}
              mountStatusesByAssetId={mountStatusesByAssetId}
              onSetSkillMountProfiles={onSetSkillMountProfiles}
              profile={profile}
              scope={selectedScope}
              skillAssetById={skillAssetById}
            />
          </div>
          <SkillScopeAssetList
            backupTask={backupTask}
            busy={busy}
            mountStatusesByAssetId={mountStatusesByAssetId}
            onBackupSkill={onBackupSkill}
            onToggleMount={onToggleMount}
            profile={profile}
            shortcut={shortcutForProfile(profile, appShortcuts)}
            skillAssets={scopeAssets}
            sourceById={sourceById}
          />
        </section>
      </ResizableColumns>
    </FoundationPanel>
  );
}

function ScopeList({
  mountStatusesByAssetId,
  onSelectScope,
  profile,
  scopes,
  selectedScopeId,
  skillAssetById,
}: {
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  onSelectScope: (scopeId: string) => void;
  profile: TargetProfile;
  scopes: MountScope[];
  selectedScopeId: string;
  skillAssetById: Map<string, Asset>;
}) {
  const { t } = useI18n();

  return (
    <div className="min-h-0 overflow-y-auto py-1" role="listbox" aria-label={t("appMount.column.scopes")}>
      {scopes.map((scope) => {
        const active = scope.id === selectedScopeId;
        const skillAssets = scope.assetIds.flatMap((assetId) => {
          const asset = skillAssetById.get(assetId);
          return asset ? [asset] : [];
        });
        const counts = getProfileMountCounts(profile.id, skillAssets, mountStatusesByAssetId);
        return (
          <button
            aria-label={t("appMount.column.selectScope", { name: scope.name })}
            aria-selected={active}
            className={clsx(
              "flex min-h-[76px] w-full items-start justify-between gap-3 border-l-2 px-3 py-3 text-left transition-colors",
              active
                ? "border-primary bg-primary/10 text-on-surface"
                : "border-transparent text-on-surface-variant hover:bg-theme-control-hover hover:text-on-surface",
            )}
            key={scope.id}
            onClick={() => onSelectScope(scope.id)}
            role="option"
            type="button"
          >
            <span className="min-w-0">
              <span className="block overflow-hidden text-ellipsis whitespace-nowrap font-mono text-code-md font-semibold">
                {scope.name}
              </span>
              <span className="mt-1 block overflow-hidden text-ellipsis whitespace-nowrap text-body-sm text-outline">
                {scope.description}
              </span>
              <span className="mt-1 text-body-sm text-on-surface-variant">
                {t("appMount.scope.mountProgress", { selected: counts.mounted, total: counts.total })}
              </span>
            </span>
            <span className="mt-1 rounded-md border border-theme-control-border bg-theme-control px-2 py-0.5 text-label-caps uppercase text-on-surface-variant">
              {t(`appMount.scope.${scope.kind}` as TranslationKey)}
            </span>
          </button>
        );
      })}
    </div>
  );
}

function ScopeBatchActions({
  busy,
  mountStatusesByAssetId,
  onSetSkillMountProfiles,
  profile,
  scope,
  skillAssetById,
}: {
  busy: boolean;
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  onSetSkillMountProfiles: (assetIds: string[], profileId: string, enabled: boolean) => Promise<void>;
  profile: TargetProfile;
  scope: MountScope;
  skillAssetById: Map<string, Asset>;
}) {
  const { t } = useI18n();
  const skillAssets = scope.assetIds.flatMap((assetId) => {
    const asset = skillAssetById.get(assetId);
    return asset ? [asset] : [];
  });
  const counts = getProfileMountCounts(profile.id, skillAssets, mountStatusesByAssetId);
  const allMounted = counts.total > 0 && counts.mounted === counts.total;
  const disabled = busy || counts.total === 0 || Boolean(scope.blockedReason);
  const label = scope.blockedReason ?? t(allMounted ? "appMount.action.unmountScope" : "appMount.action.mountScope", { profile: profile.name });

  return (
    <div className="flex flex-wrap items-center justify-between gap-3">
      <div className="min-w-0">
        <div className="text-label-caps uppercase text-outline">{t("appMount.scope.batch")}</div>
        <div className="mt-1 text-body-sm text-on-surface-variant">
          {scope.blockedReason ?? t("appMount.scope.mountProgress", { selected: counts.mounted, total: counts.total })}
        </div>
      </div>
      <Button
        disabled={disabled}
        onClick={() => void onSetSkillMountProfiles(scope.assetIds, profile.id, !allMounted)}
        type="button"
        variant={allMounted ? "outline" : "default"}
      >
        {label}
      </Button>
    </div>
  );
}

function SkillScopeAssetList({
  backupTask,
  busy,
  mountStatusesByAssetId,
  onBackupSkill,
  onToggleMount,
  profile,
  shortcut,
  skillAssets,
  sourceById,
}: {
  backupTask: SkillBackupTaskSnapshot | null;
  busy: boolean;
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  onBackupSkill: (asset: Asset) => Promise<void>;
  onToggleMount: (assetId: string, profileId: string) => void | Promise<void>;
  profile: TargetProfile;
  shortcut: AppShortcut;
  skillAssets: Asset[];
  sourceById: Map<string, Source>;
}) {
  const { t } = useI18n();

  if (skillAssets.length === 0) {
    return <div className="px-4 py-5 text-body-sm text-on-surface-variant">{t("appMount.scope.noSkills")}</div>;
  }

  return (
    <div className="min-h-0 overflow-y-auto">
      {skillAssets.map((asset) => {
        const source = sourceById.get(asset.source_id);
        const mountStatuses = mountStatusesByAssetId.get(asset.id) ?? [];
        const profileMountStatuses = mountStatuses.filter((status) => status.profile_id === profile.id);
        const mountBlocked = isDirectMountBlockedSource(source);
        const mountBlockedReason = mountBlocked ? t("mount.blocked") : undefined;
        return (
          <article
            className="grid min-h-[88px] grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-b border-theme-card-border px-4 py-3 last:border-b-0 hover:bg-theme-card-header/70 max-[760px]:grid-cols-1"
            key={asset.id}
          >
            <div className="min-w-0">
              <div className="flex min-w-0 items-center gap-2">
                <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-code-md font-semibold text-on-surface">
                  {asset.name}
                </span>
                <span className={kindBadgeClass(asset.kind)}>{assetKindLabel(asset.kind, t)}</span>
                <SkillBackupBadge asset={asset} />
                <MountStatePill compact state={getAssetMountSummaryState(profileMountStatuses)} />
              </div>
              <div className="mt-1 flex min-w-0 items-center gap-2">
                <span className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm text-on-surface-variant">
                  {displayAssetPath(asset)}
                </span>
                {source && (
                  <span className="shrink-0 rounded-md border border-theme-control-border bg-theme-control px-2 py-0.5 text-label-caps uppercase text-on-surface-variant">
                    {sourceOriginLabel(source.source_origin, t)}
                  </span>
                )}
              </div>
            </div>
            <div className="flex items-center justify-end gap-2 max-[760px]:justify-start">
              <QuickMountButtons
                asset={asset}
                mountBlockedReason={mountBlockedReason}
                mountStatuses={profileMountStatuses}
                onToggle={() => void onToggleMount(asset.id, profile.id)}
                profiles={[profile]}
                shortcuts={[{ ...shortcut, enabled: true }]}
              />
              {mountBlocked && (
                <Button
                  disabled={busy || isSkillBackupRunning(backupTask)}
                  onClick={() => void onBackupSkill(asset)}
                  type="button"
                  variant="outline"
                >
                  <SkillBackupButtonContent
                    assetIds={[asset.id]}
                    defaultLabel={t("backup.action.backup")}
                    task={backupTask}
                    t={t}
                  />
                </Button>
              )}
            </div>
          </article>
        );
      })}
    </div>
  );
}

interface AppProfileDialogValues {
  accentColor: string;
  appKind: AppKind;
  displayIcon: string;
  enabled: boolean;
  name: string;
  shortcutEnabled: boolean;
  targetPath: string;
}

function AppProfileDialog({
  appShortcuts,
  busy,
  onClose,
  onPickTargetPath,
  onSubmit,
  open,
  profile,
}: {
  appShortcuts: AppShortcut[];
  busy: boolean;
  onClose: () => void;
  onPickTargetPath: () => Promise<string | null>;
  onSubmit: (values: AppProfileDialogValues, editingProfile: TargetProfile | null) => Promise<void>;
  open: boolean;
  profile: TargetProfile | null;
}) {
  const { t } = useI18n();
  const formId = useId();
  const shortcut = profile ? (appShortcuts.find((candidate) => candidate.profileId === profile.id) ?? null) : null;
  const [values, setValues] = useState<AppProfileDialogValues>(() => initialDialogValues(profile, shortcut));
  const [picking, setPicking] = useState(false);

  useEffect(() => {
    if (open) {
      setValues(initialDialogValues(profile, shortcut));
      setPicking(false);
    }
  }, [open, profile, shortcut]);

  if (!open) {
    return null;
  }

  function updateValue<Key extends keyof AppProfileDialogValues>(key: Key, value: AppProfileDialogValues[Key]) {
    setValues((current) => ({ ...current, [key]: value }));
  }

  async function handlePickTargetPath() {
    setPicking(true);
    try {
      const selectedPath = await onPickTargetPath();
      if (selectedPath) {
        updateValue("targetPath", abbreviateHomePath(selectedPath));
      }
    } finally {
      setPicking(false);
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await onSubmit(values, profile);
  }

  return (
    <DialogFrame
      busy={busy}
      closeLabel={t("appMount.dialog.close")}
      contentClassName="p-0"
      footer={
        <>
          <Button disabled={busy} onClick={onClose} type="button" variant="outline">
            {t("appMount.dialog.cancel")}
          </Button>
          <Button disabled={busy || !values.name.trim() || !values.targetPath.trim()} form={formId} type="submit">
            {busy ? t("appMount.dialog.saving") : t("appMount.dialog.save")}
          </Button>
        </>
      }
      icon={<Boxes size={18} />}
      iconClassName="border-status-update/25 bg-status-update/15 text-status-update"
      onClose={onClose}
      overlayClassName="z-40 px-6 py-8"
      size="lg"
      title={profile ? t("appMount.dialog.editTitle") : t("appMount.dialog.importTitle")}
    >
        <form className="px-5 py-5" id={formId} onSubmit={(event) => void handleSubmit(event)}>
          <div className="grid gap-4">
            <div className="grid grid-cols-[minmax(0,1fr)_12rem] gap-3 max-[720px]:grid-cols-1">
              <Field label={t("appMount.field.name")} required>
                <Input
                  disabled={busy}
                  onChange={(event) => updateValue("name", event.target.value)}
                  placeholder={t("appMount.dialog.namePlaceholder")}
                  value={values.name}
                />
              </Field>
              <Field label={t("appMount.field.appKind")}>
                <select
                  className="h-9 rounded-lg border border-theme-control-border bg-theme-control px-3 text-body-sm text-on-surface outline-none focus:border-primary-strong/60 disabled:opacity-50"
                  disabled={busy}
                  onChange={(event) => updateValue("appKind", event.target.value as AppKind)}
                  value={values.appKind}
                >
                  {appKinds.map((appKind) => (
                    <option key={appKind} value={appKind}>
                      {appKind}
                    </option>
                  ))}
                </select>
              </Field>
            </div>

            <Field label={t("appMount.field.targetPath")} required>
              <PathPickerInput
                disabled={busy}
                onChange={(event) => updateValue("targetPath", event.target.value)}
                onPick={() => void handlePickTargetPath()}
                pickLabel={t("appMount.dialog.pickTarget")}
                picking={picking}
                placeholder={t("appMount.dialog.targetPlaceholder")}
                value={values.targetPath}
              />
            </Field>

            <div className="grid grid-cols-[minmax(0,1fr)_9rem] gap-3 max-[720px]:grid-cols-1">
              <Field label={t("appMount.field.displayIcon")}>
                <Input
                  disabled={busy}
                  maxLength={48}
                  onChange={(event) => updateValue("displayIcon", event.target.value)}
                  placeholder={t("appMount.dialog.iconPlaceholder")}
                  value={values.displayIcon}
                />
              </Field>
              <Field label={t("appMount.field.accentColor")}>
                <div className="flex gap-2">
                  <Input
                    aria-label={t("appMount.field.accentColor")}
                    className="w-12 px-1"
                    disabled={busy}
                    onChange={(event) => updateValue("accentColor", event.target.value)}
                    type="color"
                    value={values.accentColor}
                  />
                  <Input
                    className="min-w-0 flex-1 font-mono"
                    disabled={busy}
                    onChange={(event) => updateValue("accentColor", event.target.value)}
                    value={values.accentColor}
                  />
                </div>
              </Field>
            </div>

            <ToggleRow
              checked={values.enabled}
              disabled={busy}
              label={t("appMount.field.enabled")}
              onChange={(checked) => updateValue("enabled", checked)}
            />
            <ToggleRow
              checked={values.shortcutEnabled}
              disabled={busy}
              label={t("appMount.field.shortcutEnabled")}
              onChange={(checked) => updateValue("shortcutEnabled", checked)}
            />
          </div>

        </form>
    </DialogFrame>
  );
}

function initialDialogValues(profile: TargetProfile | null, shortcut: AppShortcut | null): AppProfileDialogValues {
  const defaultShortcut = profile ? defaultAppShortcut(profile) : null;
  return {
    accentColor: shortcut?.accentColor ?? defaultShortcut?.accentColor ?? DEFAULT_ENTITY_ACCENT_HEX,
    appKind: profile?.app_kind ?? "custom",
    displayIcon: shortcut?.displayIcon ?? defaultShortcut?.displayIcon ?? "",
    enabled: profile?.enabled ?? true,
    name: profile?.name ?? "",
    shortcutEnabled: shortcut?.enabled ?? true,
    targetPath: profile?.target_paths[0] ? abbreviateHomePath(profile.target_paths[0]) : "",
  };
}

function hasTargetPathChanged(profile: TargetProfile, nextTargetPath: string) {
  return (profile.target_paths[0] ?? "").trim() !== nextTargetPath.trim();
}

function buildMountScopes({
  groups,
  skillAssetById,
  sources,
  t,
}: {
  groups: AssetGroupDetail[];
  skillAssetById: Map<string, Asset>;
  sources: Source[];
  t: (key: TranslationKey, params?: Record<string, string | number>) => string;
}): MountScope[] {
  const sourceScopes = sources.map((source) => {
    const assetIds = [...skillAssetById.values()]
      .filter((asset) => asset.source_id === source.id)
      .map((asset) => asset.id);
    return {
      assetIds,
      blockedReason: isDirectMountBlockedSource(source) ? t("mount.blockedAppSource") : undefined,
      description: abbreviateHomePath(source.root_path),
      id: `source:${source.id}`,
      kind: "source" as const,
      name: source.name,
    };
  });
  const groupScopes = groups.map((detail) => ({
    assetIds: groupMemberAssetIds(detail).filter((assetId) => skillAssetById.has(assetId)),
    description: detail.group.description ?? t("group.noDescription"),
    id: `group:${detail.group.id}`,
    kind: "group" as const,
    name: detail.group.name,
  }));

  return [...sourceScopes, ...groupScopes].filter((scope) => scope.assetIds.length > 0);
}

function getProfileMountCounts(
  profileId: string,
  skillAssets: Asset[],
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>,
) {
  return skillAssets.reduce(
    (counts, asset) => {
      const status = (mountStatusesByAssetId.get(asset.id) ?? []).find((candidate) => candidate.profile_id === profileId);
      return {
        broken: counts.broken + (status?.state === "broken" ? 1 : 0),
        conflict: counts.conflict + (status?.state === "conflict" ? 1 : 0),
        mounted: counts.mounted + (status?.state === "mounted" ? 1 : 0),
        total: counts.total + 1,
      };
    },
    { broken: 0, conflict: 0, mounted: 0, total: 0 },
  );
}

function shortcutForProfile(profile: TargetProfile, shortcuts: AppShortcut[]) {
  return shortcuts.find((shortcut) => shortcut.profileId === profile.id) ?? defaultAppShortcut(profile);
}

function MountCountBadge({ count, label, total }: { count: number; label: string; total: number }) {
  return (
    <span className="inline-flex h-9 items-center gap-2 rounded-lg border border-theme-control-border bg-theme-control px-3 text-body-sm text-on-surface-variant">
      <span>{label}</span>
      <strong className="font-mono text-code-md text-primary">
        {count}/{total}
      </strong>
    </span>
  );
}

function ColumnHeader({
  actionIcon,
  actionLabel,
  dangerActionIcon,
  dangerActionDisabled = false,
  dangerActionLabel,
  meta,
  onAction,
  onDangerAction,
  title,
}: {
  actionIcon?: ReactNode;
  actionLabel?: string;
  dangerActionIcon?: ReactNode;
  dangerActionDisabled?: boolean;
  dangerActionLabel?: string;
  meta: string;
  onAction?: () => void;
  onDangerAction?: () => void;
  title: string;
}) {
  return (
    <header className="flex min-h-14 items-center justify-between gap-3 border-b border-theme-card-border bg-theme-card-header/70 px-4 py-3">
      <div className="min-w-0">
        <h3 className="overflow-hidden text-ellipsis whitespace-nowrap text-body-md font-semibold text-on-surface">{title}</h3>
        <p className="mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm text-outline">{meta}</p>
      </div>
      {onAction && actionLabel && (
        <div className="flex shrink-0 items-center gap-1.5">
          <button
            aria-label={actionLabel}
            className="grid size-8 place-items-center rounded-lg text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-primary"
            onClick={onAction}
            title={actionLabel}
            type="button"
          >
            {actionIcon ?? <Pencil size={16} />}
          </button>
          {onDangerAction && dangerActionLabel && (
            <button
              aria-label={dangerActionLabel}
              className="grid size-8 place-items-center rounded-lg text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-status-remove disabled:cursor-not-allowed disabled:opacity-45"
              disabled={dangerActionDisabled}
              onClick={onDangerAction}
              title={dangerActionLabel}
              type="button"
            >
              {dangerActionIcon ?? <Trash2 size={16} />}
            </button>
          )}
        </div>
      )}
    </header>
  );
}

function IconAction({
  children,
  danger = false,
  disabled = false,
  label,
  onClick,
}: {
  children: ReactNode;
  danger?: boolean;
  disabled?: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      aria-label={label}
      className={clsx(
        "grid size-8 place-items-center rounded-lg text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-primary disabled:cursor-not-allowed disabled:opacity-45",
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

function ToggleRow({
  checked,
  disabled,
  label,
  onChange,
}: {
  checked: boolean;
  disabled: boolean;
  label: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-xl border border-theme-control-border bg-theme-control/70 px-3 py-3">
      <span className="text-body-sm text-on-surface">{label}</span>
      <Switch aria-label={label} checked={checked} disabled={disabled} onCheckedChange={onChange} />
    </div>
  );
}

function Field({ children, label, required = false }: { children: ReactNode; label: string; required?: boolean }) {
  return (
    <label className="grid gap-1.5">
      <span className="text-body-sm font-medium text-on-surface-variant">
        {label}
        {required && <span className="text-status-remove"> *</span>}
      </span>
      {children}
    </label>
  );
}

function EmptyState({ children }: { children: ReactNode }) {
  return (
    <FoundationEmptyState className="min-h-0 px-4 py-10 text-body-md" title={children} />
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
