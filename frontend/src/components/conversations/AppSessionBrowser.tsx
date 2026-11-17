import clsx from "clsx";
import { memo, useEffect, useLayoutEffect, useMemo, useRef } from "react";
import {
  AppWindow,
  ChevronRight,
  Folder,
  PackageCheck,
  RefreshCw,
} from "lucide-react";
import {
  RenderActivityProvider,
  RenderSafeScrollSurface,
  VirtualizedCollection,
  VIRTUALIZATION_MIN_ITEMS,
  renderingFlags,
} from "../common/rendering";
import {
  AppShortcutIconForShortcut,
  APP_SHORTCUT_ICON_FRAME_CLASS,
} from "../apps/AppShortcutIcon";
import { ResizableColumns } from "../layout/ResizableColumns";
import { useCollapsibleHeader } from "../common/CollapsibleHeader";
import { DEFAULT_COLUMN_MIN_WIDTH } from "../../store/settings/settingsSchema";
import type { Translator } from "../../i18n/I18nProvider";
import { abbreviateHomePath } from "../../utils/path";
import { conversationIdFragment } from "../../utils/conversationIds";
import { iconButtonRecipe } from "../../theme/recipes";
import type {
  AppKind,
  AppShortcut,
  ConversationRecordKind,
  ConversationSessionListItem,
} from "../../types";
import { ColumnPanel, EmptyPanel } from "./ConversationColumns";

/** 服务端静态渲染没有 layout effect；浏览器内仍用 layout effect 避免闪一帧收起态。 */
const useIsomorphicLayoutEffect =
  typeof window === "undefined" ? useEffect : useLayoutEffect;

export interface ConversationAppSummary {
  appKind: AppKind;
  id: string;
  name: string;
}

export interface ConversationAppSessionGroup {
  app: ConversationAppSummary;
  projectGroups: ConversationProjectSessionGroup[];
  sessions: ConversationSessionListItem[];
  questionCount: number;
  turnCount: number;
}

export interface ConversationProjectSessionGroup {
  key: string;
  projectPath: string | null;
  sessions: ConversationSessionListItem[];
  questionCount: number;
  turnCount: number;
}

