import {
  AlertCircle,
  CheckCircle2,
  GitBranch,
  Loader2,
  RefreshCw,
  Search,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useCatalogTasks } from "../../app/backgroundTasks/CatalogTaskProvider";
import { useI18n } from "../../i18n/I18nProvider";
import type { GitSourcePullItemResult, Source } from "../../types";
import { DialogFrame } from "../foundation/DialogFrame";
import { EmptyState } from "../foundation/EmptyState";
import { Button } from "../ui/button";
import { Input } from "../ui/input";

export interface SourceGitUpdateDialogProps {
  onClose: () => void;
  onNotifyError: (message: string) => void;
  onSuccess?: () => Promise<void>;
  open: boolean;
  sources: Source[];
}

export interface GitRepositoryGroup {
  key: string;
  name: string;
  repoRoot: string;
  sourceIds: string[];
  sources: Source[];
}

export function isGitSource(source: Source): boolean {
  return source.source_origin === "git_repo" || source.kind === "git_checkout";
}

function getRepoRoot(source: Source): string {
  return source.repo_root?.trim() || source.root_path.trim();
}

function getRepoName(source: Source): string {
  const root = getRepoRoot(source);
  const segments = root.replace(/\\/g, "/").split("/").filter(Boolean);
  const folderName = segments[segments.length - 1];
  return folderName || source.name;
}

