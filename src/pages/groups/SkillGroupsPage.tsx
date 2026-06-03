import clsx from "clsx";
import {
  ChevronDown,
  ChevronRight,
  Columns3,
  Layers3,
  LayoutList,
  Pencil,
  Plus,
  RefreshCw,
  Settings,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { assetKindLabel } from "../../i18n/domain";
import { AssetRow } from "../../components/assets/AssetRow";
import { AssetToolbar } from "../../components/assets/AssetToolbar";
import { ConfirmDialog } from "../../components/common/ConfirmDialog";
import { MountStatePill } from "../../components/assets/MountStatePill";
import { QuickMountButtons } from "../../components/assets/QuickMountButtons";
import { GroupBulkMountControls } from "../../components/groups/GroupBulkMountControls";
import { GroupExclusiveMountControls, type GroupMountMode } from "../../components/groups/GroupExclusiveMountControls";
import { SkillGroupCreateDialog } from "../../components/groups/SkillGroupCreateDialog";
import { SkillGroupExclusiveMountDialog } from "../../components/groups/SkillGroupExclusiveMountDialog";
import { SkillGroupEditDialog } from "../../components/groups/SkillGroupEditDialog";
import { useI18n, type Translator } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import {
  createSkillGroup,
  deleteSkillGroup,
  listSkillGroups,
  setSkillGroupManualMembers,
  updateSkillGroup,
} from "../../services/catalog";
import type {
  AppShortcut,
  Asset,
  AssetGroup,
  AssetGroupDetail,
  AssetGroupInput,
  AssetMountStatus,
  ApplySkillGroupExclusiveMountResult,
  SkillGroupExclusiveMountPreview,
  Source,
  TargetProfile,
} from "../../types";
import { getAssetMountSummaryState, groupMountStatusesByAssetId } from "../../utils/mountState";
import { isDirectMountBlockedSource } from "../../utils/mountPolicy";
import { displayAssetPath } from "../../utils/path";
import {
  enabledGroupIds,
  groupMemberAssetIds,
  shouldShowGroupExclusiveMountControls,
  toggleEnabledGroupSelection,
} from "../../utils/skillGroups";
import { kindBadgeClass } from "../../utils/styles";

interface SkillGroupsPageProps {
  appShortcuts: AppShortcut[];
  assetMountStatuses: AssetMountStatus[];
  assets: Asset[];
  expandedAssetIds: Set<string>;
  onNotifyError: (message: string) => void;
  onOpenSettings: () => void;
  onApplyGroupExclusiveMount: (groupIds: string[], profileId: string) => Promise<ApplySkillGroupExclusiveMountResult>;
  onPreviewGroupExclusiveMount: (groupIds: string[], profileId: string) => Promise<SkillGroupExclusiveMountPreview>;
  onRefreshMountStatus: () => Promise<void>;
  onRevealPath: (path: string) => void;
  onSetGroupMountProfile: (groupId: string, assetIds: string[], profileId: string, enabled: boolean) => Promise<void>;
  onSetSkillMountProfiles: (assetIds: string[], profileId: string, enabled: boolean) => Promise<void>;
  onToggleAsset: (assetId: string) => void;
  onToggleMount: (assetId: string, profileId: string) => void;
  profiles: TargetProfile[];
  refreshingMountStatus: boolean;
  sources: Source[];
}

type GroupViewMode = "list" | "columns";

export function SkillGroupsPage({
  appShortcuts,
  assetMountStatuses,
  assets,
  expandedAssetIds,
  onNotifyError,
  onOpenSettings,
  onApplyGroupExclusiveMount,
  onPreviewGroupExclusiveMount,
  onRefreshMountStatus,
  onRevealPath,
  onSetGroupMountProfile,
  onSetSkillMountProfiles,
  onToggleAsset,
  onToggleMount,
  profiles,
  refreshingMountStatus,
  sources,
}: SkillGroupsPageProps) {
  const { t } = useI18n();
  const [groups, setGroups] = useState<AssetGroupDetail[]>([]);
  const [expandedGroupIds, setExpandedGroupIds] = useState<Set<string>>(new Set());
  const [selectedGroupId, setSelectedGroupId] = useState<string | null>(null);
  const [groupQuery, setGroupQuery] = useState("");
  const [viewMode, setViewMode] = useState<GroupViewMode>("list");
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [editingGroup, setEditingGroup] = useState<AssetGroupDetail | null>(null);
  const [deletingGroup, setDeletingGroup] = useState<AssetGroupDetail | null>(null);
  const [mountingGroupId, setMountingGroupId] = useState<string | null>(null);
  const [exclusivePreview, setExclusivePreview] = useState<SkillGroupExclusiveMountPreview | null>(null);
  const [exclusiveShortcut, setExclusiveShortcut] = useState<AppShortcut | null>(null);
  const [selectedGroupIds, setSelectedGroupIds] = useState<Set<string>>(new Set());
  const [groupMountMode, setGroupMountMode] = useState<GroupMountMode>("exclusive");
  const [exclusiveBusy, setExclusiveBusy] = useState(false);
  const [busy, setBusy] = useState(false);

  const skillAssetsById = useMemo(() => {
    return new Map(assets.filter((asset) => asset.kind === "skill").map((asset) => [asset.id, asset]));
  }, [assets]);
  const sourceById = useMemo(() => new Map(sources.map((source) => [source.id, source])), [sources]);
  const mountStatusesByAssetId = useMemo(
    () => groupMountStatusesByAssetId(assetMountStatuses),
    [assetMountStatuses],
  );
  const memberTotal = useMemo(
    () => groups.reduce((total, detail) => total + groupMemberAssetIds(detail).length, 0),
    [groups],
  );
  const selectedGroupDetails = useMemo(
    () => groups.filter((detail) => detail.group.enabled && selectedGroupIds.has(detail.group.id)),
    [groups, selectedGroupIds],
  );
  const selectedExclusiveSkillIds = useMemo(() => {
    const assetIds = new Set<string>();
    for (const detail of selectedGroupDetails) {
      for (const assetId of groupMemberAssetIds(detail)) {
        if (skillAssetsById.has(assetId)) {
          assetIds.add(assetId);
        }
      }
    }
    return [...assetIds];
  }, [selectedGroupDetails, skillAssetsById]);
  const selectedExclusiveGroupIds = useMemo(
    () => selectedGroupDetails.map((detail) => detail.group.id),
    [selectedGroupDetails],
  );

  useEffect(() => {
    void refreshGroups();
  }, []);

  useEffect(() => {
    setSelectedGroupIds((current) => {
      const enabledGroupIds = new Set(groups.filter((detail) => detail.group.enabled).map((detail) => detail.group.id));
      const next = new Set([...current].filter((groupId) => enabledGroupIds.has(groupId)));
      return next.size === current.size ? current : next;
    });
  }, [groups]);

  const filteredGroups = useMemo(() => {
    const normalizedQuery = groupQuery.trim().toLowerCase();
    if (!normalizedQuery) {
      return groups;
    }

    return groups.filter((detail) => {
      const memberNames = resolveGroupAssets(detail, skillAssetsById)
        .map((asset) => asset.name)
        .join(" ");
      return [detail.group.name, detail.group.description ?? "", memberNames]
        .join(" ")
        .toLowerCase()
        .includes(normalizedQuery);
    });
  }, [groupQuery, groups, skillAssetsById]);
  const selectableFilteredGroupIds = useMemo(() => enabledGroupIds(filteredGroups), [filteredGroups]);
  const allFilteredGroupsSelected = useMemo(
    () => selectableFilteredGroupIds.length > 0 && selectableFilteredGroupIds.every((groupId) => selectedGroupIds.has(groupId)),
    [selectableFilteredGroupIds, selectedGroupIds],
  );
  const partiallySelectedFilteredGroups = useMemo(
    () => selectableFilteredGroupIds.some((groupId) => selectedGroupIds.has(groupId)) && !allFilteredGroupsSelected,
    [allFilteredGroupsSelected, selectableFilteredGroupIds, selectedGroupIds],
  );
  const selectedColumnGroup = useMemo(
    () => filteredGroups.find((detail) => detail.group.id === selectedGroupId) ?? filteredGroups[0] ?? null,
    [filteredGroups, selectedGroupId],
  );

  async function refreshGroups() {
    setBusy(true);
    try {
      setGroups(await listSkillGroups());
    } catch (loadError) {
      onNotifyError(errorMessage(loadError));
    } finally {
      setBusy(false);
    }
  }

  async function handleCreateGroup(input: AssetGroupInput, assetIds: string[]) {
    setBusy(true);
    try {
      const createdDetail = await createSkillGroup(input);
      const detail = assetIds.length > 0
        ? await setSkillGroupManualMembers(createdDetail.group.id, assetIds)
        : createdDetail;
      setGroups((current) => upsertGroupDetail(current, detail));
      setSelectedGroupId(detail.group.id);
      setExpandedGroupIds((current) => {
        const next = new Set(current);
        next.add(detail.group.id);
        return next;
      });
      setCreateDialogOpen(false);
    } catch (createError) {
      onNotifyError(errorMessage(createError));
    } finally {
      setBusy(false);
    }
  }

  async function handleUpdateGroup(group: AssetGroup, manualAssetIds: string[]) {
    setBusy(true);
    try {
      await updateSkillGroup(group);
      const detail = await setSkillGroupManualMembers(group.id, manualAssetIds);
      setGroups((current) => upsertGroupDetail(current, detail));
      setSelectedGroupId(group.id);
      setExpandedGroupIds((current) => {
        const next = new Set(current);
        next.add(group.id);
        return next;
      });
      setEditingGroup(null);
    } catch (updateError) {
      onNotifyError(errorMessage(updateError));
    } finally {
      setBusy(false);
    }
  }

  async function handleDeleteGroup(detail: AssetGroupDetail) {
    setBusy(true);
    try {
      await deleteSkillGroup(detail.group.id);
      setGroups((current) => current.filter((candidate) => candidate.group.id !== detail.group.id));
      setSelectedGroupId((current) => (current === detail.group.id ? null : current));
      setExpandedGroupIds((current) => {
        const next = new Set(current);
        next.delete(detail.group.id);
        return next;
      });
      setDeletingGroup(null);
    } catch (deleteError) {
      onNotifyError(errorMessage(deleteError));
    } finally {
      setBusy(false);
    }
  }

  function toggleGroupExpanded(groupId: string) {
    setExpandedGroupIds((current) => {
      const next = new Set(current);
      if (next.has(groupId)) {
        next.delete(groupId);
      } else {
        next.add(groupId);
      }
      return next;
    });
  }

  function toggleGroupSelected(detail: AssetGroupDetail) {
    if (!detail.group.enabled) {
      return;
    }

    setSelectedGroupIds((current) => {
      const next = new Set(current);
      if (next.has(detail.group.id)) {
        next.delete(detail.group.id);
      } else {
        next.add(detail.group.id);
      }
      return next;
    });
  }

  async function handlePreviewExclusiveMount(shortcut: AppShortcut) {
    if (selectedExclusiveGroupIds.length === 0) {
      return;
    }

    setExclusiveBusy(true);
    try {
      const preview = groupMountMode === "exclusive"
        ? await onPreviewGroupExclusiveMount(selectedExclusiveGroupIds, shortcut.profileId)
        : buildAdditiveMountPreview(shortcut.profileId);
      setExclusivePreview(preview);
      setExclusiveShortcut(shortcut);
    } catch (previewError) {
      onNotifyError(errorMessage(previewError));
    } finally {
      setExclusiveBusy(false);
    }
  }

  async function handleApplyExclusiveMount() {
    if (!exclusivePreview) {
      return;
    }

    setExclusiveBusy(true);
    try {
      if (groupMountMode === "exclusive") {
        await onApplyGroupExclusiveMount(exclusivePreview.group_ids, exclusivePreview.profile_id);
      } else {
        await onSetSkillMountProfiles(
          exclusivePreview.mount.map((item) => item.asset_id),
          exclusivePreview.profile_id,
          true,
        );
      }
      setSelectedGroupIds(new Set());
      setExclusivePreview(null);
      setExclusiveShortcut(null);
    } finally {
      setExclusiveBusy(false);
    }
  }

  function toggleAllFilteredGroups() {
    setSelectedGroupIds((current) => toggleEnabledGroupSelection(current, filteredGroups));
  }

  function buildAdditiveMountPreview(profileId: string): SkillGroupExclusiveMountPreview {
    const keep: SkillGroupExclusiveMountPreview["keep"] = [];
    const mount: SkillGroupExclusiveMountPreview["mount"] = [];
    const skipped: SkillGroupExclusiveMountPreview["skipped"] = [];

    for (const assetId of selectedExclusiveSkillIds) {
      const asset = skillAssetsById.get(assetId);
      if (!asset) {
        continue;
      }

      const source = sourceById.get(asset.source_id);
      const status = (mountStatusesByAssetId.get(asset.id) ?? []).find((candidate) => candidate.profile_id === profileId);
      if (isDirectMountBlockedSource(source)) {
        skipped.push({ asset_id: asset.id, name: asset.name, reason: t("mount.blockedAppSource") });
      } else if (status?.state === "mounted") {
        keep.push({ asset_id: asset.id, name: asset.name });
      } else if (status?.state === "conflict" || status?.state === "broken") {
        skipped.push({
          asset_id: asset.id,
          name: asset.name,
          reason: t(`mount.status.${status.state}` as TranslationKey),
        });
      } else {
        mount.push({ asset_id: asset.id, name: asset.name });
      }
    }

    keep.sort(compareMountItems);
    mount.sort(compareMountItems);
    skipped.sort(compareMountItems);

    return {
      profile_id: profileId,
      group_ids: selectedExclusiveGroupIds,
      selected_skill_ids: [...selectedExclusiveSkillIds].sort(),
      keep,
      mount,
      unmount: [],
      skipped,
      keep_count: keep.length,
      mount_count: mount.length,
      unmount_count: 0,
      skipped_count: skipped.length,
    };
  }

  async function handleSetGroupMountProfile(
    detail: AssetGroupDetail,
    groupAssets: Asset[],
    profileId: string,
    enabled: boolean,
  ) {
    const assetIds = groupAssets.filter((asset) => asset.kind === "skill").map((asset) => asset.id);
    if (assetIds.length === 0) {
      return;
    }

    setMountingGroupId(detail.group.id);
    try {
      await onSetGroupMountProfile(detail.group.id, assetIds, profileId, enabled);
    } finally {
      setMountingGroupId(null);
    }
  }

  return (
    <section className="flex flex-1 flex-col gap-[var(--app-section-gap)] px-[var(--app-page-x)] py-[var(--app-page-y)]">
      <div className="flex items-start justify-between gap-4 max-[920px]:flex-col">
        <div className="min-w-0">
          <div className="flex items-center gap-2 text-status-update">
            <Layers3 size={21} />
            <span className="text-label-caps uppercase">{t("group.page.subtitle")}</span>
          </div>
          <h1 className="mt-1 text-h2 text-on-surface">{t("group.page.title")}</h1>
        </div>
      </div>

      <AssetToolbar
        actionGroups={[
          [
            {
              disabled: busy,
              icon: <Plus size={17} />,
              label: t("group.action.create"),
              onClick: () => setCreateDialogOpen(true),
              primary: true,
              text: t("group.action.create"),
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
        ariaLabel={t("group.page.title")}
        metrics={[
          { label: t("group.metric.groups"), value: groups.length },
          { label: t("group.metric.members"), value: memberTotal },
        ]}
        onQueryChange={setGroupQuery}
        onViewModeChange={setViewMode}
        query={groupQuery}
        searchClassName="w-72 max-[1160px]:w-full"
        searchPlaceholder={t("group.search.groups")}
        viewAriaLabel={t("toolbar.view.aria")}
        viewMode={viewMode}
        viewOptions={[
          { icon: <LayoutList size={17} />, label: t("toolbar.view.list"), value: "list" },
          { icon: <Columns3 size={17} />, label: t("toolbar.view.columns"), value: "columns" },
        ]}
      />

      {shouldShowGroupExclusiveMountControls(selectedGroupDetails.length) && (
        <GroupExclusiveMountControls
          allSelected={allFilteredGroupsSelected}
          appShortcuts={appShortcuts}
          busy={busy || exclusiveBusy || mountingGroupId !== null}
          mode={groupMountMode}
          onModeChange={setGroupMountMode}
          onPreviewProfile={handlePreviewExclusiveMount}
          onToggleAll={toggleAllFilteredGroups}
          partiallySelected={partiallySelectedFilteredGroups}
          profiles={profiles}
          selectableGroupCount={selectableFilteredGroupIds.length}
          selectedGroupCount={selectedGroupDetails.length}
          selectedSkillCount={selectedExclusiveSkillIds.length}
        />
      )}

      {busy && groups.length === 0 ? (
        <div className="rounded-xl border border-theme-card-border bg-theme-card/70 px-4 py-10 text-center text-body-md text-on-surface-variant shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.16)]">
          {t("status.loading")}
        </div>
      ) : filteredGroups.length === 0 ? (
        <div className="rounded-xl border border-theme-card-border bg-theme-card/70 px-4 py-10 text-center text-body-md text-on-surface-variant shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.16)]">
          {t("group.empty")}
        </div>
      ) : viewMode === "columns" && selectedColumnGroup ? (
        <GroupColumnView
          appShortcuts={appShortcuts}
          busy={busy || mountingGroupId !== null}
          groups={filteredGroups}
          mountStatusesByAssetId={mountStatusesByAssetId}
          onAssetReveal={onRevealPath}
          onDelete={setDeletingGroup}
          onEdit={setEditingGroup}
          onSelectGroup={setSelectedGroupId}
          onToggleGroupSelected={toggleGroupSelected}
          onSetGroupMountProfile={(detail, groupAssets, profileId, enabled) =>
            void handleSetGroupMountProfile(detail, groupAssets, profileId, enabled)
          }
          onToggleMount={onToggleMount}
          profiles={profiles}
          selectedGroup={selectedColumnGroup}
          selectedGroupAssets={resolveGroupAssets(selectedColumnGroup, skillAssetsById)}
          selectedGroupIds={selectedGroupIds}
          sourceById={sourceById}
        />
      ) : (
        <div
          aria-label={t("group.page.title")}
          className="overflow-hidden rounded-xl border border-theme-card-border bg-theme-card/70 shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.18)]"
        >
          {filteredGroups.map((detail) => {
            const groupAssets = resolveGroupAssets(detail, skillAssetsById);
            return (
              <GroupRow
                appShortcuts={appShortcuts}
                assets={groupAssets}
                busy={busy || mountingGroupId !== null}
                expanded={expandedGroupIds.has(detail.group.id)}
                expandedAssetIds={expandedAssetIds}
                key={detail.group.id}
                mountStatusesByAssetId={mountStatusesByAssetId}
                onAssetReveal={onRevealPath}
                onDelete={() => setDeletingGroup(detail)}
                onEdit={() => setEditingGroup(detail)}
                onToggleSelected={() => toggleGroupSelected(detail)}
                onSetGroupMountProfile={(profileId, enabled) =>
                  void handleSetGroupMountProfile(detail, groupAssets, profileId, enabled)
                }
                onToggleAsset={onToggleAsset}
                onToggleExpanded={() => toggleGroupExpanded(detail.group.id)}
                onToggleMount={onToggleMount}
                profiles={profiles}
                selected={selectedGroupIds.has(detail.group.id)}
                sourceById={sourceById}
                detail={detail}
              />
            );
          })}
        </div>
      )}

      <SkillGroupCreateDialog
        assets={assets}
        busy={busy}
        nextSortOrder={groups.length * 10}
        onClose={() => setCreateDialogOpen(false)}
        onSubmit={handleCreateGroup}
        open={createDialogOpen}
      />
      <SkillGroupEditDialog
        assets={assets}
        busy={busy}
        detail={editingGroup}
        onClose={() => setEditingGroup(null)}
        onSubmit={handleUpdateGroup}
      />
      <SkillGroupExclusiveMountDialog
        busy={exclusiveBusy}
        onClose={() => {
          if (!exclusiveBusy) {
            setExclusivePreview(null);
            setExclusiveShortcut(null);
          }
        }}
        onConfirm={handleApplyExclusiveMount}
        mode={groupMountMode}
        preview={exclusivePreview}
        shortcut={exclusiveShortcut}
      />
      <ConfirmDialog
        busy={busy}
        confirmLabel={t("common.delete")}
        message={deletingGroup ? t("group.deleteDialog.message", { name: deletingGroup.group.name }) : ""}
        onClose={() => setDeletingGroup(null)}
        onConfirm={() => deletingGroup && void handleDeleteGroup(deletingGroup)}
        open={Boolean(deletingGroup)}
        title={t("group.deleteDialog.title")}
        tone="danger"
      >
        <div className="rounded-xl border border-theme-card-border bg-theme-card/65 p-3 text-body-sm text-on-surface-variant">
          {t("group.deleteDialog.detail")}
        </div>
      </ConfirmDialog>
    </section>
  );
}

function GroupColumnView({
  appShortcuts,
  busy,
  groups,
  mountStatusesByAssetId,
  onAssetReveal,
  onDelete,
  onEdit,
  onSelectGroup,
  onSetGroupMountProfile,
  onToggleGroupSelected,
  onToggleMount,
  profiles,
  selectedGroup,
  selectedGroupAssets,
  selectedGroupIds,
  sourceById,
}: {
  appShortcuts: AppShortcut[];
  busy: boolean;
  groups: AssetGroupDetail[];
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  onAssetReveal: (path: string) => void;
  onDelete: (detail: AssetGroupDetail) => void;
  onEdit: (detail: AssetGroupDetail) => void;
  onSelectGroup: (groupId: string) => void;
  onSetGroupMountProfile: (detail: AssetGroupDetail, groupAssets: Asset[], profileId: string, enabled: boolean) => void;
  onToggleGroupSelected: (detail: AssetGroupDetail) => void;
  onToggleMount: (assetId: string, profileId: string) => void;
  profiles: TargetProfile[];
  selectedGroup: AssetGroupDetail;
  selectedGroupAssets: Asset[];
  selectedGroupIds: Set<string>;
  sourceById: Map<string, Source>;
}) {
  const { t } = useI18n();
  const selectedMemberIds = groupMemberAssetIds(selectedGroup);
  const manualMemberCount = selectedGroup.manual_asset_ids.length;
  const ruleMemberCount = selectedGroup.members.filter(
    (member) => member.origin === "rule" || member.origin === "manual_and_rule",
  ).length;

  return (
    <div className="grid min-h-[560px] overflow-hidden rounded-xl border border-theme-card-border bg-theme-card/70 shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.18)] grid-cols-[minmax(240px,0.72fr)_minmax(360px,1.14fr)_minmax(320px,0.9fr)] max-[1120px]:grid-cols-[minmax(240px,0.8fr)_minmax(0,1.2fr)]">
      <section className="flex min-h-0 flex-col border-r border-theme-card-border bg-theme-card-header/35">
        <GroupColumnHeader title={t("group.column.groups")} meta={t("group.metric.groupsWithCount", { count: groups.length })} />
        <div className="min-h-0 overflow-y-auto py-1" role="listbox" aria-label={t("group.column.groups")}>
          {groups.map((detail) => {
            const memberCount = groupMemberAssetIds(detail).length;
            const active = detail.group.id === selectedGroup.group.id;
            const selected = selectedGroupIds.has(detail.group.id);
            return (
              <div
                aria-selected={active}
                className={clsx(
                  "grid min-h-[72px] w-full grid-cols-[auto_minmax(0,1fr)] items-start gap-3 border-l-2 px-3 py-3 text-left transition-colors",
                  active
                    ? "border-primary bg-primary/10 text-on-surface"
                    : "border-transparent text-on-surface-variant hover:bg-theme-control-hover hover:text-on-surface",
                )}
                key={detail.group.id}
                role="option"
              >
                <input
                  aria-label={t("group.exclusive.selectGroup", { name: detail.group.name })}
                  checked={selected}
                  className="mt-1.5 size-4 rounded border-theme-control-border accent-primary disabled:cursor-not-allowed disabled:opacity-40"
                  disabled={!detail.group.enabled}
                  onChange={() => onToggleGroupSelected(detail)}
                  type="checkbox"
                />
                <button
                  aria-label={detail.group.name}
                  className="flex min-w-0 items-start gap-3 text-left"
                  onClick={() => onSelectGroup(detail.group.id)}
                  type="button"
                >
                  <GroupAvatar color={detail.group.color} enabled={detail.group.enabled} compact />
                  <span className="min-w-0 flex-1">
                    <span className="block overflow-hidden text-ellipsis whitespace-nowrap font-mono text-code-md font-semibold">
                      {detail.group.name}
                    </span>
                    <span className="mt-1 line-clamp-1 text-body-sm text-on-surface-variant">
                      {detail.group.description || summarizeRules(detail, sourceById, t)}
                    </span>
                    <span className="mt-1 text-body-sm text-outline">{t("group.memberCount", { count: memberCount })}</span>
                  </span>
                </button>
              </div>
            );
          })}
        </div>
      </section>

      <section className="flex min-h-0 flex-col border-r border-theme-card-border max-[1120px]:border-r-0">
        <GroupColumnHeader
          title={selectedGroup.group.name}
          meta={t("group.memberCount", { count: selectedGroupAssets.length })}
        />
        <div className="min-h-0 overflow-y-auto">
          {selectedGroupAssets.length === 0 ? (
            <div className="px-4 py-5 text-body-sm text-on-surface-variant">{t("group.emptyMembers")}</div>
          ) : (
            selectedGroupAssets.map((asset) => {
              const source = sourceById.get(asset.source_id);
              const mountStatuses = mountStatusesByAssetId.get(asset.id) ?? [];
              const mountBlockedReason = isDirectMountBlockedSource(source) ? t("mount.blocked") : undefined;
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
                  <QuickMountButtons
                    asset={asset}
                    mountBlockedReason={mountBlockedReason}
                    mountStatuses={mountStatuses}
                    profiles={profiles}
                    shortcuts={appShortcuts}
                    onToggle={(profileId) => onToggleMount(asset.id, profileId)}
                  />
                </article>
              );
            })
          )}
        </div>
      </section>

      <section className="flex min-h-0 flex-col bg-theme-card-header/35 max-[1120px]:col-span-2 max-[1120px]:border-t max-[1120px]:border-theme-card-border">
        <GroupColumnHeader
          title={t("group.column.details")}
          meta={selectedGroup.group.enabled ? t("group.status.enabled") : t("group.status.disabled")}
        />
        <div className="min-h-0 overflow-y-auto p-4">
          <div className="flex flex-wrap items-center gap-2">
            <button
              className="inline-flex h-9 items-center justify-center gap-2 rounded-lg border border-theme-control-border bg-theme-control px-3 text-body-sm font-semibold text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-primary"
              onClick={() => onEdit(selectedGroup)}
              type="button"
            >
              <Pencil size={15} />
              {t("group.action.edit")}
            </button>
            <button
              className="inline-flex h-9 items-center justify-center gap-2 rounded-lg border border-status-remove/45 bg-status-remove/10 px-3 text-body-sm font-semibold text-status-remove transition-colors hover:bg-status-remove/15"
              onClick={() => onDelete(selectedGroup)}
              type="button"
            >
              <Trash2 size={15} />
              {t("group.action.delete")}
            </button>
          </div>

          <div className="mt-4">
            <GroupBulkMountControls
              appShortcuts={appShortcuts}
              assets={selectedGroupAssets}
              busy={busy}
              detail={selectedGroup}
              mountStatusesByAssetId={mountStatusesByAssetId}
              onSetGroupMountProfile={(profileId, enabled) =>
                onSetGroupMountProfile(selectedGroup, selectedGroupAssets, profileId, enabled)
              }
              profiles={profiles}
              variant="panel"
            />
          </div>

          <div className="mt-4 space-y-3 rounded-xl border border-theme-card-border bg-theme-card/65 p-3">
            <GroupDetailRow label={t("group.field.description")} value={selectedGroup.group.description ?? t("group.noDescription")} />
            <GroupDetailRow label={t("group.metric.members")} value={String(selectedMemberIds.length)} mono />
            <GroupDetailRow label={t("group.detail.manualMembers")} value={String(manualMemberCount)} mono />
            <GroupDetailRow label={t("group.detail.ruleMembers")} value={String(ruleMemberCount)} mono />
            <GroupDetailRow
              label={t("group.rules.nameContains")}
              value={selectedGroup.group.rules.name_contains || t("group.rules.empty")}
              mono={Boolean(selectedGroup.group.rules.name_contains)}
            />
            <GroupRuleList
              label={t("group.rules.sources")}
              rules={selectedGroup.group.rules.source_ids.map((sourceId) => sourceById.get(sourceId)?.name ?? sourceId)}
            />
            <GroupRuleList label={t("group.rules.pathGlobs")} rules={selectedGroup.group.rules.relative_path_globs} />
          </div>
        </div>
      </section>
    </div>
  );
}

function GroupColumnHeader({
  meta,
  title,
}: {
  meta: string;
  title: string;
}) {
  return (
    <header className="flex min-h-14 items-center justify-between gap-3 border-b border-theme-card-border bg-theme-card-header/70 px-4 py-3">
      <div className="min-w-0">
        <h3 className="overflow-hidden text-ellipsis whitespace-nowrap text-body-md font-semibold text-on-surface">{title}</h3>
        <p className="mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm text-outline">{meta}</p>
      </div>
    </header>
  );
}

function GroupDetailRow({ label, mono = false, value }: { label: string; mono?: boolean; value: string }) {
  return (
    <div className="min-w-0">
      <div className="text-label-caps uppercase text-outline">{label}</div>
      <div className={clsx("mt-1 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm text-on-surface", mono && "font-mono")}>
        {value}
      </div>
    </div>
  );
}

function GroupRuleList({ label, rules }: { label: string; rules: string[] }) {
  const { t } = useI18n();

  return (
    <div className="min-w-0">
      <div className="text-label-caps uppercase text-outline">{label}</div>
      {rules.length === 0 ? (
        <div className="mt-1 text-body-sm text-on-surface-variant">{t("group.rules.empty")}</div>
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

function GroupAvatar({
  color,
  compact = false,
  enabled,
}: {
  color: string;
  compact?: boolean;
  enabled: boolean;
}) {
  return (
    <span
      className={clsx(
        "relative grid shrink-0 place-items-center rounded-lg border shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.28)]",
        compact ? "mt-0.5 size-8" : "size-10",
        !enabled && "grayscale",
      )}
      style={{
        backgroundColor: `${color}18`,
        borderColor: `${color}66`,
        color,
      }}
      aria-hidden="true"
    >
      <Layers3 size={compact ? 16 : 18} />
      <span
        className={clsx(
          "absolute rounded-full border border-surface-card",
          compact ? "-right-0.5 -top-0.5 size-2.5" : "-right-0.5 -top-0.5 size-3",
          enabled ? "bg-status-create" : "bg-outline",
        )}
      />
    </span>
  );
}

function GroupRow({
  appShortcuts,
  assets,
  busy,
  detail,
  expanded,
  expandedAssetIds,
  mountStatusesByAssetId,
  onAssetReveal,
  onDelete,
  onEdit,
  onSetGroupMountProfile,
  onToggleAsset,
  onToggleExpanded,
  onToggleMount,
  onToggleSelected,
  profiles,
  selected,
  sourceById,
}: {
  appShortcuts: AppShortcut[];
  assets: Asset[];
  busy: boolean;
  detail: AssetGroupDetail;
  expanded: boolean;
  expandedAssetIds: Set<string>;
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  onAssetReveal: (path: string) => void;
  onDelete: () => void;
  onEdit: () => void;
  onSetGroupMountProfile: (profileId: string, enabled: boolean) => void;
  onToggleAsset: (assetId: string) => void;
  onToggleExpanded: () => void;
  onToggleMount: (assetId: string, profileId: string) => void;
  onToggleSelected: () => void;
  profiles: TargetProfile[];
  selected: boolean;
  sourceById: Map<string, Source>;
}) {
  const { t } = useI18n();
  const ruleSummary = summarizeRules(detail, sourceById, t);

  return (
    <article
      className={clsx(
        "border-theme-card-border transition-colors",
        "border-b last:border-b-0",
        expanded && "bg-theme-card-header/45",
      )}
    >
      <div className="grid min-h-20 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-4 px-4 py-3.5 hover:bg-theme-card-header/70 max-[760px]:grid-cols-[auto_minmax(0,1fr)]">
        <input
          aria-label={t("group.exclusive.selectGroup", { name: detail.group.name })}
          checked={selected}
          className="size-4 rounded border-theme-control-border accent-primary disabled:cursor-not-allowed disabled:opacity-40"
          disabled={busy || !detail.group.enabled}
          onChange={onToggleSelected}
          type="checkbox"
        />
        <div className="flex min-w-0 items-start gap-3">
          <GroupAvatar color={detail.group.color} enabled={detail.group.enabled} />
          <div className="min-w-0">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <h3 className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-code-md text-on-surface">
                {detail.group.name}
              </h3>
              <span className="rounded-md border border-theme-control-border bg-theme-control px-2 py-0.5 text-label-caps uppercase text-on-surface-variant">
                {t("group.memberCount", { count: assets.length })}
              </span>
              {!detail.group.enabled && (
                <span className="rounded-md border border-theme-control-border bg-theme-control px-2 py-0.5 text-label-caps uppercase text-outline">
                  {t("group.disabled")}
                </span>
              )}
            </div>
            <p className="mt-2 line-clamp-1 text-body-sm text-on-surface-variant">
              {detail.group.description || ruleSummary}
            </p>
          </div>
        </div>

        <div className="flex items-start gap-3 max-[1180px]:flex-col max-[1180px]:items-end max-[760px]:col-span-2 max-[760px]:ml-8 max-[760px]:items-start">
          <GroupBulkMountControls
            appShortcuts={appShortcuts}
            assets={assets}
            busy={busy}
            detail={detail}
            mountStatusesByAssetId={mountStatusesByAssetId}
            onSetGroupMountProfile={onSetGroupMountProfile}
            profiles={profiles}
          />
          <div className="flex items-start gap-1.5">
            <GroupIconButton disabled={busy} label={t("group.action.edit")} onClick={onEdit}>
              <Pencil size={16} />
            </GroupIconButton>
            <GroupIconButton disabled={busy} label={t("group.action.delete")} onClick={onDelete} danger>
              <Trash2 size={16} />
            </GroupIconButton>
            <GroupIconButton label={t(expanded ? "group.action.collapse" : "group.action.expand")} onClick={onToggleExpanded}>
              {expanded ? <ChevronDown size={17} /> : <ChevronRight size={17} />}
            </GroupIconButton>
          </div>
        </div>
      </div>

      {expanded && (
        <div className="border-t border-theme-card-border bg-theme-card-header/35 py-2 pl-8 pr-3">
          <div className="border-l border-outline-variant/70 pl-3">
            {assets.length === 0 ? (
              <div className="px-4 py-4 text-body-sm text-on-surface-variant">{t("group.emptyMembers")}</div>
            ) : (
              <div className="overflow-hidden rounded-xl border border-theme-card-border bg-theme-card/45">
                {assets.map((asset) => (
                  <AssetRow
                    appShortcuts={appShortcuts}
                    asset={asset}
                    expanded={expandedAssetIds.has(asset.id)}
                    key={asset.id}
                    mountStatuses={mountStatusesByAssetId.get(asset.id) ?? []}
                    onRevealPath={onAssetReveal}
                    onToggleExpanded={() => onToggleAsset(asset.id)}
                    onToggleMount={(profileId) => onToggleMount(asset.id, profileId)}
                    profiles={profiles}
                    source={sourceById.get(asset.source_id)}
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

function GroupIconButton({
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

function resolveGroupAssets(detail: AssetGroupDetail, assetById: Map<string, Asset>) {
  return groupMemberAssetIds(detail).flatMap((assetId) => {
    const asset = assetById.get(assetId);
    return asset ? [asset] : [];
  });
}

function summarizeRules(
  detail: AssetGroupDetail,
  sourceById: Map<string, Source>,
  t: Translator,
) {
  const sourceNames = detail.group.rules.source_ids
    .map((sourceId) => sourceById.get(sourceId)?.name ?? sourceId)
    .slice(0, 2);
  const rules = [
    ...sourceNames,
    ...detail.group.rules.relative_path_globs.slice(0, 2),
    detail.group.rules.name_contains ? `${t("group.rules.nameContains")}: ${detail.group.rules.name_contains}` : null,
  ].filter(Boolean);

  return rules.length > 0 ? rules.join(" · ") : t("group.emptyMembers");
}

function upsertGroupDetail(groups: AssetGroupDetail[], detail: AssetGroupDetail) {
  return [...groups.filter((candidate) => candidate.group.id !== detail.group.id), detail].sort((left, right) => {
    const sortOrder = left.group.sort_order - right.group.sort_order;
    return sortOrder === 0 ? left.group.name.localeCompare(right.group.name) : sortOrder;
  });
}

function compareMountItems(left: { asset_id: string; name: string }, right: { asset_id: string; name: string }) {
  return left.name.localeCompare(right.name) || left.asset_id.localeCompare(right.asset_id);
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