export const AppSessionBrowser = memo(function AppSessionBrowser({
  active = true,
  appShortcuts,
  columnMinWidth = DEFAULT_COLUMN_MIN_WIDTH,
  completedAdapterIds,
  groups,
  onAppSelect,
  onProjectSelect,
  onSessionOpen,
  recordKind = "session",
  selectedAppId,
  selectedProjectKey,
  syncingAdapterIds,
  t,
}: {
  active?: boolean;
  appShortcuts: AppShortcut[];
  columnMinWidth?: number;
  completedAdapterIds?: Set<string>;
  groups: ConversationAppSessionGroup[];
  onAppSelect: (appId: string) => void;
  onProjectSelect: (projectKey: string) => void;
  onSessionOpen: (sessionId: string) => void;
  recordKind?: ConversationRecordKind;
  selectedAppId: string | null;
  selectedProjectKey: string | null;
  syncingAdapterIds?: Set<string>;
  t: Translator;
}) {
  const collapsible = useCollapsibleHeader();
  const sessionListScrollRef = useRef<HTMLDivElement>(null);
  const wasActiveRef = useRef(active);

  // 保活视图重新可见时展开折叠头，并用列表当前偏移作为滚动基线，
  // 否则返回后第一次向上回滚会被误判成向下滚动而再次收起。
  useIsomorphicLayoutEffect(() => {
    if (active && !wasActiveRef.current) {
      collapsible?.reset(sessionListScrollRef.current?.scrollTop ?? 0);
    }
    wasActiveRef.current = active;
  }, [active, collapsible]);
  const showProjectColumn = recordKind !== "web";
  const selectedGroup =
    groups.find((group) => group.app.id === selectedAppId) ?? null;
  const selectedProjectGroup = showProjectColumn
    ? (selectedGroup?.projectGroups.find(
        (group) => group.key === selectedProjectKey,
      ) ??
      selectedGroup?.projectGroups[0] ??
      null)
    : selectedGroup
      ? {
          key: selectedGroup.app.id,
          projectPath: null,
          sessions: selectedGroup.sessions,
          questionCount: selectedGroup.questionCount,
          turnCount: selectedGroup.turnCount,
        }
      : null;
  const selectedShortcut = selectedGroup
    ? findConversationAppShortcut(appShortcuts, selectedGroup.app)
    : null;
  const browserColumns = showProjectColumn
    ? [
        { defaultWeight: 0.3 },
        { defaultWeight: 0.62 },
        { defaultWeight: 1.08, minWidthScale: 1.25 },
      ]
    : [{ defaultWeight: 0.36 }, { defaultWeight: 1.64, minWidthScale: 1.25 }];
  const sessionHeaderTitle = showProjectColumn
    ? selectedProjectGroup
      ? projectGroupLabel(selectedProjectGroup, t)
      : t("conversation.project.select")
    : (selectedGroup?.app.name ?? t("conversation.app.select"));
  const emptySessionsMessage = showProjectColumn
    ? t("conversation.session.emptyForProject")
    : t("conversation.session.emptyForApp");
  const sessionListVirtualized =
    renderingFlags.conversationListVirtualization &&
    (selectedProjectGroup?.sessions.length ?? 0) >= VIRTUALIZATION_MIN_ITEMS;

  return (
    <ResizableColumns
      ariaLabel={t("layout.resizeColumns")}
      className="conversation-session-browser conversation-surface mt-5 min-h-0 flex-1 rounded-2xl shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.18)]"
      columns={browserColumns}
      handleClassName="max-[1040px]:hidden"
      minimumWidth={columnMinWidth}
      responsiveClassName="max-[1040px]:w-full max-[1040px]:grid-cols-1"
      scrollBarLabel={t("layout.scrollColumns")}
      scrollLeftLabel={t("layout.scrollColumnsLeft")}
      scrollRightLabel={t("layout.scrollColumnsRight")}
      storageKey={
        showProjectColumn
          ? "assetiweave.conversationBrowserColumns.v2"
          : "assetiweave.webRecordBrowserColumns.v1"
      }
    >
      <ColumnPanel
        title={t("conversation.column.apps")}
        icon={<AppWindow size={16} />}
      >
        {groups.length === 0 ? (
          <EmptyPanel>{t("conversation.app.empty")}</EmptyPanel>
        ) : (
          groups.map((group) => (
            <AppListItem
              completed={completedAdapterIds?.has(group.app.id)}
              group={group}
              key={group.app.id}
              onSelect={() => onAppSelect(group.app.id)}
              selected={group.app.id === selectedAppId}
              shortcut={findConversationAppShortcut(appShortcuts, group.app)}
              syncing={syncingAdapterIds?.has(group.app.id)}
              t={t}
            />
          ))
        )}
      </ColumnPanel>
      {showProjectColumn ? (
        <ColumnPanel
          title={t("conversation.column.projects")}
          icon={<Folder size={16} />}
        >
          {!selectedGroup ? (
            <EmptyPanel>{t("conversation.app.select")}</EmptyPanel>
          ) : selectedGroup.projectGroups.length === 0 ? (
            <EmptyPanel>{t("conversation.session.emptyForApp")}</EmptyPanel>
          ) : (
            selectedGroup.projectGroups.map((group) => (
              <ProjectListItem
                key={group.key}
                group={group}
                onSelect={() => onProjectSelect(group.key)}
                selected={group.key === selectedProjectGroup?.key}
                t={t}
              />
            ))
          )}
        </ColumnPanel>
      ) : null}
      <section className="conversation-column flex h-full min-h-0 flex-col">
        <header className="conversation-column-header flex min-h-16 shrink-0 items-center justify-between gap-4 px-5 py-3">
          <div className="flex min-w-0 items-center gap-3">
            {selectedGroup ? (
              <ConversationAppIcon
                appName={selectedGroup.app.name}
                shortcut={selectedShortcut}
              />
            ) : null}
            <div className="min-w-0">
              <p className="text-label-caps text-primary">
                {t("conversation.column.sessions")}
              </p>
              <h2 className="mt-1 truncate text-title-sm text-on-surface">
                {sessionHeaderTitle}
              </h2>
            </div>
          </div>
          {selectedProjectGroup ? (
            <span className="shrink-0 rounded-full bg-theme-control px-3 py-1 text-code-sm text-on-surface-variant">
              {t("conversation.project.summary", {
                questions: selectedProjectGroup.questionCount,
                sessions: selectedProjectGroup.sessions.length,
                turns: selectedProjectGroup.turnCount,
              })}
            </span>
          ) : null}
        </header>
        <RenderSafeScrollSurface
          className="min-h-0 flex-1 p-4"
          ref={sessionListScrollRef}
        >
          {!selectedGroup ? (
            <EmptyPanel>{t("conversation.app.select")}</EmptyPanel>
          ) : showProjectColumn && selectedGroup.projectGroups.length === 0 ? (
            <EmptyPanel>{t("conversation.session.emptyForApp")}</EmptyPanel>
          ) : !selectedProjectGroup ? (
            <EmptyPanel>
              {showProjectColumn
                ? t("conversation.project.select")
                : t("conversation.app.select")}
            </EmptyPanel>
          ) : selectedProjectGroup.sessions.length === 0 ? (
            <EmptyPanel>{emptySessionsMessage}</EmptyPanel>
          ) : (
            <RenderActivityProvider scrollElementRef={sessionListScrollRef}>
              <div className="render-safe-scroll-content grid gap-3">
                <VirtualizedCollection
                  className={sessionListVirtualized ? undefined : "grid gap-3"}
                  contentVisibilityContainmentEnabled={
                    renderingFlags.sessionCardContentVisibilityContainment
                  }
                  deferredRenderingEnabled={
                    renderingFlags.deferredSkeletonRendering &&
                    sessionListVirtualized
                  }
                  enabled={renderingFlags.conversationListVirtualization}
                  estimateSize={132}
                  fallback={() => <SessionCardSkeleton />}
                  getItemKey={(session) => session.id}
                  gap={12}
                  items={selectedProjectGroup.sessions}
                  minItems={VIRTUALIZATION_MIN_ITEMS}
                  renderItem={(session) => (
                    <SessionCard
                      onOpen={() => onSessionOpen(session.id)}
                      session={session}
                      showProjectPath={showProjectColumn}
                      t={t}
                    />
                  )}
                  scrollElementRef={sessionListScrollRef}
                  size="regular"
                />
              </div>
            </RenderActivityProvider>
          )}
        </RenderSafeScrollSurface>
      </section>
    </ResizableColumns>
  );
});