export function SourceGitUpdateDialog({
  onClose,
  onNotifyError,
  onSuccess,
  open,
  sources,
}: SourceGitUpdateDialogProps) {
  const { t } = useI18n();
  const { cancelGitSourcePull, gitSourcePull, startGitSourcePull } =
    useCatalogTasks();
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedKeys, setSelectedKeys] = useState<Set<string>>(new Set());
  const searchInputRef = useRef<HTMLInputElement>(null);
  const refreshedTaskIdRef = useRef<string | null>(null);

  const gitSources = useMemo(() => sources.filter(isGitSource), [sources]);

  const gitRepoGroups = useMemo(() => {
    const map = new Map<string, GitRepositoryGroup>();
    for (const source of gitSources) {
      const key = getRepoRoot(source);
      const existing = map.get(key);
      if (existing) {
        existing.sourceIds.push(source.id);
        existing.sources.push(source);
      } else {
        map.set(key, {
          key,
          name: getRepoName(source),
          repoRoot: key,
          sourceIds: [source.id],
          sources: [source],
        });
      }
    }
    return Array.from(map.values());
  }, [gitSources]);

  useEffect(() => {
    if (open) {
      setSelectedKeys(new Set(gitRepoGroups.map((g) => g.key)));
      setSearchQuery("");
    }
  }, [open, gitRepoGroups]);

  const filteredRepoGroups = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return gitRepoGroups;
    return gitRepoGroups.filter(
      (g) =>
        g.name.toLowerCase().includes(query) ||
        g.repoRoot.toLowerCase().includes(query) ||
        g.sources.some((s) => s.name.toLowerCase().includes(query)),
    );
  }, [gitRepoGroups, searchQuery]);

  const isRunning =
    gitSourcePull?.status === "running" ||
    gitSourcePull?.status === "cancelling";

  // Trigger catalog refresh once when task completes with changes
  useEffect(() => {
    if (
      gitSourcePull?.status === "completed" &&
      gitSourcePull.id !== refreshedTaskIdRef.current
    ) {
      refreshedTaskIdRef.current = gitSourcePull.id;
      const hasChanges = gitSourcePull.results.some(
        (r) => r.success && r.changed,
      );
      if (hasChanges && onSuccess) {
        void onSuccess();
      }
    }
  }, [gitSourcePull, onSuccess]);

  const resultMap = useMemo(() => {
    const map = new Map<string, GitSourcePullItemResult>();
    gitSourcePull?.results.forEach((res) => map.set(res.source_id, res));
    return map;
  }, [gitSourcePull?.results]);

  if (!open) {
    return null;
  }

  const allFilteredSelected =
    filteredRepoGroups.length > 0 &&
    filteredRepoGroups.every((g) => selectedKeys.has(g.key));

  const toggleSelectAll = () => {
    if (allFilteredSelected) {
      const next = new Set(selectedKeys);
      filteredRepoGroups.forEach((g) => next.delete(g.key));
      setSelectedKeys(next);
    } else {
      const next = new Set(selectedKeys);
      filteredRepoGroups.forEach((g) => next.add(g.key));
      setSelectedKeys(next);
    }
  };

  const toggleSelect = (key: string) => {
    const next = new Set(selectedKeys);
    if (next.has(key)) {
      next.delete(key);
    } else {
      next.add(key);
    }
    setSelectedKeys(next);
  };

  const handlePullSelected = async () => {
    if (selectedKeys.size === 0 || isRunning) return;
    const targetSourceIds = gitRepoGroups
      .filter((g) => selectedKeys.has(g.key))
      .flatMap((g) => g.sourceIds);
    if (targetSourceIds.length === 0) return;
    try {
      await startGitSourcePull(targetSourceIds);
    } catch (err) {
      onNotifyError(err instanceof Error ? err.message : String(err));
    }
  };

  const handlePullAll = async () => {
    if (gitRepoGroups.length === 0 || isRunning) return;
    const allSourceIds = gitRepoGroups.flatMap((g) => g.sourceIds);
    try {
      await startGitSourcePull(allSourceIds);
    } catch (err) {
      onNotifyError(err instanceof Error ? err.message : String(err));
    }
  };

  const handlePullOne = async (group: GitRepositoryGroup) => {
    if (isRunning) return;
    try {
      await startGitSourcePull(group.sourceIds);
    } catch (err) {
      onNotifyError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleCancel = async () => {
    if (!gitSourcePull || !isRunning) return;
    try {
      await cancelGitSourcePull(gitSourcePull.id);
    } catch (err) {
      onNotifyError(err instanceof Error ? err.message : String(err));
    }
  };

  const progressTotal =
    gitSourcePull?.progress.total || gitRepoGroups.length || 1;
  const progressCompleted = gitSourcePull?.progress.completed || 0;
  const progressPercent = Math.min(
    100,
    Math.round((progressCompleted / progressTotal) * 100),
  );

  return (
    <DialogFrame
      busy={isRunning}
      closeLabel={t("source.gitPull.close")}
      contentClassName="p-0 flex flex-col max-h-[75vh]"
      description={t("source.gitPull.dialogDescription")}
      footer={
        <div className="flex w-full flex-wrap items-center justify-between gap-3">
          <div className="text-body-xs text-on-surface-variant">
            {isRunning
              ? t("source.gitPull.backgroundNotice")
              : t("source.gitPull.selectedCount", {
                  selected: selectedKeys.size,
                  total: gitRepoGroups.length,
                })}
          </div>
          <div className="flex items-center gap-2">
            <Button onClick={onClose} type="button" variant="outline">
              {t("source.gitPull.close")}
            </Button>
            {isRunning && (
              <Button onClick={handleCancel} type="button" variant="outline">
                {t("source.gitPull.cancel")}
              </Button>
            )}
            <Button
              disabled={isRunning || gitRepoGroups.length === 0}
              onClick={() => void handlePullAll()}
              type="button"
              variant="outline"
            >
              <GitBranch size={15} />
              {t("source.gitPull.startAll")}
            </Button>
            <Button
              disabled={isRunning || selectedKeys.size === 0}
              onClick={() => void handlePullSelected()}
              type="button"
              variant="default"
            >
              <RefreshCw
                className={isRunning ? "animate-spin" : ""}
                size={15}
              />
              {t("source.gitPull.startSelected", {
                count: selectedKeys.size,
              })}
            </Button>
          </div>
        </div>
      }
      icon={<GitBranch size={18} />}
      iconClassName="border-primary/25 bg-primary/10 text-primary"
      initialFocusRef={searchInputRef}
      onClose={onClose}
      size="xl"
      title={t("source.gitPull.dialogTitle")}
    >
      <div className="flex flex-col gap-3 p-5">
        {/* Progress bar if running */}
        {isRunning && (
          <div className="flex flex-col gap-1.5 rounded-xl border border-primary/25 bg-primary/5 p-3">
            <div className="flex items-center justify-between text-body-xs font-medium">
              <span className="flex items-center gap-1.5 text-primary">
                <Loader2 className="size-3.5 animate-spin" />
                {t("source.gitPull.progress", {
                  completed: progressCompleted,
                  total: progressTotal,
                })}
              </span>
              <span className="font-mono text-outline">{progressPercent}%</span>
            </div>
            <div className="h-1.5 w-full overflow-hidden rounded-full bg-theme-control">
              <div
                className="h-full rounded-full bg-primary transition-all duration-300"
                style={{ width: `${progressPercent}%` }}
              />
            </div>
          </div>
        )}

        {/* Toolbar: Search + Select All */}
        {gitRepoGroups.length > 0 && (
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="relative min-w-48 flex-1 max-w-sm">
              <Search
                className="absolute left-3 top-1/2 -translate-y-1/2 text-outline"
                size={14}
              />
              <Input
                className="pl-8 text-body-sm"
                disabled={isRunning}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={t("source.gitPull.searchPlaceholder")}
                ref={searchInputRef}
                value={searchQuery}
              />
            </div>
            <div className="flex items-center gap-2">
              <Button
                disabled={isRunning || filteredRepoGroups.length === 0}
                onClick={toggleSelectAll}
                size="sm"
                type="button"
                variant="ghost"
              >
                {allFilteredSelected
                  ? t("source.gitPull.deselectAll")
                  : t("source.gitPull.selectAll")}
              </Button>
            </div>
          </div>
        )}

        {/* List of Git repository groups */}
        <div className="flex flex-1 flex-col gap-2 overflow-y-auto max-h-[46vh] pr-1">
          {gitRepoGroups.length === 0 ? (
            <EmptyState
              description={t("source.gitPull.noGitSources")}
              icon={<GitBranch size={22} />}
              title={t("source.gitPull.noGitSources")}
            />
          ) : filteredRepoGroups.length === 0 ? (
            <div className="rounded-xl border border-dashed border-theme-card-border p-6 text-center text-body-sm text-on-surface-variant">
              {t("source.toolbar.searchSubmit")}
            </div>
          ) : (
            filteredRepoGroups.map((group) => {
              const isCurrentPulling =
                isRunning &&
                group.sources.some(
                  (s) => gitSourcePull?.progress.current_source_name === s.name,
                );
              const isSelected = selectedKeys.has(group.key);

              // Find the latest result for this repository group
              const groupResults = group.sourceIds
                .map((id) => resultMap.get(id))
                .filter((r): r is GitSourcePullItemResult => Boolean(r));
              const groupResult =
                groupResults.find((r) => !r.success) ??
                groupResults.find((r) => r.changed) ??
                groupResults[0];

              return (
                <div
                  className="flex items-center justify-between gap-3 rounded-xl border border-theme-control-border bg-theme-control/60 p-3 transition-colors hover:bg-theme-control"
                  key={group.key}
                >
                  <label className="flex min-w-0 flex-1 cursor-pointer items-center gap-3">
                    <input
                      checked={isSelected}
                      className="size-4 shrink-0 rounded-md accent-primary"
                      disabled={isRunning}
                      onChange={() => toggleSelect(group.key)}
                      type="checkbox"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="truncate font-mono text-body-sm font-medium text-on-surface">
                          {group.name}
                        </span>
                        {group.sources.length > 1 && (
                          <span className="shrink-0 rounded-full border border-theme-control-border bg-theme-control px-2 py-0.5 text-label-xs text-on-surface-variant">
                            {t("source.gitPull.multiSourceBadge", {
                              count: group.sources.length,
                            })}
                          </span>
                        )}
                        {/* Status Badge */}
                        {isCurrentPulling && (
                          <span className="inline-flex shrink-0 items-center gap-1 rounded-full border border-primary/25 bg-primary/10 px-2 py-0.5 text-label-xs text-primary">
                            <Loader2 className="size-3 animate-spin" />
                            {t("source.gitPull.status.pulling")}
                          </span>
                        )}
                        {!isCurrentPulling && groupResult && (
                          <span
                            className={`inline-flex shrink-0 items-center gap-1 rounded-full border px-2 py-0.5 text-label-xs ${
                              groupResult.success
                                ? groupResult.changed
                                  ? "border-status-create/30 bg-status-create/10 text-status-create"
                                  : "border-theme-control-border bg-theme-control text-on-surface-variant"
                                : "border-status-remove/30 bg-status-remove/10 text-status-remove"
                            }`}
                          >
                            {groupResult.success ? (
                              groupResult.changed ? (
                                <GitBranch className="size-3" />
                              ) : (
                                <CheckCircle2 className="size-3" />
                              )
                            ) : (
                              <AlertCircle className="size-3" />
                            )}
                            {groupResult.success
                              ? groupResult.changed
                                ? t("source.gitPull.status.updated")
                                : t("source.gitPull.status.upToDate")
                              : t("source.gitPull.status.failed")}
                          </span>
                        )}
                      </div>
                      <div className="truncate font-mono text-label-xs text-outline">
                        {group.repoRoot}
                      </div>
                      {/* Pull result details message if any */}
                      {groupResult?.message && (
                        <div
                          className={`mt-1 line-clamp-2 font-mono text-label-xs ${
                            groupResult.success
                              ? "text-on-surface-variant"
                              : "text-status-remove"
                          }`}
                        >
                          {groupResult.message}
                        </div>
                      )}
                    </div>
                  </label>

                  <div className="flex shrink-0 items-center gap-1">
                    <Button
                      disabled={isRunning}
                      onClick={() => void handlePullOne(group)}
                      size="sm"
                      type="button"
                      variant="ghost"
                    >
                      <RefreshCw size={13} />
                      <span className="hidden sm:inline">
                        {t("source.gitPull.pullOne")}
                      </span>
                    </Button>
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>
    </DialogFrame>
  );
}
