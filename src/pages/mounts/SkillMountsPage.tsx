import clsx from "clsx";
import {
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
import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import { AssetToolbar, type AssetToolbarViewMode } from "../../components/assets/AssetToolbar";
import { MountStatePill } from "../../components/assets/MountStatePill";
import { QuickMountButtons } from "../../components/assets/QuickMountButtons";
import { AppShortcutIconForShortcut } from "../../components/apps/AppShortcutIcon";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { Switch } from "../../components/ui/switch";
import { assetKindLabel, sourceOriginLabel } from "../../i18n/domain";
import { useI18n } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import {
  createProfile,
  deleteProfile,
  listSkillGroups,
  selectTargetDirectory,
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
import { displayAssetPath } from "../../utils/path";
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

export function SkillMountsPage({
  appShortcuts,
  assetMountStatuses,
  assets,
  onNotifyError,
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
  const [query, setQuery] = useState("");
  const [viewMode, setViewMode] = useState<SkillMountViewMode>("list");
  const [groups, setGroups] = useState<AssetGroupDetail[]>([]);
  const [expandedProfileIds, setExpandedProfileIds] = useState<Set<string>>(new Set());
  const [selectedProfileId, setSelectedProfileId] = useState<string | null>(null);
  const [selectedScopeId, setSelectedScopeId] = useState<string | null>(null);
  const [dialogProfile, setDialogProfile] = useState<TargetProfile | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
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

  async function handleSaveProfile(values: AppProfileDialogValues, editingProfile: TargetProfile | null) {
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
    if (!window.confirm(t("appMount.confirmDelete", { name: profile.name }))) {
      return;
    }

    setBusy(true);
    try {
      await deleteProfile(profile.id);
      await onSaveAppShortcuts(appShortcuts.filter((shortcut) => shortcut.profileId !== profile.id));
      await onRefreshProfiles();
      setSelectedProfileId((current) => (current === profile.id ? null : current));
    } catch (error) {
      onNotifyError(errorMessage(error));
    } finally {
      setBusy(false);
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
      <div className="flex items-start justify-between gap-4 max-[920px]:flex-col">
        <div className="min-w-0">
          <div className="flex items-center gap-2 text-status-update">
            <Boxes size={21} />
            <span className="text-label-caps uppercase">{t("appMount.page.subtitle")}</span>
          </div>
          <h1 className="mt-1 text-h2 text-on-surface">{t("appMount.page.title")}</h1>
        </div>
      </div>

      <AssetToolbar
        actionGroups={[
          [
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
        metrics={[
          { label: t("appMount.metric.apps"), value: profiles.length },
          { label: t("appMount.metric.skills"), value: skillAssets.length },
        ]}
        onQueryChange={setQuery}
        onViewModeChange={setViewMode}
        query={query}
        searchClassName="w-72 max-[1160px]:w-full"
        searchPlaceholder={t("appMount.searchPlaceholder")}
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
          busy={busy}
          mountStatusesByAssetId={mountStatusesByAssetId}
          onEditProfile={(profile) => {
            setDialogProfile(profile);
            setDialogOpen(true);
          }}
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
        <div className="overflow-hidden rounded-xl border border-border bg-surface-card/60" aria-label={t("appMount.page.title")}>
          {filteredProfiles.map((profile) => (
            <AppMountRow
              appShortcuts={appShortcuts}
              busy={busy}
              expanded={expandedProfileIds.has(profile.id)}
              key={profile.id}
              mountStatusesByAssetId={mountStatusesByAssetId}
              onDelete={() => void handleDeleteProfile(profile)}
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
        </div>
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
    </section>
  );
}

function AppMountRow({
  appShortcuts,
  busy,
  expanded,
  mountStatusesByAssetId,
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
  busy: boolean;
  expanded: boolean;
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
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

  return (
    <article className={clsx("border-b border-border last:border-b-0", expanded && "bg-surface-low/35")}>
      <div className="grid min-h-20 grid-cols-[minmax(0,1fr)_auto] items-center gap-4 px-4 py-3.5 hover:bg-surface-low/70 max-[860px]:grid-cols-1">
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
              <span className="rounded-md border border-border bg-surface-high px-2 py-0.5 text-label-caps uppercase text-on-surface-variant">
                {profile.app_kind}
              </span>
              <span
                className={clsx(
                  "rounded-md px-2 py-0.5 text-label-caps uppercase",
                  profile.enabled ? "bg-status-create/15 text-status-create" : "bg-surface-highest text-outline",
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
              {profile.target_paths[0] ?? ""}
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
          <IconAction danger disabled={busy} label={t("appMount.action.delete")} onClick={onDelete}>
            <Trash2 size={16} />
          </IconAction>
          <IconAction label={t(expanded ? "appMount.action.collapse" : "appMount.action.expand")} onClick={onToggleExpanded}>
            {expanded ? <ChevronDown size={17} /> : <ChevronRight size={17} />}
          </IconAction>
        </div>
      </div>

      {expanded && (
        <div className="border-t border-border bg-surface-lowest/20 p-4">
          <AppMountWorkbench
            appShortcuts={appShortcuts}
            busy={busy}
            mountStatusesByAssetId={mountStatusesByAssetId}
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
  busy,
  mountStatusesByAssetId,
  onEditProfile,
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
  busy: boolean;
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  onEditProfile: (profile: TargetProfile) => void;
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
  const scopeAssets = selectedScope.assetIds.flatMap((assetId) => {
    const asset = skillAssetById.get(assetId);
    return asset ? [asset] : [];
  });
  const shortcut = shortcutForProfile(selectedProfile, appShortcuts);

  return (
    <div className="grid min-h-[560px] overflow-hidden rounded-xl border border-border bg-surface-card/60 grid-cols-[minmax(240px,0.72fr)_minmax(320px,0.9fr)_minmax(360px,1.15fr)] max-[1120px]:grid-cols-[minmax(240px,0.8fr)_minmax(0,1.2fr)]">
      <section className="flex min-h-0 flex-col border-r border-border bg-surface-lowest/20">
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
                    : "border-transparent text-on-surface-variant hover:bg-surface-low/80 hover:text-on-surface",
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
                    {profile.target_paths[0] ?? ""}
                  </span>
                </span>
              </button>
            );
          })}
        </div>
      </section>

      <section className="flex min-h-0 flex-col border-r border-border max-[1120px]:border-r-0">
        <ColumnHeader
          actionLabel={t("appMount.action.edit")}
          meta={selectedProfile.target_paths[0] ?? ""}
          onAction={() => onEditProfile(selectedProfile)}
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

      <section className="flex min-h-0 flex-col bg-surface-lowest/20 max-[1120px]:col-span-2 max-[1120px]:border-t max-[1120px]:border-border">
        <ColumnHeader
          actionLabel={t("appMount.action.reveal")}
          actionIcon={<FolderOpen size={16} />}
          meta={t("appMount.scope.assetCount", { count: scopeAssets.length })}
          onAction={() => onRevealPath(selectedProfile.target_paths[0] ?? "")}
          title={selectedScope.name}
        />
        <div className="border-b border-border p-3">
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
          mountStatusesByAssetId={mountStatusesByAssetId}
          onToggleMount={onToggleMount}
          profile={selectedProfile}
          shortcut={shortcut}
          skillAssets={scopeAssets}
          sourceById={sourceById}
        />
      </section>
    </div>
  );
}

function AppMountWorkbench({
  appShortcuts,
  busy,
  mountStatusesByAssetId,
  onSetSkillMountProfiles,
  onToggleMount,
  profile,
  scopes,
  skillAssetById,
  sourceById,
}: {
  appShortcuts: AppShortcut[];
  busy: boolean;
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  onSetSkillMountProfiles: (assetIds: string[], profileId: string, enabled: boolean) => Promise<void>;
  onToggleMount: (assetId: string, profileId: string) => void | Promise<void>;
  profile: TargetProfile;
  scopes: MountScope[];
  skillAssetById: Map<string, Asset>;
  sourceById: Map<string, Source>;
}) {
  const { t } = useI18n();
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
    <div className="grid min-h-[420px] overflow-hidden rounded-xl border border-border bg-surface-card/45 grid-cols-[minmax(280px,0.85fr)_minmax(360px,1.15fr)] max-[960px]:grid-cols-1">
      <section className="flex min-h-0 flex-col border-r border-border max-[960px]:border-r-0 max-[960px]:border-b">
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
        <div className="border-b border-border p-3">
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
          mountStatusesByAssetId={mountStatusesByAssetId}
          onToggleMount={onToggleMount}
          profile={profile}
          shortcut={shortcutForProfile(profile, appShortcuts)}
          skillAssets={scopeAssets}
          sourceById={sourceById}
        />
      </section>
    </div>
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
                : "border-transparent text-on-surface-variant hover:bg-surface-low/80 hover:text-on-surface",
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
            <span className="mt-1 rounded-md border border-border bg-surface-high px-2 py-0.5 text-label-caps uppercase text-on-surface-variant">
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
  mountStatusesByAssetId,
  onToggleMount,
  profile,
  shortcut,
  skillAssets,
  sourceById,
}: {
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
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
        const mountBlockedReason = isDirectMountBlockedSource(source) ? t("mount.blocked") : undefined;
        return (
          <article
            className="grid min-h-[88px] grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-b border-border/70 px-4 py-3 last:border-b-0 hover:bg-surface-low/70 max-[760px]:grid-cols-1"
            key={asset.id}
          >
            <div className="min-w-0">
              <div className="flex min-w-0 items-center gap-2">
                <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-code-md font-semibold text-on-surface">
                  {asset.name}
                </span>
                <span className={kindBadgeClass(asset.kind)}>{assetKindLabel(asset.kind, t)}</span>
                <MountStatePill compact state={getAssetMountSummaryState(profileMountStatuses)} />
              </div>
              <div className="mt-1 flex min-w-0 items-center gap-2">
                <span className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm text-on-surface-variant">
                  {displayAssetPath(asset)}
                </span>
                {source && (
                  <span className="shrink-0 rounded-md border border-border bg-surface-high px-2 py-0.5 text-label-caps uppercase text-on-surface-variant">
                    {sourceOriginLabel(source.source_origin, t)}
                  </span>
                )}
              </div>
            </div>
            <QuickMountButtons
              asset={asset}
              mountBlockedReason={mountBlockedReason}
              mountStatuses={profileMountStatuses}
              onToggle={() => void onToggleMount(asset.id, profile.id)}
              profiles={[profile]}
              shortcuts={[{ ...shortcut, enabled: true }]}
            />
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
        updateValue("targetPath", selectedPath);
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
    <div className="fixed inset-0 z-40 grid place-items-center bg-background/72 px-6 py-8 backdrop-blur-sm">
      <section
        aria-modal="true"
        className="flex max-h-full w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-border bg-surface-low shadow-[0_24px_72px_rgba(0,0,0,0.42)]"
        role="dialog"
      >
        <header className="flex h-16 shrink-0 items-center justify-between border-b border-border px-5">
          <div className="flex min-w-0 items-center gap-3">
            <span className="grid size-9 place-items-center rounded-xl border border-status-update/25 bg-status-update/15 text-status-update">
              <Boxes size={18} />
            </span>
            <h2 className="truncate text-h2 text-on-surface">
              {profile ? t("appMount.dialog.editTitle") : t("appMount.dialog.importTitle")}
            </h2>
          </div>
          <Button aria-label={t("appMount.dialog.close")} disabled={busy} onClick={onClose} size="icon" type="button" variant="ghost">
            <X size={18} />
          </Button>
        </header>

        <form className="min-h-0 overflow-y-auto px-5 py-5" onSubmit={(event) => void handleSubmit(event)}>
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
                  className="h-9 rounded-lg border border-border bg-surface-high px-3 text-body-sm text-on-surface outline-none focus:border-primary-strong/60 disabled:opacity-50"
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
              <div className="flex gap-2">
                <Input
                  className="min-w-0 flex-1"
                  disabled={busy || picking}
                  onChange={(event) => updateValue("targetPath", event.target.value)}
                  placeholder={t("appMount.dialog.targetPlaceholder")}
                  value={values.targetPath}
                />
                <Button
                  aria-label={t("appMount.dialog.pickTarget")}
                  disabled={busy || picking}
                  onClick={() => void handlePickTargetPath()}
                  size="icon"
                  title={t("appMount.dialog.pickTarget")}
                  type="button"
                  variant="outline"
                >
                  <FolderOpen size={17} />
                </Button>
              </div>
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

          <footer className="mt-5 flex justify-end gap-2 border-t border-border pt-4">
            <Button disabled={busy} onClick={onClose} type="button" variant="outline">
              {t("appMount.dialog.cancel")}
            </Button>
            <Button disabled={busy || !values.name.trim() || !values.targetPath.trim()} type="submit">
              {busy ? t("appMount.dialog.saving") : t("appMount.dialog.save")}
            </Button>
          </footer>
        </form>
      </section>
    </div>
  );
}

function initialDialogValues(profile: TargetProfile | null, shortcut: AppShortcut | null): AppProfileDialogValues {
  const defaultShortcut = profile ? defaultAppShortcut(profile) : null;
  return {
    accentColor: shortcut?.accentColor ?? defaultShortcut?.accentColor ?? "#8c909f",
    appKind: profile?.app_kind ?? "custom",
    displayIcon: shortcut?.displayIcon ?? defaultShortcut?.displayIcon ?? "",
    enabled: profile?.enabled ?? true,
    name: profile?.name ?? "",
    shortcutEnabled: shortcut?.enabled ?? true,
    targetPath: profile?.target_paths[0] ?? "",
  };
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
      description: source.root_path,
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
    <span className="inline-flex h-9 items-center gap-2 rounded-lg border border-border bg-surface-high px-3 text-body-sm text-on-surface-variant">
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
  meta,
  onAction,
  title,
}: {
  actionIcon?: ReactNode;
  actionLabel?: string;
  meta: string;
  onAction?: () => void;
  title: string;
}) {
  return (
    <header className="flex min-h-14 items-center justify-between gap-3 border-b border-border bg-surface-high/55 px-4 py-3">
      <div className="min-w-0">
        <h3 className="overflow-hidden text-ellipsis whitespace-nowrap text-body-md font-semibold text-on-surface">{title}</h3>
        <p className="mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm text-outline">{meta}</p>
      </div>
      {onAction && actionLabel && (
        <button
          aria-label={actionLabel}
          className="grid size-8 shrink-0 place-items-center rounded-lg text-on-surface-variant transition-colors hover:bg-surface-highest hover:text-primary"
          onClick={onAction}
          title={actionLabel}
          type="button"
        >
          {actionIcon ?? <Pencil size={16} />}
        </button>
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
    <div className="flex items-center justify-between gap-4 rounded-xl border border-border bg-surface-high/60 px-3 py-3">
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
    <div className="rounded-xl border border-border bg-surface-card/60 px-4 py-10 text-center text-body-md text-on-surface-variant">
      {children}
    </div>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