function SessionCardSkeleton() {
  return (
    <div
      aria-hidden="true"
      className="conversation-session-card grid min-h-32 animate-pulse gap-3 border px-4 py-4"
    >
      <div className="h-4 w-2/3 rounded bg-theme-control" />
      <div className="h-3 w-4/5 rounded bg-theme-control/75" />
      <div className="h-7 w-1/2 rounded-xl bg-theme-control/60" />
    </div>
  );
}
function ProjectListItem({
  group,
  onSelect,
  selected,
  t,
}: {
  group: ConversationProjectSessionGroup;
  onSelect: () => void;
  selected: boolean;
  t: Translator;
}) {
  const label = projectGroupLabel(group, t);

  return (
    <button
      aria-label={t("conversation.project.selectNamed", { path: label })}
      aria-pressed={selected}
      className={`conversation-row grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-start gap-3 px-3 py-3 text-left ${selected ? "text-on-surface" : ""}`}
      data-selected={selected}
      onClick={onSelect}
      type="button"
    >
      <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-xl border border-theme-control-border bg-theme-control text-primary">
        <Folder size={16} />
      </span>
      <span className="min-w-0">
        <span className="block truncate font-mono text-code-sm font-semibold text-on-surface">
          {label}
        </span>
        <span className="mt-1 block text-code-sm text-on-surface-variant">
          {t("conversation.project.sessionCount", {
            count: group.sessions.length,
          })}
        </span>
      </span>
      <ChevronRight
        className={selected ? "text-primary" : "text-on-surface-muted"}
        size={16}
      />
    </button>
  );
}
function projectGroupLabel(
  group: ConversationProjectSessionGroup,
  t: Translator,
) {
  return group.projectPath
    ? abbreviateHomePath(group.projectPath)
    : t("conversation.session.noProject");
}
function AppListItem({
  completed,
  group,
  onSelect,
  selected,
  shortcut,
  syncing,
  t,
}: {
  completed?: boolean;
  group: ConversationAppSessionGroup;
  onSelect: () => void;
  selected: boolean;
  shortcut: AppShortcut | null;
  syncing?: boolean;
  t: Translator;
}) {
  const isSyncing = Boolean(syncing && !completed);
  const isCompleted = Boolean(syncing && completed);

  return (
    <button
      aria-label={t("conversation.app.selectNamed", { name: group.app.name })}
      aria-pressed={selected}
      className={`conversation-row grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 px-3 py-3 text-left ${selected ? "text-on-surface" : ""}`}
      data-selected={selected}
      onClick={onSelect}
      type="button"
    >
      <ConversationAppIcon appName={group.app.name} shortcut={shortcut} />
      <span className="min-w-0">
        <span className="block truncate text-body-sm font-semibold text-on-surface">
          {group.app.name}
        </span>
        <span className="mt-1 flex items-center gap-1.5 text-code-sm text-on-surface-variant">
          <span>
            {t("conversation.app.sessionCount", {
              count: group.sessions.length,
            })}
          </span>
          {isSyncing ? (
            <span className="inline-flex items-center gap-1 rounded-full bg-primary/10 px-1.5 py-0.5 text-[11px] font-medium text-primary">
              <RefreshCw className="size-2.5 animate-spin" />
              <span>{t("conversation.app.syncing")}</span>
            </span>
          ) : isCompleted ? (
            <span className="inline-flex items-center gap-1 rounded-full bg-status-create/15 px-1.5 py-0.5 text-[11px] font-medium text-status-create">
              <PackageCheck className="size-2.5" />
              <span>{t("conversation.app.synced")}</span>
            </span>
          ) : null}
        </span>
      </span>
      <ChevronRight
        className={selected ? "text-primary" : "text-on-surface-muted"}
        size={16}
      />
    </button>
  );
}
function ConversationAppIcon({
  appName,
  shortcut,
}: {
  appName: string;
  shortcut: AppShortcut | null;
}) {
  if (!shortcut) {
    return (
      <span className="grid size-9 shrink-0 place-items-center rounded-xl border border-theme-control-border bg-theme-control text-primary">
        <AppWindow size={17} />
      </span>
    );
  }

  return (
    <span
      aria-label={appName}
      className={clsx(
        APP_SHORTCUT_ICON_FRAME_CLASS,
        "size-9 shrink-0 text-[13px] font-bold",
      )}
      style={{
        borderColor: `${shortcut.accentColor}66`,
        backgroundColor: `${shortcut.accentColor}1f`,
        color: shortcut.accentColor,
      }}
    >
      <AppShortcutIconForShortcut className="size-5" shortcut={shortcut} />
    </span>
  );
}
export function findConversationAppShortcut(
  shortcuts: AppShortcut[],
  app: ConversationAppSummary,
) {
  return (
    shortcuts.find((shortcut) => shortcut.profileId === app.id) ??
    shortcuts.find(
      (shortcut) =>
        app.appKind !== "custom" && shortcut.appKind === app.appKind,
    ) ??
    null
  );
}
function SessionCard({
  onOpen,
  session,
  showProjectPath = true,
  t,
}: {
  onOpen: () => void;
  session: ConversationSessionListItem;
  showProjectPath?: boolean;
  t: Translator;
}) {
  const idFragment = conversationIdFragment(session.id);

  return (
    <article className="conversation-session-card group grid w-full grid-cols-[minmax(0,1fr)_auto] items-center gap-4 border px-4 py-4 text-left">
      <span className="min-w-0 select-text">
        <span className="block truncate text-body-sm font-semibold text-on-surface">
          {session.title}
        </span>
        {showProjectPath ? (
          <span className="mt-1 block truncate font-mono text-code-sm text-on-surface-variant">
            {session.project_path || session.last_cwd
              ? abbreviateHomePath(session.project_path || session.last_cwd!)
              : t("conversation.session.noProject")}
          </span>
        ) : null}
        <SessionMetaChips
          executionOrigin={session.execution_origin}
          executionPurpose={session.execution_purpose}
          idFragment={idFragment}
          questions={session.question_count}
          sourceLocator={session.source_locator}
          t={t}
          turns={session.turn_count}
        />
      </span>
      <button
        aria-label={t("conversation.session.open", { title: session.title })}
        className={iconButtonRecipe({
          className:
            "size-9 shrink-0 border border-theme-control-border bg-theme-control text-on-surface-variant group-hover:text-primary",
        })}
        onClick={onOpen}
        type="button"
      >
        <ChevronRight size={17} />
      </button>
    </article>
  );
}
function SessionMetaChips({
  executionOrigin,
  executionPurpose,
  idFragment,
  questions,
  sourceLocator,
  t,
  turns,
}: {
  executionOrigin?: string;
  executionPurpose?: string | null;
  idFragment: string;
  questions: number;
  sourceLocator?: string | null;
  t: Translator;
  turns: number;
}) {
  const originBadge = useMemo(() => {
    const raw = [executionOrigin, executionPurpose, sourceLocator]
      .filter((v): v is string => Boolean(v))
      .join(" ")
      .toLowerCase();
    if (raw.includes("workbuddy")) {
      return {
        label: "WorkBuddy",
        className: "border border-primary/20 bg-primary/10 text-primary",
      };
    }
    if (
      raw.includes("codebuddy-cli") ||
      raw.includes(".codebuddy") ||
      (raw.includes("cli") && !raw.includes("client"))
    ) {
      return {
        label: "CLI",
        className:
          "border border-theme-control-border bg-theme-control text-on-surface-variant",
      };
    }
    return null;
  }, [executionOrigin, executionPurpose, sourceLocator]);

  return (
    <span
      aria-label={t("conversation.session.counts", { questions, turns })}
      className="mt-3 flex min-w-0 flex-wrap items-center gap-1.5"
    >
      {originBadge ? (
        <span
          className={`inline-flex h-7 items-center rounded-full px-2.5 text-code-sm font-medium ${originBadge.className}`}
        >
          {originBadge.label}
        </span>
      ) : null}
      <SessionMetaChip className="font-mono" label={idFragment} />
      <SessionMetaChip
        label={t("conversation.session.questionCountChip", {
          count: questions,
        })}
      />
      <SessionMetaChip
        label={t("conversation.session.turnCountChip", { count: turns })}
      />
    </span>
  );
}
function SessionMetaChip({
  className = "",
  label,
}: {
  className?: string;
  label: string;
}) {
  return (
    <span
      className={`inline-flex h-7 max-w-full items-center rounded-xl border border-theme-control-border bg-theme-control/80 px-2.5 text-code-sm font-medium text-on-surface-variant shadow-[var(--theme-shadow-control-inset)] ${className}`}
    >
      {label}
    </span>
  );
}
