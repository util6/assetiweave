import {
  closestCenter,
  DndContext,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import clsx from "clsx";
import {
  Activity,
  Bell,
  Bot,
  Code2,
  Columns3,
  Cpu,
  ChevronDown,
  ChevronRight,
  Database,
  FileJson,
  FolderOpen,
  Gauge,
  GripVertical,
  Languages,
  ListTree,
  Menu,
  MousePointerClick,
  Palette,
  PanelLeft,
  PanelTop,
  Puzzle,
  RefreshCw,
  RotateCcw,
  Settings,
  Sparkles,
  Terminal,
  Type,
  X,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { DialogFrame } from "@/components/foundation/DialogFrame";
import {
  AppShortcutIcon,
  AppShortcutIconForShortcut,
  APP_SHORTCUT_ICON_FRAME_CLASS,
  appIconToken,
  appShortcutIconFrameStyle,
  resolveAppIconKey,
  shortcutCustomIconText,
  shortcutUsesAppIcon,
} from "../apps/AppShortcutIcon";
import { appShortcutIconCatalog } from "../../config/appShortcutIcons";
import { SkillBackupDirectorySetting } from "../backup/SkillBackupDirectorySetting";
import { SkillBackupLibraryDialog } from "../backup/SkillBackupLibraryDialog";
import { ConfirmDialog } from "../common/ConfirmDialog";
import { AgentCapabilityDialog } from "./AgentCapabilityDialog";
import { AgentSettingsPanel } from "./AgentSettingsPanel";
import { AgentCapabilitySetting } from "./AgentCapabilitySetting";
import { ConversationFullSyncProgress } from "./ConversationFullSyncProgress";
import { conversationCardColor, conversationCardLabel } from "../conversations/ConversationContentCards";
import {
  isRedundantConversationCardKind,
  useConversationCardKindRegistry,
} from "../conversations/ConversationCardKindRegistry";
import { useI18n, type Translator } from "../../i18n/I18nProvider";
import { headerTabLabel, railLabel, subNavLabel } from "../../i18n/navigation";
import type { Locale, TranslationKey } from "../../i18n/messages";
import type { HeaderTabItem, LocalizedNavigationLabels, NavigationModel, RailMenuItem, SubNavItem } from "../../router/types";
import { getCliToolsStatus, installCliTools, type CliToolsStatus } from "../../services/cliTools";
import { getSkillBackupSettings, revealPath, selectTargetDirectory } from "../../services/catalog";
import {
  listConversationAdapterRuntimeStatuses,
  type ConversationAdapterRuntimeStatus,
} from "../../services/conversations";
import type { ThemeId } from "../../theme/schema";
import { isHexColor } from "../../theme/colorValidation";
import { themeOptions } from "../../theme/themes";
import {
  AUTO_DREAM_MIN_HOURS_MAX,
  AUTO_DREAM_MIN_HOURS_MIN,
  AUTO_DREAM_MIN_SESSIONS_MAX,
  AUTO_DREAM_MIN_SESSIONS_MIN,
  COLUMN_MIN_WIDTH_MAX,
  COLUMN_MIN_WIDTH_MIN,
  COLUMN_MIN_WIDTH_STEP,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  FONT_SIZE_STEP,
  RESULT_PREVIEW_LINE_LIMIT_MAX,
  RESULT_PREVIEW_LINE_LIMIT_MIN,
  RESULT_PREVIEW_LINE_LIMIT_STEP,
  firstFontFamilyName,
  fontFamilyOptions,
  normalizeConversationTranslationTargetLanguage,
  PROMPT_OPTIMIZATION_PROMPT_TEMPLATE_MAX_LENGTH,
  resolveFontFamilyCss,
  type AgentCapabilityServiceId,
  TRANSLATION_PROMPT_TEMPLATE_MAX_LENGTH,
  TRANSLATION_TARGET_LANGUAGE_MAX_LENGTH,
  useAppSettings,
  type ConversationTranslationProvider,
  type ConversationRuntimeOverrideSettings,
  type ConversationContentCardColorSettings,
  type FontFallbackKind,
  type FontFamilyPresetId,
  type FontFamilyValue,
  type InterfaceDensity,
  type SettingsPanelId,
} from "../../store/settings/AppSettingsProvider";
import type { AppShortcut, AppShortcutIconSvg, SkillBackupSettings } from "../../types";
import { abbreviateHomePath } from "../../utils/path";
import { useConversationSync } from "../../app/backgroundTasks/ConversationSyncProvider";

interface SettingsPanelConfig {
  id: SettingsPanelId;
  icon: LucideIcon;
  label: string;
}

interface SettingsGroupConfig {
  id: string;
  label: string;
  scope: string;
  panels: SettingsPanelConfig[];
}

function normalizeSettingsPanelId(panelId: SettingsPanelId): SettingsPanelId {
  return panelId === "general.agents" ? "agents.market" : panelId;
}

export function GlobalSettingsDialog({
  appShortcuts,
  initialPanel = "general.appearance",
  navigationModel,
  onClose,
  onAppShortcutsChange,
  onNavigationModelChange,
  onSkillBackupLibraryChange,
  open,
}: {
  appShortcuts: AppShortcut[];
  initialPanel?: SettingsPanelId;
  navigationModel: NavigationModel;
  onClose: () => void;
  onAppShortcutsChange: (shortcuts: AppShortcut[]) => void;
  onNavigationModelChange: (model: NavigationModel) => void;
  onSkillBackupLibraryChange?: () => Promise<void> | void;
  open: boolean;
}) {
  const { locale, setLocale, t } = useI18n();
  const { resetSettings, settings, storageInfo, updateSetting } = useAppSettings();
  const { startSync: startConversationSync, tasks: conversationSyncTasks } = useConversationSync();
  const [activePanel, setActivePanel] = useState<SettingsPanelId>(() => normalizeSettingsPanelId(initialPanel));
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());
  const [editingShortcutIconId, setEditingShortcutIconId] = useState<string | null>(null);
  const [iconSvgDraft, setIconSvgDraft] = useState("");
  const [iconSvgError, setIconSvgError] = useState("");
  const [backupDialogOpen, setBackupDialogOpen] = useState(false);
  const [backupError, setBackupError] = useState("");
  const [backupSettings, setBackupSettings] = useState<SkillBackupSettings | null>(null);
  const [cliToolsStatus, setCliToolsStatus] = useState<CliToolsStatus | null>(null);
  const [cliToolsError, setCliToolsError] = useState("");
  const [cliToolsInstalling, setCliToolsInstalling] = useState(false);
  const [adapterRuntimeStatuses, setAdapterRuntimeStatuses] = useState<ConversationAdapterRuntimeStatus[]>([]);
  const [adapterRuntimeLoading, setAdapterRuntimeLoading] = useState(false);
  const [adapterRuntimeError, setAdapterRuntimeError] = useState("");
  const [fullSyncConfirmOpen, setFullSyncConfirmOpen] = useState(false);
  const [fullSyncStarting, setFullSyncStarting] = useState(false);
  const [fullSyncError, setFullSyncError] = useState("");
  const [agentFocusId, setAgentFocusId] = useState<string | null>(null);
  const [agentCapabilityDialog, setAgentCapabilityDialog] = useState<{
    agentId: string;
    model?: string;
    serviceId: AgentCapabilityServiceId;
  } | null>(null);

  useEffect(() => {
    if (open) {
      const nextPanel = normalizeSettingsPanelId(initialPanel);
      setActivePanel(nextPanel);
      ensureGroupExpanded(nextPanel);
    }
  }, [initialPanel, open]);

  function toggleGroupCollapsed(groupId: string) {
    setCollapsedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(groupId)) {
        next.delete(groupId);
      } else {
        next.add(groupId);
      }
      return next;
    });
  }

  function ensureGroupExpanded(panelId: SettingsPanelId) {
    const group = settingGroups.find((candidate) =>
      candidate.panels.some((panel) => panel.id === panelId),
    );
    if (!group) {
      return;
    }
    setCollapsedGroups((prev) => {
      if (!prev.has(group.id)) {
        return prev;
      }
      const next = new Set(prev);
      next.delete(group.id);
      return next;
    });
  }

  useEffect(() => {
    if (!open) {
      return;
    }

    let cancelled = false;
    getSkillBackupSettings()
      .then((nextSettings) => {
        if (!cancelled) {
          setBackupError("");
          setBackupSettings(nextSettings);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setBackupError(errorMessage(error));
        }
      });
    getCliToolsStatus()
      .then((status) => {
        if (!cancelled) {
          setCliToolsStatus(status);
          setCliToolsError("");
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setCliToolsError(errorMessage(error));
        }
      });
    setAdapterRuntimeLoading(true);
    listConversationAdapterRuntimeStatuses()
      .then((statuses) => {
        if (!cancelled) {
          setAdapterRuntimeStatuses(statuses);
          setAdapterRuntimeError("");
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setAdapterRuntimeError(errorMessage(error));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setAdapterRuntimeLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [open]);

  useEffect(() => {
    if (!open) {
      closeShortcutIconEditor();
      return;
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose, open]);

  if (!open) {
    return null;
  }

  const settingGroups: SettingsGroupConfig[] = [
    {
      id: "general",
      label: t("settings.group.general"),
      scope: t("settings.scope.general"),
      panels: [
        { id: "general.appearance", icon: Palette, label: t("settings.section.appearance") },
        { id: "general.memory", icon: Cpu, label: t("settings.section.memory") },
        { id: "general.promptOptimization", icon: Sparkles, label: t("settings.section.promptOptimization") },
        { id: "general.typography", icon: Type, label: t("settings.section.typography") },
        { id: "general.storage", icon: FileJson, label: t("settings.section.storage") },
      ],
    },
    {
      id: "workspace",
      label: t("settings.group.workspace"),
      scope: t("settings.scope.workspace"),
      panels: [
        { id: "workspace.menu", icon: Menu, label: t("settings.section.menu") },
        { id: "workspace.shortcuts", icon: MousePointerClick, label: t("settings.section.shortcuts") },
      ],
    },
    {
      id: "agents",
      label: t("settings.group.agents"),
      scope: t("settings.scope.agents"),
      panels: [
        { id: "agents.market", icon: Bot, label: t("settings.section.acpMarket") },
        { id: "agents.settings", icon: Settings, label: t("settings.section.acpSettings") },
      ],
    },
    {
      id: "conversations",
      label: t("settings.group.conversations"),
      scope: t("settings.scope.conversations"),
      panels: [
        { id: "conversations.sessions", icon: ListTree, label: t("settings.section.conversationSessions") },
        { id: "conversations.translation", icon: Languages, label: t("settings.section.conversationTranslation") },
        { id: "conversations.adapters", icon: Puzzle, label: t("settings.section.conversationAdapters") },
      ],
    },
  ];
  const activePanelConfig =
    settingGroups.flatMap((group) => group.panels).find((panel) => panel.id === activePanel) ??
    settingGroups[0].panels[0];
  const activeScope =
    settingGroups.find((group) => group.panels.some((panel) => panel.id === activePanel))?.scope ??
    t("settings.scope.general");
  const configurableRailItems = navigationModel.railItems.filter(isConfigurableRailItem);

  function commitNavigationModel(nextNavigationModel: NavigationModel) {
    onNavigationModelChange(nextNavigationModel);
  }

  function commitAppShortcuts(nextAppShortcuts: AppShortcut[]) {
    onAppShortcutsChange(nextAppShortcuts);
  }

  function openAgentSettings(agentId: string) {
    setAgentCapabilityDialog(null);
    setAgentFocusId(agentId);
    setActivePanel("agents.market");
    ensureGroupExpanded("agents.market");
  }

  function openAgentCapabilityDialog(serviceId: AgentCapabilityServiceId) {
    const agentId = settings.agentCapabilityAssignments[serviceId];
    setAgentCapabilityDialog({
      agentId,
      model: settings.agentModels[agentId],
      serviceId,
    });
  }

  function selectAgentCapability(agentId: string) {
    if (!agentCapabilityDialog) {
      return;
    }
    updateSetting("agentCapabilityAssignments", {
      ...settings.agentCapabilityAssignments,
      [agentCapabilityDialog.serviceId]: agentId,
    });
    setAgentCapabilityDialog(null);
  }

  function updateRailItem(id: string, patch: Partial<RailMenuItem>) {
    commitNavigationModel({
      ...navigationModel,
      railItems: navigationModel.railItems.map((item) => (item.id === id ? { ...item, ...patch } : item)),
    });
  }

  function updateHeaderTab(id: string, patch: Partial<HeaderTabItem>) {
    commitNavigationModel({
      ...navigationModel,
      headerTabs: navigationModel.headerTabs.map((item) => (item.id === id ? { ...item, ...patch } : item)),
    });
  }

  function updateSubNavItem(parentTabId: string, id: string, patch: Partial<SubNavItem>) {
    commitNavigationModel({
      ...navigationModel,
      subNavItems: {
        ...navigationModel.subNavItems,
        [parentTabId]: (navigationModel.subNavItems[parentTabId] ?? []).map((item) => (item.id === id ? { ...item, ...patch } : item)),
      },
    });
  }

  function updateRailItemLabel(id: string, label: string) {
    commitNavigationModel({
      ...navigationModel,
      railItems: navigationModel.railItems.map((item) =>
        item.id === id ? { ...item, labels: setLocalizedNavigationLabel(item.labels, locale, label) } : item,
      ),
    });
  }

  function updateHeaderTabLabel(id: string, label: string) {
    commitNavigationModel({
      ...navigationModel,
      headerTabs: navigationModel.headerTabs.map((item) =>
        item.id === id ? { ...item, labels: setLocalizedNavigationLabel(item.labels, locale, label) } : item,
      ),
    });
  }

  function updateSubNavItemLabel(parentTabId: string, id: string, label: string) {
    commitNavigationModel({
      ...navigationModel,
      subNavItems: {
        ...navigationModel.subNavItems,
        [parentTabId]: (navigationModel.subNavItems[parentTabId] ?? []).map((item) =>
          item.id === id ? { ...item, labels: setLocalizedNavigationLabel(item.labels, locale, label) } : item,
        ),
      },
    });
  }

  function reorderRailItems(position: RailMenuItem["position"], orderedIds: string[]) {
    const itemById = new Map(navigationModel.railItems.map((item) => [item.id, item]));
    const orderedItems = orderedIds.flatMap((id) => {
      const item = itemById.get(id);
      return item ? [item] : [];
    });
    let inserted = false;
    const nextRailItems = navigationModel.railItems.flatMap((item) => {
      if (item.position !== position) {
        return [item];
      }
      if (inserted) {
        return [];
      }
      inserted = true;
      return orderedItems;
    });

    commitNavigationModel({
      ...navigationModel,
      railItems: nextRailItems,
    });
  }

  function reorderHeaderTabs(orderedIds: string[]) {
    const itemById = new Map(navigationModel.headerTabs.map((item) => [item.id, item]));
    commitNavigationModel({
      ...navigationModel,
      headerTabs: orderedIds.flatMap((id) => {
        const item = itemById.get(id);
        return item ? [item] : [];
      }),
    });
  }

  function reorderSubNavItems(parentTabId: string, orderedIds: string[]) {
    const items = navigationModel.subNavItems[parentTabId] ?? [];
    const itemById = new Map(items.map((item) => [item.id, item]));
    commitNavigationModel({
      ...navigationModel,
      subNavItems: {
        ...navigationModel.subNavItems,
        [parentTabId]: orderedIds.flatMap((id) => {
          const item = itemById.get(id);
          return item ? [item] : [];
        }),
      },
    });
  }

  function updateAppShortcut(profileId: string, patch: Partial<AppShortcut>) {
    commitAppShortcuts(appShortcuts.map((shortcut) => (shortcut.profileId === profileId ? { ...shortcut, ...patch } : shortcut)));
  }

  function reorderAppShortcuts(orderedIds: string[]) {
    const shortcutById = new Map(appShortcuts.map((shortcut) => [shortcut.profileId, shortcut]));
    commitAppShortcuts(
      orderedIds.flatMap((id) => {
        const shortcut = shortcutById.get(id);
        return shortcut ? [shortcut] : [];
      }),
    );
  }

  async function chooseDataBackupDirectory() {
    const selected = await selectTargetDirectory(t("settings.storage.pickDataBackupDir"));
    if (!selected) {
      return;
    }

    updateSetting("dataBackup", {
      ...settings.dataBackup,
      customDirectory: selected,
    });
  }

  function updateConversationTranslation(
    patch: Partial<typeof settings.conversationTranslation>,
  ) {
    updateSetting("conversationTranslation", {
      ...settings.conversationTranslation,
      ...patch,
    });
  }

  function updatePromptOptimization(
    patch: Partial<typeof settings.promptOptimization>,
  ) {
    updateSetting("promptOptimization", {
      ...settings.promptOptimization,
      ...patch,
    });
  }

  function clearDataBackupDirectory() {
    updateSetting("dataBackup", {
      ...settings.dataBackup,
      customDirectory: "",
    });
  }

  function updateConversationRuntimeOverride(
    key: keyof ConversationRuntimeOverrideSettings,
    value: string,
  ) {
    updateSetting("conversationRuntimeOverrides", {
      ...settings.conversationRuntimeOverrides,
      [key]: value,
    });
  }

  async function handleInstallCliTools() {
    setCliToolsInstalling(true);
    setCliToolsError("");
    try {
      setCliToolsStatus(await installCliTools());
    } catch (error) {
      setCliToolsError(errorMessage(error));
    } finally {
      setCliToolsInstalling(false);
    }
  }

  async function refreshAdapterRuntimeStatuses() {
    setAdapterRuntimeLoading(true);
    setAdapterRuntimeError("");
    try {
      setAdapterRuntimeStatuses(await listConversationAdapterRuntimeStatuses());
    } catch (error) {
      setAdapterRuntimeError(errorMessage(error));
    } finally {
      setAdapterRuntimeLoading(false);
    }
  }

  function openShortcutIconEditor(shortcut: AppShortcut) {
    setEditingShortcutIconId(shortcut.profileId);
    setIconSvgDraft(shortcut.iconSvg ? stringifyIconSvg(shortcut.iconSvg) : "");
    setIconSvgError("");
  }

  function closeShortcutIconEditor() {
    setEditingShortcutIconId(null);
    setIconSvgDraft("");
    setIconSvgError("");
  }

  function saveShortcutIconSvg() {
    const shortcut = editingShortcutIcon;
    if (!shortcut) {
      return;
    }

    const input = iconSvgDraft.trim();
    const result = parseIconSvgInput(input);
    if (result.iconSvg) {
      updateAppShortcut(shortcut.profileId, { iconSvg: result.iconSvg });
      closeShortcutIconEditor();
      return;
    }

    if (isPlainShortcutIconInput(input)) {
      updateAppShortcut(shortcut.profileId, {
        displayIcon: input.slice(0, 4),
        iconSvg: null,
      });
      closeShortcutIconEditor();
      return;
    }

    const appKey = resolveAppIconKey(input) || resolveAppIconKey(shortcut);
    if (!input && appKey) {
      updateAppShortcut(shortcut.profileId, {
        displayIcon: appIconToken(appKey),
        iconSvg: null,
      });
      closeShortcutIconEditor();
      return;
    }

    if (!input) {
      setIconSvgError(t("settings.shortcuts.iconCodeEmptyError"));
    } else {
      setIconSvgError(t("settings.shortcuts.svgError"));
    }
  }

  function clearShortcutIconSvg() {
    const shortcut = editingShortcutIcon;
    if (!shortcut) {
      return;
    }

    const appKey = resolveAppIconKey(shortcut);
    updateAppShortcut(shortcut.profileId, {
      ...(appKey ? { displayIcon: appIconToken(appKey) } : {}),
      iconSvg: null,
    });
    closeShortcutIconEditor();
  }

  async function startFullConversationSync() {
    setFullSyncStarting(true);
    setFullSyncError("");
    try {
      const snapshot = await startConversationSync({
        dry_run: false,
        mode: "full",
        record_kind: null,
      });
      if (snapshot.mode !== "full") {
        throw new Error(t("settings.conversation.fullSyncConflict"));
      }
      setFullSyncConfirmOpen(false);
    } catch (error) {
      setFullSyncError(errorMessage(error));
    } finally {
      setFullSyncStarting(false);
    }
  }

  const editingShortcutIcon = appShortcuts.find((shortcut) => shortcut.profileId === editingShortcutIconId) ?? null;
  const runningConversationSync = conversationSyncTasks.some((task) => task.status === "running");
  const fullConversationSyncTask = conversationSyncTasks
    .filter((task) => task.mode === "full" && task.record_kind == null)
    .sort((left, right) => right.started_at.localeCompare(left.started_at))[0] ?? null;
  const fullConversationSyncStatus = fullConversationSyncTask?.status === "running"
    ? t("settings.conversation.fullSyncRunning")
    : fullConversationSyncTask?.status === "completed"
      ? t("settings.conversation.fullSyncCompleted")
      : fullConversationSyncTask?.status === "failed"
        ? fullConversationSyncTask.error || t("settings.conversation.fullSyncFailed")
        : t("settings.conversation.fullSyncIdle");
  const fullSyncRunning = fullConversationSyncTask?.status === "running";
  const fullSyncAnimating = fullSyncStarting || fullSyncRunning;
  const fullSyncButtonPercent = fullSyncRunning
    && fullConversationSyncTask.progress
    && fullConversationSyncTask.progress.total_source_count > 0
    ? Math.round(
      (Math.min(
        fullConversationSyncTask.progress.completed_source_count,
        fullConversationSyncTask.progress.total_source_count,
      )
        / fullConversationSyncTask.progress.total_source_count) * 100,
    )
    : null;
  const fullSyncButtonLabel = fullSyncButtonPercent == null
    ? fullSyncAnimating
      ? t("settings.conversation.fullSyncButtonRunning")
      : t("settings.conversation.fullSyncAction")
    : t("settings.conversation.fullSyncButtonRunningWithProgress", {
      percent: fullSyncButtonPercent,
    });

  return (
    <div className="fixed inset-x-0 bottom-0 top-[var(--app-window-titlebar-height)] z-50 bg-background text-on-surface">
      <section
        aria-labelledby="global-settings-title"
        aria-modal="true"
        className="grid h-[calc(100vh-var(--app-window-titlebar-height))] w-screen grid-cols-[288px_minmax(0,1fr)] overflow-hidden bg-theme-card-header"
        role="dialog"
      >
        <aside className="flex min-h-0 flex-col border-r border-theme-card-border bg-theme-nav/95">
          <div className="border-b border-theme-card-border px-6 py-6">
            <div className="flex items-center gap-3">
              <span className="grid size-10 place-items-center rounded-xl border border-theme-nav-active-border bg-theme-nav-active text-theme-nav-active-fg">
                <Settings size={20} />
              </span>
              <div className="min-w-0">
                <p className="text-label-caps uppercase text-outline">AssetIWeave</p>
                <h2 className="truncate text-h2 text-on-surface" id="global-settings-title">
                  {t("settings.title")}
                </h2>
              </div>
            </div>
          </div>

          <nav className="flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain px-4 py-5" aria-label={t("settings.navAria")}>
            {settingGroups.map((group) => {
              const collapsed = collapsedGroups.has(group.id);
              return (
                <div className="mb-3 last:mb-0" key={group.id}>
                  <button
                    aria-expanded={!collapsed}
                    aria-label={`${t(collapsed ? "settings.group.toggle.expand" : "settings.group.toggle.collapse")} ${group.label}`}
                    className="-mx-1 flex w-[calc(100%+0.5rem)] items-center gap-1 rounded-md px-3 pb-1 text-label-caps uppercase text-outline transition-colors hover:text-on-surface-variant"
                    onClick={() => toggleGroupCollapsed(group.id)}
                    type="button"
                  >
                    {collapsed ? <ChevronRight size={17} /> : <ChevronDown size={17} />}
                    <span>{group.label}</span>
                  </button>
                  {!collapsed && (
                    <div className="flex flex-col gap-1">
                      {group.panels.map((panel) => {
                        const Icon = panel.icon;

                        return (
                          <Button
                            variant="ghost"
                            className={clsx(
                              "h-10 justify-start px-3",
                              activePanel === panel.id
                                ? "bg-theme-nav-active text-theme-nav-active-fg"
                                : "text-on-surface-variant hover:bg-theme-nav-hover hover:text-theme-nav-active-fg",
                            )}
                            key={panel.id}
                            onClick={() => setActivePanel(panel.id)}
                            type="button"
                          >
                            <Icon size={17} />
                            <span>{panel.label}</span>
                          </Button>
                        );
                      })}
                    </div>
                  )}
                </div>
              );
            })}
          </nav>

          <div className="border-t border-theme-card-border p-4">
            <Button
              className="w-full"
              onClick={resetSettings}
              type="button"
              variant="outline"
            >
              <RotateCcw size={16} />
              <span>{t("settings.reset")}</span>
            </Button>
          </div>
        </aside>

        <div className="flex min-h-0 min-w-0 flex-col bg-surface">
          <header className="flex h-20 shrink-0 items-center justify-between border-b border-theme-card-border bg-theme-toolbar/72 px-8">
            <div className="min-w-0">
              <p className="text-label-caps uppercase text-outline">{activeScope}</p>
              <h3 className="truncate text-h2 text-on-surface">{activePanelConfig.label}</h3>
            </div>
            <Button
              className="text-on-surface-variant hover:text-on-surface"
              onClick={onClose}
              aria-label={t("settings.close")}
              size="icon"
              type="button"
              variant="ghost"
            >
              <X size={18} />
            </Button>
          </header>

          <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-8 py-6">
            {activePanel === "general.appearance" && (
              <SettingsGroup>
                <SettingRow icon={<Languages size={18} />} label={t("settings.language")}>
                  <SegmentedControl
                    label={t("settings.language")}
                    onChange={(value) => setLocale(value as Locale)}
                    options={[
                      { label: t("language.zh"), value: "zh" },
                      { label: t("language.en"), value: "en" },
                    ]}
                    value={locale}
                  />
                </SettingRow>
                <SettingRow icon={<Palette size={18} />} label={t("settings.theme")}>
                  <ThemePaletteControl onChange={(value) => updateSetting("theme", value)} t={t} value={settings.theme} />
                </SettingRow>
                <SettingRow icon={<Gauge size={18} />} label={t("settings.density")}>
                  <SegmentedControl
                    label={t("settings.density")}
                    onChange={(value) => updateSetting("density", value as InterfaceDensity)}
                    options={[
                      { label: t("settings.density.comfortable"), value: "comfortable" },
                      { label: t("settings.density.compact"), value: "compact" },
                    ]}
                    value={settings.density}
                  />
                </SettingRow>
                <SettingRow icon={<Columns3 size={18} />} label={t("settings.columnMinWidth")}>
                  <RangeSettingControl
                    label={t("settings.columnMinWidth")}
                    max={COLUMN_MIN_WIDTH_MAX}
                    min={COLUMN_MIN_WIDTH_MIN}
                    onChange={(value) => updateSetting("columnMinWidth", value)}
                    step={COLUMN_MIN_WIDTH_STEP}
                    value={settings.columnMinWidth}
                  />
                </SettingRow>
                <SettingRow icon={<Bell size={18} />} label={t("settings.showStartupNotification")}>
                  <SwitchControl
                    checked={settings.showStartupNotification}
                    label={t("settings.showStartupNotification")}
                    onChange={(checked) => updateSetting("showStartupNotification", checked)}
                  />
                </SettingRow>
              </SettingsGroup>
            )}

            {(activePanel === "agents.market" || activePanel === "agents.settings") && (
              <AgentSettingsPanel
                appShortcuts={appShortcuts}
                focusAgentId={agentFocusId}
                onModelChange={(agentId, modelId) => {
                  updateSetting("agentModels", {
                    ...settings.agentModels,
                    [agentId]: modelId,
                  });
                  if (agentId === "opencode") {
                    updateSetting("aiRuntime", {
                      ...settings.aiRuntime,
                      model: modelId,
                    });
                  }
                }}
                selectedModels={settings.agentModels}
                view={activePanel === "agents.settings" ? "settings" : "market"}
              />
            )}

            {activePanel === "general.memory" && (
              <SettingsGroup>
                <SettingRow icon={<Bot size={18} />} label={t("settings.agentCapabilities.label")}>
                  <AgentCapabilitySetting
                    agentId={settings.agentCapabilityAssignments.memory}
                    appShortcuts={appShortcuts}
                    description={t("settings.agentCapabilities.memoryDescription")}
                    model={settings.agentModels[settings.agentCapabilityAssignments.memory]}
                    onOpen={() => openAgentCapabilityDialog("memory")}
                  />
                </SettingRow>
                <SettingRow icon={<Cpu size={18} />} label={t("settings.ai.autoDream")}>
                  <div className="flex w-[min(38rem,52vw)] items-center justify-between gap-4">
                    <p className="text-body-sm text-on-surface-variant">{t("settings.ai.autoDreamHint")}</p>
                    <SwitchControl
                      checked={settings.memory.autoDreamEnabled}
                      label={t("settings.ai.autoDream")}
                      onChange={(checked) =>
                        updateSetting("memory", {
                          ...settings.memory,
                          autoDreamEnabled: checked,
                        })
                      }
                    />
                  </div>
                </SettingRow>
                <SettingRow icon={<Gauge size={18} />} label={t("settings.ai.autoDreamMinHours")}>
                  <RangeSettingControl
                    label={t("settings.ai.autoDreamMinHours")}
                    max={AUTO_DREAM_MIN_HOURS_MAX}
                    min={AUTO_DREAM_MIN_HOURS_MIN}
                    onChange={(value) =>
                      updateSetting("memory", {
                        ...settings.memory,
                        minHours: value,
                      })
                    }
                    step={1}
                    unit={t("settings.unit.hours")}
                    value={settings.memory.minHours}
                  />
                </SettingRow>
                <SettingRow icon={<ListTree size={18} />} label={t("settings.ai.autoDreamMinSessions")}>
                  <RangeSettingControl
                    label={t("settings.ai.autoDreamMinSessions")}
                    max={AUTO_DREAM_MIN_SESSIONS_MAX}
                    min={AUTO_DREAM_MIN_SESSIONS_MIN}
                    onChange={(value) =>
                      updateSetting("memory", {
                        ...settings.memory,
                        minSessions: value,
                      })
                    }
                    step={1}
                    unit={t("settings.unit.sessions")}
                    value={settings.memory.minSessions}
                  />
                </SettingRow>
              </SettingsGroup>
            )}

            {activePanel === "general.promptOptimization" && (
              <SettingsGroup>
                <SettingRow icon={<Bot size={18} />} label={t("settings.agentCapabilities.label")}>
                  <AgentCapabilitySetting
                    agentId={settings.agentCapabilityAssignments.promptOptimization}
                    appShortcuts={appShortcuts}
                    description={t("settings.agentCapabilities.promptOptimizationDescription")}
                    model={settings.agentModels[settings.agentCapabilityAssignments.promptOptimization]}
                    onOpen={() => openAgentCapabilityDialog("promptOptimization")}
                  />
                </SettingRow>
                <SettingRow icon={<Sparkles size={18} />} label={t("settings.promptOptimization.descriptionLabel")}>
                  <p className="w-[min(38rem,52vw)] text-body-sm leading-6 text-on-surface-variant">
                    {t("settings.promptOptimization.description")}
                  </p>
                </SettingRow>
                <SettingRow icon={<Code2 size={18} />} label={t("settings.promptOptimization.systemPrompt")}>
                  <textarea
                    aria-label={t("settings.promptOptimization.systemPrompt")}
                    className="min-h-44 w-[min(38rem,52vw)] rounded-xl border border-theme-control-border bg-theme-control px-3 py-2 font-mono text-code-md text-on-surface outline-none transition-colors placeholder:text-outline focus:border-primary-strong/60"
                    maxLength={PROMPT_OPTIMIZATION_PROMPT_TEMPLATE_MAX_LENGTH}
                    onBlur={(event) =>
                      updatePromptOptimization({
                        promptTemplate: event.currentTarget.value.trim(),
                      })
                    }
                    onChange={(event) =>
                      updatePromptOptimization({
                        promptTemplate: event.target.value.slice(0, PROMPT_OPTIMIZATION_PROMPT_TEMPLATE_MAX_LENGTH),
                      })
                    }
                    placeholder={t("settings.promptOptimization.systemPromptPlaceholder")}
                    spellCheck={false}
                    value={settings.promptOptimization.promptTemplate}
                  />
                </SettingRow>
              </SettingsGroup>
            )}

            {activePanel === "general.typography" && (
              <SettingsGroup>
                <SettingRow icon={<Type size={18} />} label={t("settings.font.interface")}>
                  <FontFamilyControl
                    fallback="sans"
                    label={t("settings.font.interface")}
                    onChange={(value) =>
                      updateSetting("typography", {
                        ...settings.typography,
                        interfaceFontFamily: value,
                      })
                    }
                    t={t}
                    value={settings.typography.interfaceFontFamily}
                  />
                </SettingRow>
                <SettingRow icon={<Type size={18} />} label={t("settings.font.content")}>
                  <FontFamilyControl
                    fallback="sans"
                    label={t("settings.font.content")}
                    onChange={(value) =>
                      updateSetting("typography", {
                        ...settings.typography,
                        contentFontFamily: value,
                      })
                    }
                    t={t}
                    value={settings.typography.contentFontFamily}
                  />
                </SettingRow>
                <SettingRow icon={<Code2 size={18} />} label={t("settings.font.code")}>
                  <FontFamilyControl
                    fallback="mono"
                    label={t("settings.font.code")}
                    onChange={(value) =>
                      updateSetting("typography", {
                        ...settings.typography,
                        codeFontFamily: value,
                      })
                    }
                    t={t}
                    value={settings.typography.codeFontFamily}
                  />
                </SettingRow>
                <SettingRow icon={<Gauge size={18} />} label={t("settings.font.baseSize")}>
                  <RangeSettingControl
                    label={t("settings.font.baseSize")}
                    max={FONT_SIZE_MAX}
                    min={FONT_SIZE_MIN}
                    onChange={(value) =>
                      updateSetting("typography", {
                        ...settings.typography,
                        baseFontSize: value,
                      })
                    }
                    step={FONT_SIZE_STEP}
                    unit="px"
                    value={settings.typography.baseFontSize}
                  />
                </SettingRow>
                <SettingRow icon={<Type size={18} />} label={t("settings.font.contentSize")}>
                  <RangeSettingControl
                    label={t("settings.font.contentSize")}
                    max={FONT_SIZE_MAX}
                    min={FONT_SIZE_MIN}
                    onChange={(value) =>
                      updateSetting("typography", {
                        ...settings.typography,
                        contentFontSize: value,
                      })
                    }
                    step={FONT_SIZE_STEP}
                    unit="px"
                    value={settings.typography.contentFontSize}
                  />
                </SettingRow>
                <SettingRow icon={<Code2 size={18} />} label={t("settings.font.codeSize")}>
                  <RangeSettingControl
                    label={t("settings.font.codeSize")}
                    max={FONT_SIZE_MAX}
                    min={FONT_SIZE_MIN}
                    onChange={(value) =>
                      updateSetting("typography", {
                        ...settings.typography,
                        codeFontSize: value,
                      })
                    }
                    step={FONT_SIZE_STEP}
                    unit="px"
                    value={settings.typography.codeFontSize}
                  />
                </SettingRow>
              </SettingsGroup>
            )}

            {activePanel === "general.storage" && (
              <SettingsGroup>
                <SettingsPathRow
                  icon={<FileJson size={18} />}
                  label={t("settings.storage.configFile")}
                  onOpen={() => void revealPath(storageInfo.configPath)}
                  openLabel={t("settings.storage.reveal")}
                  value={storageInfo.configPath}
                />
                <SettingsPathRow
                  icon={<FolderOpen size={18} />}
                  label={t("settings.storage.configDir")}
                  onOpen={() => void revealPath(storageInfo.configDir)}
                  openLabel={t("settings.storage.open")}
                  value={storageInfo.configDir}
                />
                <SettingsPathRow
                  icon={<Database size={18} />}
                  label={t("settings.storage.defaultDataBackupDir")}
                  onOpen={() => void revealPath(storageInfo.defaultDataBackupDir)}
                  openLabel={t("settings.storage.open")}
                  value={storageInfo.defaultDataBackupDir}
                />
                <CliToolsInstallRow
                  error={cliToolsError}
                  installing={cliToolsInstalling}
                  onInstall={() => void handleInstallCliTools()}
                  status={cliToolsStatus}
                  t={t}
                />
                <DataBackupDirectoryRow
                  customDirectory={settings.dataBackup.customDirectory}
                  onClear={clearDataBackupDirectory}
                  onOpen={() => void revealPath(settings.dataBackup.customDirectory)}
                  onPick={() => void chooseDataBackupDirectory()}
                  t={t}
                />
                <SettingsPathRow
                  icon={<Puzzle size={18} />}
                  label={t("settings.storage.conversationAdapterDir")}
                  onOpen={() => void revealPath(storageInfo.conversationAdapterDir)}
                  openLabel={t("settings.storage.open")}
                  value={storageInfo.conversationAdapterDir}
                />


                <SkillBackupDirectorySetting
                  onOpen={() => setBackupDialogOpen(true)}
                  rootPath={backupSettings?.display_root_path ?? backupSettings?.expanded_root_path}
                />
                {backupError && (
                  <div className="rounded-lg border border-status-remove/30 bg-status-remove/10 px-3 py-2 text-body-sm text-status-remove">
                    {backupError}
                  </div>
                )}
              </SettingsGroup>
            )}

            {activePanel === "workspace.menu" && (
              <div className="flex flex-col gap-5">
                <MenuSection icon={<PanelLeft size={18} />} title={t("settings.menu.headerTabs")}>
                  <SortableMenuList itemIds={navigationModel.headerTabs.map((item) => item.id)} onReorder={reorderHeaderTabs}>
                    {navigationModel.headerTabs.map((item) => (
                      <SortableMenuEditRow
                        enabled={item.enabled}
                        id={item.id}
                        key={item.id}
                        label={headerTabLabel(item, t, locale)}
                        onEnabledChange={(enabled) => updateHeaderTab(item.id, { enabled })}
                        onLabelChange={(label) => updateHeaderTabLabel(item.id, label)}
                        t={t}
                      />
                    ))}
                  </SortableMenuList>
                </MenuSection>

                <MenuSection icon={<PanelTop size={18} />} title={t("settings.menu.sideRail")}>
                  <SortableMenuList itemIds={configurableRailItems.map((item) => item.id)} onReorder={(orderedIds) => reorderRailItems("secondary", orderedIds)}>
                    {configurableRailItems.map((item) => (
                      <SortableMenuEditRow
                        enabled={item.enabled}
                        id={item.id}
                        key={item.id}
                        label={railLabel(item, t, locale)}
                        onEnabledChange={(enabled) => updateRailItem(item.id, { enabled })}
                        onLabelChange={(label) => updateRailItemLabel(item.id, label)}
                        t={t}
                      />
                    ))}
                  </SortableMenuList>
                </MenuSection>

                <MenuSection icon={<ListTree size={18} />} title={t("settings.menu.subNavigation")}>
                  {navigationModel.headerTabs.map((tab) => {
                    const items = navigationModel.subNavItems[tab.id] ?? [];
                    if (items.length === 0) {
                      return null;
                    }

                    return (
                      <div className="border-b border-theme-card-border last:border-b-0" key={tab.id}>
                        <div className="border-b border-theme-card-border/70 bg-theme-card-header/65 px-4 py-2 text-label-caps uppercase text-outline">
                          {headerTabLabel(tab, t, locale)}
                        </div>
                        <SortableMenuList itemIds={items.map((item) => item.id)} onReorder={(orderedIds) => reorderSubNavItems(tab.id, orderedIds)}>
                          {items.map((item) => (
                            <SortableMenuEditRow
                              enabled={item.enabled}
                              id={item.id}
                              key={item.id}
                              label={subNavLabel(item, t, locale)}
                              onEnabledChange={(enabled) => updateSubNavItem(tab.id, item.id, { enabled })}
                              onLabelChange={(label) => updateSubNavItemLabel(tab.id, item.id, label)}
                              t={t}
                            />
                          ))}
                        </SortableMenuList>
                      </div>
                    );
                  })}
                </MenuSection>
              </div>
            )}

            {activePanel === "workspace.shortcuts" && (
              <MenuSection icon={<MousePointerClick size={18} />} title={t("settings.shortcuts.title")}>
                <SortableMenuList itemIds={appShortcuts.map((shortcut) => shortcut.profileId)} onReorder={reorderAppShortcuts}>
                  {appShortcuts.map((shortcut) => (
                    <SortableShortcutEditRow
                      id={shortcut.profileId}
                      key={shortcut.profileId}
                      onAccentColorChange={(accentColor) => updateAppShortcut(shortcut.profileId, { accentColor })}
                      onDisplayIconChange={(displayIcon) => updateAppShortcut(shortcut.profileId, { displayIcon })}
                      onEnabledChange={(enabled) => updateAppShortcut(shortcut.profileId, { enabled })}
                      onIconSvgEdit={() => openShortcutIconEditor(shortcut)}
                      shortcut={shortcut}
                      t={t}
                    />
                  ))}
                </SortableMenuList>
              </MenuSection>
            )}



            {activePanel === "conversations.sessions" && (
              <SettingsGroup>
                <SettingRow icon={<RefreshCw size={18} />} label={t("settings.conversation.autoFullSyncOnStartup")}>
                  <div className="flex w-[min(38rem,52vw)] items-center justify-between gap-4">
                    <p className="text-body-sm text-on-surface-variant">
                      {t("settings.conversation.autoFullSyncOnStartupHint")}
                    </p>
                    <SwitchControl
                      checked={settings.conversations.autoFullSyncOnStartup}
                      label={t("settings.conversation.autoFullSyncOnStartup")}
                      onChange={(checked) =>
                        updateSetting("conversations", {
                          ...settings.conversations,
                          autoFullSyncOnStartup: checked,
                        })
                      }
                    />
                  </div>
                </SettingRow>
                <SettingRow icon={<RefreshCw size={18} />} label={t("settings.conversation.fullSyncTitle")}>
                  <div className="flex w-[min(38rem,52vw)] flex-col gap-2 py-1">
                    <p className="text-body-sm leading-6 text-on-surface-variant">
                      {t("settings.conversation.fullSyncDescription")}
                    </p>
                    <div className="flex items-center justify-between gap-3">
                      <span aria-live="polite" className="min-w-0 truncate text-body-sm text-outline">
                        {fullConversationSyncStatus}
                      </span>
                      <Button
                        className={fullSyncAnimating
                          ? "border-status-update/55 bg-status-update/10 text-status-update disabled:opacity-100"
                          : undefined}
                        disabled={runningConversationSync || fullSyncStarting}
                        onClick={() => {
                          setFullSyncError("");
                          setFullSyncConfirmOpen(true);
                        }}
                        type="button"
                        variant="outline"
                      >
                        <RefreshCw
                          className={fullSyncAnimating ? "motion-safe:animate-spin" : undefined}
                          size={16}
                        />
                        {fullSyncButtonLabel}
                      </Button>
                    </div>
                    {fullConversationSyncTask?.progress ? (
                      <ConversationFullSyncProgress progress={fullConversationSyncTask.progress} t={t} />
                    ) : null}
                    {fullSyncError ? (
                      <p className="text-body-sm text-status-remove" role="alert">{fullSyncError}</p>
                    ) : null}
                  </div>
                </SettingRow>
                <SettingRow icon={<Type size={18} />} label={t("settings.conversation.sessionBrowserFont")}>
                  <FontFamilyControl
                    fallback="sans"
                    label={t("settings.conversation.sessionBrowserFont")}
                    onChange={(value) =>
                      updateSetting("conversations", {
                        ...settings.conversations,
                        sessionBrowserFontFamily: value,
                      })
                    }
                    t={t}
                    value={settings.conversations.sessionBrowserFontFamily}
                  />
                </SettingRow>
                <SettingRow icon={<Gauge size={18} />} label={t("settings.conversation.sessionBrowserSize")}>
                  <RangeSettingControl
                    label={t("settings.conversation.sessionBrowserSize")}
                    max={FONT_SIZE_MAX}
                    min={FONT_SIZE_MIN}
                    onChange={(value) =>
                      updateSetting("conversations", {
                        ...settings.conversations,
                        sessionBrowserFontSize: value,
                      })
                    }
                    step={FONT_SIZE_STEP}
                    unit="px"
                    value={settings.conversations.sessionBrowserFontSize}
                  />
                </SettingRow>
                <SettingRow icon={<Palette size={18} />} label={t("settings.conversation.contentCardColors")}>
                  <ConversationContentCardColorControl
                    onChange={(colors) =>
                      updateSetting("conversations", {
                        ...settings.conversations,
                        contentCardColors: colors,
                      })
                    }
                    t={t}
                    value={settings.conversations.contentCardColors}
                  />
                </SettingRow>
                <SettingRow icon={<Type size={18} />} label={t("settings.conversation.contentFont")}>
                  <FontFamilyControl
                    fallback="sans"
                    label={t("settings.conversation.contentFont")}
                    onChange={(value) =>
                      updateSetting("conversations", {
                        ...settings.conversations,
                        contentFontFamily: value,
                      })
                    }
                    t={t}
                    value={settings.conversations.contentFontFamily}
                  />
                </SettingRow>
                <SettingRow icon={<Gauge size={18} />} label={t("settings.conversation.contentSize")}>
                  <RangeSettingControl
                    label={t("settings.conversation.contentSize")}
                    max={FONT_SIZE_MAX}
                    min={FONT_SIZE_MIN}
                    onChange={(value) =>
                      updateSetting("conversations", {
                        ...settings.conversations,
                        contentFontSize: value,
                      })
                    }
                    step={FONT_SIZE_STEP}
                    unit="px"
                    value={settings.conversations.contentFontSize}
                  />
                </SettingRow>
                <SettingRow icon={<Code2 size={18} />} label={t("settings.conversation.codeSize")}>
                  <RangeSettingControl
                    label={t("settings.conversation.codeSize")}
                    max={FONT_SIZE_MAX}
                    min={FONT_SIZE_MIN}
                    onChange={(value) =>
                      updateSetting("conversations", {
                        ...settings.conversations,
                        codeFontSize: value,
                      })
                    }
                    step={FONT_SIZE_STEP}
                    unit="px"
                    value={settings.conversations.codeFontSize}
                  />
                </SettingRow>
                <SettingRow icon={<Gauge size={18} />} label={t("settings.conversation.resultPreviewLines")}>
                  <RangeSettingControl
                    label={t("settings.conversation.resultPreviewLines")}
                    max={RESULT_PREVIEW_LINE_LIMIT_MAX}
                    min={RESULT_PREVIEW_LINE_LIMIT_MIN}
                    onChange={(value) =>
                      updateSetting("conversations", {
                        ...settings.conversations,
                        resultPreviewLineLimit: value,
                      })
                    }
                    step={RESULT_PREVIEW_LINE_LIMIT_STEP}
                    unit={t("settings.unit.lines")}
                    value={settings.conversations.resultPreviewLineLimit}
                  />
                </SettingRow>
                <SettingRow icon={<Columns3 size={18} />} label={t("settings.conversation.compactToolbar")}>
                  <SwitchControl
                    checked={settings.conversations.sessionToolbarCompact}
                    label={t("settings.conversation.compactToolbar")}
                    onChange={(checked) =>
                      updateSetting("conversations", {
                        ...settings.conversations,
                        sessionToolbarCompact: checked,
                      })
                    }
                  />
                </SettingRow>
              </SettingsGroup>
            )}

            {activePanel === "conversations.translation" && (
              <SettingsGroup>
                <SettingRow icon={<Bot size={18} />} label={t("settings.agentCapabilities.label")}>
                  <AgentCapabilitySetting
                    agentId={settings.agentCapabilityAssignments.cardTranslation}
                    appShortcuts={appShortcuts}
                    description={t("settings.agentCapabilities.cardTranslationDescription")}
                    model={settings.agentModels[settings.agentCapabilityAssignments.cardTranslation]}
                    onOpen={() => openAgentCapabilityDialog("cardTranslation")}
                  />
                </SettingRow>
                <SettingRow icon={<Languages size={18} />} label={t("settings.conversation.translationProvider")}>
                  <SegmentedControl
                    label={t("settings.conversation.translationProvider")}
                    onChange={(value) =>
                      updateConversationTranslation({
                        provider: value as ConversationTranslationProvider,
                      })
                    }
                    options={[
                      { label: t("settings.conversation.translationProvider.cli"), value: "cli" },
                      { label: t("settings.conversation.translationProvider.google"), value: "google" },
                      { label: t("settings.conversation.translationProvider.apple"), value: "apple" },
                    ]}
                    value={settings.conversationTranslation.provider}
                  />
                </SettingRow>
                {settings.conversationTranslation.provider !== "cli" ? (
                  <SettingRow icon={<Activity size={18} />} label={t("settings.conversation.translationConnection")}>
                    <div className="w-[min(38rem,52vw)] rounded-lg border border-theme-control-border bg-theme-control px-3 py-2 text-body-sm text-on-surface-variant">
                      {t("settings.conversation.translationProviderReserved")}
                    </div>
                  </SettingRow>
                ) : null}
                <SettingRow icon={<Languages size={18} />} label={t("settings.conversation.translationTarget")}>
                  <Input
                    aria-label={t("settings.conversation.translationTarget")}
                    className="h-9 w-[min(28rem,44vw)] min-w-56"
                    maxLength={TRANSLATION_TARGET_LANGUAGE_MAX_LENGTH}
                    onBlur={(event) =>
                      updateConversationTranslation({
                        targetLanguage: normalizeConversationTranslationTargetLanguage(event.currentTarget.value),
                      })
                    }
                    onChange={(event) =>
                      updateConversationTranslation({
                        targetLanguage: event.target.value.slice(0, TRANSLATION_TARGET_LANGUAGE_MAX_LENGTH),
                      })
                    }
                    placeholder={t("settings.conversation.translationTargetPlaceholder")}
                    value={settings.conversationTranslation.targetLanguage}
                  />
                </SettingRow>
                <SettingRow icon={<Code2 size={18} />} label={t("settings.conversation.translationPrompt")}>
                  <textarea
                    aria-label={t("settings.conversation.translationPrompt")}
                    className="min-h-44 w-[min(38rem,52vw)] rounded-xl border border-theme-control-border bg-theme-control px-3 py-2 font-mono text-code-md text-on-surface outline-none transition-colors placeholder:text-outline focus:border-primary-strong/60"
                    maxLength={TRANSLATION_PROMPT_TEMPLATE_MAX_LENGTH}
                    onBlur={(event) =>
                      updateConversationTranslation({
                        promptTemplate: event.currentTarget.value.trim(),
                      })
                    }
                    onChange={(event) =>
                      updateConversationTranslation({
                        promptTemplate: event.target.value.slice(0, TRANSLATION_PROMPT_TEMPLATE_MAX_LENGTH),
                      })
                    }
                    placeholder={t("settings.conversation.translationPromptPlaceholder")}
                    spellCheck={false}
                    value={settings.conversationTranslation.promptTemplate}
                  />
                </SettingRow>
              </SettingsGroup>
            )}

            {activePanel === "conversations.adapters" && (
              <SettingsGroup>
                <SettingsPathRow
                  icon={<Puzzle size={18} />}
                  label={t("settings.storage.conversationAdapterDir")}
                  onOpen={() => void revealPath(storageInfo.conversationAdapterDir)}
                  openLabel={t("settings.storage.open")}
                  value={storageInfo.conversationAdapterDir}
                />
                <SettingRow icon={<Code2 size={18} />} label={t("settings.conversation.adapterWorkflow")}>
                  <div className="w-[min(36rem,52vw)] rounded-lg border border-theme-control-border bg-theme-control px-3 py-2 font-mono text-code-md text-on-surface-variant">
                    assetiweave-cli conversation adapter scaffold --directory {storageInfo.conversationAdapterDir} --runtime node
                  </div>
                </SettingRow>
                <ConversationRuntimeOverrideRow
                  onChange={updateConversationRuntimeOverride}
                  t={t}
                  value={settings.conversationRuntimeOverrides}
                />
                <AdapterRuntimeStatusRow
                  error={adapterRuntimeError}
                  loading={adapterRuntimeLoading}
                  onRefresh={() => void refreshAdapterRuntimeStatuses()}
                  statuses={adapterRuntimeStatuses}
                  t={t}
                />
              </SettingsGroup>
            )}
          </div>
        </div>
      </section>
      {editingShortcutIcon && (
        <ShortcutIconSvgDialog
          draft={iconSvgDraft}
          error={iconSvgError}
          onCancel={closeShortcutIconEditor}
          onChange={(value) => {
            setIconSvgDraft(value);
            setIconSvgError("");
          }}
          onClear={clearShortcutIconSvg}
          onSave={saveShortcutIconSvg}
          shortcut={editingShortcutIcon}
          t={t}
        />
      )}
      <SkillBackupLibraryDialog
        onClose={() => setBackupDialogOpen(false)}
        onNotifyError={setBackupError}
        onSaved={async (nextSettings) => {
          setBackupError("");
          setBackupSettings(nextSettings);
          await onSkillBackupLibraryChange?.();
        }}
        open={backupDialogOpen}
      />
      <ConfirmDialog
        busy={fullSyncStarting}
        confirmLabel={t("settings.conversation.fullSyncConfirmAction")}
        message={t("settings.conversation.fullSyncConfirmMessage")}
        onClose={() => {
          if (!fullSyncStarting) {
            setFullSyncConfirmOpen(false);
          }
        }}
        onConfirm={() => void startFullConversationSync()}
        open={fullSyncConfirmOpen}
        title={t("settings.conversation.fullSyncConfirmTitle")}
      />
      {agentCapabilityDialog ? (
        <AgentCapabilityDialog
          agentId={agentCapabilityDialog.agentId}
          appShortcuts={appShortcuts}
          model={agentCapabilityDialog.model}
          onAgentChange={selectAgentCapability}
          onClose={() => setAgentCapabilityDialog(null)}
          onOpenAgentSettings={openAgentSettings}
        />
      ) : null}
    </div>
  );
}

function SettingsGroup({ children }: { children: ReactNode }) {
  return (
    <Card className="overflow-hidden">
      <CardContent className="p-0">{children}</CardContent>
    </Card>
  );
}

function MenuSection({ children, icon, title }: { children: ReactNode; icon: ReactNode; title: string }) {
  return (
    <Card aria-label={title} className="overflow-hidden" role="region">
      <CardHeader className="flex h-12 flex-row items-center gap-3 bg-theme-card-header px-4 py-0">
        <span className="grid size-8 place-items-center rounded-lg border border-theme-control-border bg-theme-control text-primary">{icon}</span>
        <CardTitle className="text-body-md">{title}</CardTitle>
      </CardHeader>
      <CardContent className="p-0">{children}</CardContent>
    </Card>
  );
}

function SortableMenuList({
  children,
  itemIds,
  onReorder,
}: {
  children: ReactNode;
  itemIds: string[];
  onReorder: (orderedIds: string[]) => void;
}) {
  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: {
        distance: 6,
      },
    }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    if (!over || active.id === over.id) {
      return;
    }

    const oldIndex = itemIds.indexOf(String(active.id));
    const newIndex = itemIds.indexOf(String(over.id));
    if (oldIndex < 0 || newIndex < 0) {
      return;
    }

    onReorder(arrayMove(itemIds, oldIndex, newIndex));
  }

  return (
    <DndContext collisionDetection={closestCenter} onDragEnd={handleDragEnd} sensors={sensors}>
      <SortableContext items={itemIds} strategy={verticalListSortingStrategy}>
        <div>{children}</div>
      </SortableContext>
    </DndContext>
  );
}

function SortableMenuEditRow({
  enabled,
  id,
  label,
  onEnabledChange,
  onLabelChange,
  t,
}: {
  enabled: boolean;
  id: string;
  label: string;
  onEnabledChange: (enabled: boolean) => void;
  onLabelChange: (label: string) => void;
  t: Translator;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id });
  const [draftLabel, setDraftLabel] = useState(label);
  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  useEffect(() => {
    setDraftLabel(label);
  }, [label]);

  function commitLabel(value: string) {
    const nextLabel = value.trim();
    if (!nextLabel) {
      setDraftLabel(label);
      return;
    }

    if (nextLabel !== label) {
      onLabelChange(nextLabel);
    }
  }

  return (
    <div
      className={clsx(
        "grid min-h-14 grid-cols-[32px_minmax(220px,1fr)_auto] items-center gap-4 border-b border-theme-card-border px-4 py-2.5 last:border-b-0",
        isDragging && "relative z-10 border-theme-nav-active-border bg-theme-control-hover shadow-[0_18px_44px_rgb(var(--theme-panel-shadow)/0.28)]",
      )}
      ref={setNodeRef}
      style={style}
    >
      <Button
        aria-label={t("settings.menu.dragHandle")}
        className="cursor-grab text-outline hover:text-primary active:cursor-grabbing"
        size="icon-sm"
        title={t("settings.menu.dragHandle")}
        type="button"
        variant="ghost"
        {...attributes}
        {...listeners}
      >
        <GripVertical size={16} />
      </Button>
      <label className="flex min-w-0 items-center gap-3">
        <span className={clsx("size-2 shrink-0 rounded-full", enabled ? "bg-status-create" : "bg-outline-variant")} aria-hidden="true" />
        <Input
          aria-label={t("settings.menu.name")}
          className="min-w-0 flex-1 font-semibold"
          onBlur={(event) => commitLabel(event.currentTarget.value)}
          onChange={(event) => setDraftLabel(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              commitLabel(event.currentTarget.value);
              event.currentTarget.blur();
            }
            if (event.key === "Escape") {
              setDraftLabel(label);
              event.currentTarget.blur();
            }
          }}
          placeholder={t("settings.menu.name")}
          value={draftLabel}
        />
      </label>

      <div className="flex items-center gap-2">
        <span className="w-12 text-right text-body-sm text-on-surface-variant">{enabled ? t("settings.menu.visible") : t("settings.menu.hidden")}</span>
        <SwitchControl checked={enabled} label={t("settings.menu.visible")} onChange={onEnabledChange} />
      </div>
    </div>
  );
}

function SortableShortcutEditRow({
  id,
  onAccentColorChange,
  onDisplayIconChange,
  onEnabledChange,
  onIconSvgEdit,
  shortcut,
  t,
}: {
  id: string;
  onAccentColorChange: (accentColor: string) => void;
  onDisplayIconChange: (displayIcon: string) => void;
  onEnabledChange: (enabled: boolean) => void;
  onIconSvgEdit: () => void;
  shortcut: AppShortcut;
  t: Translator;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id });
  const [draftColor, setDraftColor] = useState(shortcut.accentColor);
  const appIconKey = resolveAppIconKey(shortcut);
  const canUseAppIcon = Boolean(appIconKey);
  const usesAppIcon = shortcutUsesAppIcon(shortcut);
  const usesCustomIcon = Boolean(shortcut.iconSvg || shortcutCustomIconText(shortcut));
  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  useEffect(() => {
    setDraftColor(shortcut.accentColor);
  }, [shortcut.accentColor]);

  function commitColor(value: string) {
    if (!isHexColor(value)) {
      setDraftColor(shortcut.accentColor);
      return;
    }

    if (value !== shortcut.accentColor) {
      onAccentColorChange(value);
    }
  }

  return (
    <div
      className={clsx(
        "grid min-h-16 grid-cols-[32px_minmax(200px,1fr)_240px_170px_auto] items-center gap-4 border-b border-theme-card-border px-4 py-3 last:border-b-0",
        isDragging && "relative z-10 border-theme-nav-active-border bg-theme-control-hover shadow-[0_18px_44px_rgb(var(--theme-panel-shadow)/0.28)]",
      )}
      ref={setNodeRef}
      style={style}
    >
      <Button
        aria-label={t("settings.menu.dragHandle")}
        className="cursor-grab text-outline hover:text-primary active:cursor-grabbing"
        size="icon-sm"
        title={t("settings.menu.dragHandle")}
        type="button"
        variant="ghost"
        {...attributes}
        {...listeners}
      >
        <GripVertical size={16} />
      </Button>
      <div className="flex min-w-0 items-center gap-3">
        <span
          className={clsx(APP_SHORTCUT_ICON_FRAME_CLASS, "size-9 shrink-0 text-[13px] font-bold")}
          style={appShortcutIconFrameStyle(shortcut.accentColor)}
          aria-hidden="true"
        >
          <AppShortcutIconForShortcut className="size-5" shortcut={shortcut} />
        </span>
        <div className="min-w-0">
          <p className="truncate text-body-md font-bold text-on-surface">{shortcut.profileName}</p>
          <p className="truncate font-mono text-code-md uppercase text-outline">{shortcut.appKind}</p>
        </div>
      </div>

      <div className="flex min-w-0 flex-col gap-1">
        <span className="text-label-caps uppercase text-outline">{t("settings.shortcuts.icon")}</span>
        <div className="flex h-9 min-w-0 items-center gap-2">
          <Button
            aria-pressed={usesAppIcon}
            className={clsx("h-9 shrink-0 px-3", usesAppIcon && "border-primary-strong/50 bg-theme-control-hover text-primary")}
            disabled={!canUseAppIcon}
            onClick={() => {
              if (appIconKey) {
                onDisplayIconChange(appIconToken(appIconKey));
              }
            }}
            title={t("settings.shortcuts.useAppIcon")}
            type="button"
            variant="outline"
          >
            <AppShortcutIconForShortcut
              className="size-4"
              shortcut={{
                ...shortcut,
                displayIcon: (appIconKey ? appIconToken(appIconKey) : null) || shortcut.displayIcon,
              }}
            />
            <span>{t("settings.shortcuts.appIcon")}</span>
          </Button>
          <Button
            aria-label={t("settings.shortcuts.editSvg")}
            aria-pressed={usesCustomIcon}
            className={clsx("h-9 shrink-0 px-3", usesCustomIcon && "border-primary-strong/50 bg-theme-control-hover text-primary")}
            onClick={onIconSvgEdit}
            title={t("settings.shortcuts.editSvg")}
            type="button"
            variant="outline"
          >
            <Code2 size={15} />
            <span>{t("settings.shortcuts.iconCode")}</span>
          </Button>
        </div>
      </div>

      <label className="flex min-w-0 flex-col gap-1">
        <span className="text-label-caps uppercase text-outline">{t("settings.shortcuts.color")}</span>
        <div className="flex h-9 items-center gap-2 rounded-lg border border-theme-control-border bg-theme-control px-2 transition-colors focus-within:border-primary-strong/60">
          <input
            aria-label={t("settings.shortcuts.color")}
            className="size-5 shrink-0 cursor-pointer rounded border-0 bg-transparent p-0"
            onChange={(event) => {
              setDraftColor(event.target.value);
              onAccentColorChange(event.target.value);
            }}
            type="color"
            value={shortcut.accentColor}
          />
          <Input
            aria-label={t("settings.shortcuts.colorValue")}
            className="h-auto min-w-0 flex-1 border-0 bg-transparent p-0 font-mono text-code-md focus:border-transparent"
            onBlur={(event) => commitColor(event.currentTarget.value)}
            onChange={(event) => setDraftColor(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                commitColor(event.currentTarget.value);
                event.currentTarget.blur();
              }
              if (event.key === "Escape") {
                setDraftColor(shortcut.accentColor);
                event.currentTarget.blur();
              }
            }}
            value={draftColor}
          />
        </div>
      </label>

      <div className="flex items-center gap-2">
        <span className="w-12 text-right text-body-sm text-on-surface-variant">
          {shortcut.enabled ? t("settings.menu.visible") : t("settings.menu.hidden")}
        </span>
        <SwitchControl checked={shortcut.enabled} label={t("settings.menu.visible")} onChange={onEnabledChange} />
      </div>
    </div>
  );
}

function ShortcutIconSvgDialog({
  draft,
  error,
  onCancel,
  onChange,
  onClear,
  onSave,
  shortcut,
  t,
}: {
  draft: string;
  error: string;
  onCancel: () => void;
  onChange: (value: string) => void;
  onClear: () => void;
  onSave: () => void;
  shortcut: AppShortcut;
  t: Translator;
}) {
  return (
    <DialogFrame
      closeLabel={t("settings.shortcuts.closeSvg")}
      description={shortcut.profileName}
      footer={
        <>
          <Button onClick={onClear} type="button" variant="ghost">
            {t("settings.shortcuts.clearSvg")}
          </Button>
          <div className="flex items-center gap-2">
            <Button onClick={onCancel} type="button" variant="outline">
              {t("settings.shortcuts.cancelSvg")}
            </Button>
            <Button onClick={onSave} type="button">
              {t("settings.shortcuts.saveSvg")}
            </Button>
          </div>
        </>
      }
      footerClassName="justify-between"
      icon={
        <span
          className={clsx(APP_SHORTCUT_ICON_FRAME_CLASS, "size-10 text-[13px] font-bold")}
          style={appShortcutIconFrameStyle(shortcut.accentColor)}
          aria-hidden="true"
        >
          <AppShortcutIconForShortcut className="size-5" shortcut={shortcut} />
        </span>
      }
      iconClassName="size-10 border-0 bg-transparent p-0"
      onClose={onCancel}
      overlayClassName="z-[60] px-6"
      size="xl"
      title={t("settings.shortcuts.svgEditorTitle")}
    >
      <div className="flex min-h-0 flex-col gap-3">
        <p className="text-body-sm text-on-surface-variant">{t("settings.shortcuts.svgEditorDescription")}</p>
        <div className="flex min-w-0 flex-col gap-2">
          <span className="text-label-caps uppercase text-outline">{t("settings.shortcuts.icon")}</span>
          <div className="flex flex-wrap items-center gap-1.5">
            {appShortcutIconCatalog.map(({ appKind, asset }) => (
              <Button
                className="h-7 gap-1.5 px-2 text-xs capitalize"
                key={appKind}
                onClick={() => {
                  onChange(asset.svg.trim());
                }}
                size="sm"
                type="button"
                variant="outline"
              >
                <AppShortcutIcon appKind={appKind} className="size-3.5" displayIcon={`app:${appKind}`} />
                <span>{appKind}</span>
              </Button>
            ))}
          </div>
        </div>
        <label className="flex min-h-0 flex-1 flex-col gap-2">
          <span className="text-label-caps uppercase text-outline">{t("settings.shortcuts.svgInput")}</span>
          <textarea
            aria-label={t("settings.shortcuts.svgInput")}
            className="min-h-72 resize-y rounded-lg border border-theme-control-border bg-theme-control px-3 py-3 font-mono text-code-md text-on-surface outline-none transition-colors placeholder:text-outline focus:border-primary-strong/60"
            onChange={(event) => onChange(event.target.value)}
            placeholder={t("settings.shortcuts.svgPlaceholder")}
            spellCheck={false}
            value={draft}
          />
        </label>
        {error && <p className="text-body-sm text-status-remove">{error}</p>}
      </div>
    </DialogFrame>
  );
}

function SettingRow({ children, icon, label }: { children: ReactNode; icon: ReactNode; label: string }) {
  return (
    <div className="flex min-h-16 items-center justify-between gap-5 border-b border-theme-card-border px-4 py-3 last:border-b-0">
      <div className="flex min-w-0 items-center gap-3">
        <span className="grid size-9 shrink-0 place-items-center rounded-xl border border-theme-control-border bg-theme-control text-primary">{icon}</span>
        <span className="min-w-0 truncate text-body-md font-semibold text-on-surface">{label}</span>
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function SettingsPathRow({
  icon,
  label,
  onOpen,
  openLabel,
  value,
}: {
  icon: ReactNode;
  label: string;
  onOpen: () => void;
  openLabel: string;
  value: string;
}) {
  return (
    <SettingRow icon={icon} label={label}>
      <div className="flex w-[min(38rem,52vw)] min-w-0 items-center gap-2">
        <code className="min-w-0 flex-1 truncate rounded-lg border border-theme-control-border bg-theme-control px-3 py-2 text-code-md text-on-surface-variant">
          {value}
        </code>
        <Button onClick={onOpen} type="button" variant="outline">
          <FolderOpen size={15} />
          <span>{openLabel}</span>
        </Button>
      </div>
    </SettingRow>
  );
}

function DataBackupDirectoryRow({
  customDirectory,
  onClear,
  onOpen,
  onPick,
  t,
}: {
  customDirectory: string;
  onClear: () => void;
  onOpen: () => void;
  onPick: () => void;
  t: Translator;
}) {
  const hasCustomDirectory = customDirectory.trim().length > 0;
  const displayPath = hasCustomDirectory
    ? abbreviateHomePath(customDirectory)
    : t("settings.storage.dataBackupDirEmpty");

  return (
    <SettingRow icon={<Database size={18} />} label={t("settings.storage.customDataBackupDir")}>
      <div className="flex w-[min(38rem,52vw)] min-w-0 items-center gap-2">
        <span
          className={clsx(
            "min-w-0 flex-1 truncate rounded-lg border border-theme-control-border bg-theme-control px-3 py-2 text-code-md text-on-surface-variant",
            !hasCustomDirectory && "font-sans",
          )}
          title={displayPath}
        >
          {displayPath}
        </span>
        {hasCustomDirectory && (
          <Button onClick={onOpen} type="button" variant="outline">
            <FolderOpen size={15} />
            <span>{t("settings.storage.open")}</span>
          </Button>
        )}
        <Button onClick={onPick} type="button" variant="outline">
          <FolderOpen size={15} />
          <span>{t("settings.storage.chooseDirectory")}</span>
        </Button>
        {hasCustomDirectory && (
          <Button
            aria-label={t("settings.storage.clearDataBackupDir")}
            onClick={onClear}
            size="icon"
            title={t("settings.storage.clearDataBackupDir")}
            type="button"
            variant="ghost"
          >
            <X size={15} />
          </Button>
        )}
      </div>
    </SettingRow>
  );
}

function CliToolsInstallRow({
  error,
  installing,
  onInstall,
  status,
  t,
}: {
  error: string;
  installing: boolean;
  onInstall: () => void;
  status: CliToolsStatus | null;
  t: Translator;
}) {
  const ready = Boolean(status?.bundled && status.installed && status.path_configured);
  const statusText = !status
    ? t("settings.cli.loading")
    : ready
      ? t("settings.cli.ready")
      : status.installed
        ? t("settings.cli.needsTerminalRestart")
        : status.bundled
          ? t("settings.cli.notInstalled")
          : t("settings.cli.notBundled");
  const installLabel = status?.installed ? t("settings.cli.repair") : t("settings.cli.install");
  const detail = error || status?.message || t("settings.cli.loading");

  return (
    <SettingRow icon={<Terminal size={18} />} label={t("settings.cli.title")}>
      <div className="flex w-[min(38rem,52vw)] min-w-0 items-center gap-2">
        <div className="min-w-0 flex-1 rounded-lg border border-theme-control-border bg-theme-control px-3 py-2">
          <div className="flex min-w-0 items-center gap-2">
            <span
              className={clsx(
                "size-2 shrink-0 rounded-full",
                ready ? "bg-status-create" : error ? "bg-status-remove" : "bg-status-update",
              )}
              aria-hidden="true"
            />
            <span className="truncate text-body-sm font-semibold text-on-surface">{statusText}</span>
          </div>
          <p className={clsx("mt-1 truncate text-body-sm", error ? "text-status-remove" : "text-on-surface-variant")} title={detail}>
            {detail}
          </p>
          {status?.install_dir && (
            <code className="mt-1 block truncate text-code-md text-on-surface-variant" title={status.install_dir}>
              {status.install_dir}
            </code>
          )}
        </div>
        <Button disabled={installing || status?.bundled === false} onClick={onInstall} type="button" variant="outline">
          <Terminal size={15} />
          <span>{installing ? t("settings.cli.installing") : installLabel}</span>
        </Button>
      </div>
    </SettingRow>
  );
}

function RangeSettingControl({
  label,
  max,
  min,
  onChange,
  step,
  unit = "",
  value,
}: {
  label: string;
  max: number;
  min: number;
  onChange: (value: number) => void;
  step: number;
  unit?: string;
  value: number;
}) {
  return (
    <div className="flex w-72 items-center gap-3">
      <input
        aria-label={label}
        aria-valuetext={`${value}${unit}`}
        className="aurora-range h-2 min-w-0 flex-1 cursor-pointer appearance-none rounded-full"
        max={max}
        min={min}
        onChange={(event) => onChange(Number(event.target.value))}
        step={step}
        type="range"
        value={value}
        style={{ "--range-progress": `${((value - min) / (max - min)) * 100}%` } as CSSProperties}
      />
      <output className="w-16 rounded-xl border border-theme-control-border/70 bg-theme-control/70 px-2 py-1 text-center font-mono text-body-sm text-on-surface shadow-[var(--theme-shadow-control-inset)]">
        {value}{unit}
      </output>
    </div>
  );
}

function FontFamilyControl({
  fallback,
  label,
  onChange,
  t,
  value,
}: {
  fallback: FontFallbackKind;
  label: string;
  onChange: (value: FontFamilyValue) => void;
  t: Translator;
  value: FontFamilyValue;
}) {
  const presetOption = value.preset === "custom"
    ? null
    : fontFamilyOptions.find((option) => option.id === value.preset);
  const customSelected = value.preset === "custom";
  const inputValue = customSelected ? value.customFontFamily : presetOption?.value ?? "";

  return (
    <div className="grid w-[min(34rem,100%)] grid-cols-[minmax(9rem,0.42fr)_minmax(0,1fr)] gap-2 max-[760px]:grid-cols-1">
      <select
        aria-label={t("settings.font.preset")}
        className="h-10 rounded-xl border border-theme-control-border bg-theme-control px-3 text-body-sm font-semibold text-on-surface outline-none transition-colors focus:border-primary-strong/60"
        onChange={(event) => {
          onChange({
            ...value,
            preset: event.target.value as FontFamilyPresetId,
          });
        }}
        value={value.preset}
      >
        {fontFamilyOptions.map((option) => (
          <option key={option.id} value={option.id}>
            {t(option.labelKey as TranslationKey)}
          </option>
        ))}
        <option value="custom">{t("settings.font.custom")}</option>
      </select>
      <input
        aria-label={label}
        className={clsx(
          "h-10 w-full rounded-xl border border-theme-control-border bg-theme-control px-3 text-body-sm text-on-surface outline-none transition-colors placeholder:text-outline focus:border-primary-strong/60",
          !customSelected && "cursor-not-allowed opacity-70",
        )}
        disabled={!customSelected}
        maxLength={80}
        onChange={(event) =>
          onChange({
            ...value,
            customFontFamily: firstFontFamilyName(event.target.value),
            preset: "custom",
          })
        }
        placeholder={t("settings.font.customPlaceholder")}
        spellCheck={false}
        style={{ fontFamily: resolveFontFamilyCss(value, fallback) }}
        value={inputValue}
      />
    </div>
  );
}

const builtInConversationCardKinds = ["answer", "tool", "command", "code", "result"];

function ConversationContentCardColorControl({
  onChange,
  t,
  value,
}: {
  onChange: (value: ConversationContentCardColorSettings) => void;
  t: Translator;
  value: ConversationContentCardColorSettings;
}) {
  const { definitions } = useConversationCardKindRegistry();
  const [draftKind, setDraftKind] = useState("");
  const fields = Array.from(new Set([
    ...builtInConversationCardKinds,
    ...Array.from(definitions.entries())
      .filter(([kind, definition]) => !isRedundantConversationCardKind(kind, definition))
      .map(([kind]) => kind),
    ...Object.keys(value),
  ])).filter((kind) => !isRedundantConversationCardKind(kind, definitions.get(kind)));

  function commitColor(key: string, color: string) {
    const nextColor = color.trim();
    if (!isHexColor(nextColor) || nextColor === value[key]) {
      return;
    }

    onChange({
      ...value,
      [key]: nextColor.toLowerCase(),
    });
  }

  function addHistoricalKind() {
    const kind = draftKind.trim();
    if (!/^[a-z0-9][a-z0-9._-]{0,127}$/.test(kind)) return;
    onChange({ ...value, [kind]: conversationCardColor(kind, value) });
    setDraftKind("");
  }

  return (
    <div className="w-[min(42rem,100%)] space-y-3">
      <div className="grid grid-cols-2 gap-3 max-[900px]:grid-cols-1">
        {fields.map((kind) => (
          <ConversationContentCardColorField
            key={kind}
            label={definitions.get(kind)?.label ?? conversationCardLabel(kind, t)}
            onCommit={(color) => commitColor(kind, color)}
            value={conversationCardColor(kind, value)}
          />
        ))}
      </div>
      <div className="flex items-center gap-2">
        <Input
          aria-label={t("settings.conversation.addCardKind")}
          onChange={(event) => setDraftKind(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") addHistoricalKind();
          }}
          placeholder={t("settings.conversation.cardKindPlaceholder")}
          value={draftKind}
        />
        <Button onClick={addHistoricalKind} type="button" variant="outline">
          {t("settings.conversation.addCardKind")}
        </Button>
      </div>
    </div>
  );
}

function ConversationContentCardColorField({
  label,
  onCommit,
  value,
}: {
  label: string;
  onCommit: (value: string) => void;
  value: string;
}) {
  const [draft, setDraft] = useState(value);

  useEffect(() => {
    setDraft(value);
  }, [value]);

  function commitDraft(nextValue: string) {
    if (!isHexColor(nextValue)) {
      setDraft(value);
      return;
    }

    const normalized = nextValue.toLowerCase();
    setDraft(normalized);
    onCommit(normalized);
  }

  return (
    <label className="flex min-w-0 flex-col gap-1">
      <span className="text-label-caps uppercase text-outline">{label}</span>
      <div className="flex h-10 items-center gap-2 rounded-xl border border-theme-control-border bg-theme-control px-2 transition-colors focus-within:border-primary-strong/60">
        <input
          aria-label={label}
          className="size-5 shrink-0 cursor-pointer rounded border-0 bg-transparent p-0"
          onChange={(event) => commitDraft(event.target.value)}
          type="color"
          value={value}
        />
        <Input
          aria-label={label}
          className="h-auto min-w-0 flex-1 border-0 bg-transparent p-0 font-mono text-code-md focus:border-transparent"
          maxLength={7}
          onBlur={(event) => commitDraft(event.currentTarget.value)}
          onChange={(event) => setDraft(event.target.value.slice(0, 7))}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              commitDraft(event.currentTarget.value);
              event.currentTarget.blur();
            }
            if (event.key === "Escape") {
              setDraft(value);
              event.currentTarget.blur();
            }
          }}
          value={draft}
        />
      </div>
    </label>
  );
}

function ConversationRuntimeOverrideRow({
  onChange,
  t,
  value,
}: {
  onChange: (key: keyof ConversationRuntimeOverrideSettings, value: string) => void;
  t: Translator;
  value: ConversationRuntimeOverrideSettings;
}) {
  const runtimes: Array<{
    key: keyof ConversationRuntimeOverrideSettings;
    label: string;
    placeholder: string;
  }> = [
    { key: "node", label: "Node", placeholder: "node" },
    { key: "python", label: "Python", placeholder: "python3" },
    { key: "bash", label: "Bash", placeholder: "bash" },
  ];

  return (
    <SettingRow icon={<Terminal size={18} />} label={t("settings.conversation.runtimeOverrides")}>
      <div className="grid w-[min(38rem,52vw)] gap-2">
        {runtimes.map((runtime) => (
          <label className="grid grid-cols-[72px_minmax(0,1fr)_auto] items-center gap-2" key={runtime.key}>
            <span className="text-body-sm font-semibold text-on-surface">{runtime.label}</span>
            <Input
              aria-label={`${runtime.label} ${t("settings.conversation.runtimePath")}`}
              className="font-mono text-code-md"
              onChange={(event) => onChange(runtime.key, event.target.value)}
              placeholder={runtime.placeholder}
              value={value[runtime.key]}
            />
            <Button
              aria-label={`${t("settings.conversation.runtimeClear")} ${runtime.label}`}
              disabled={!value[runtime.key]}
              onClick={() => onChange(runtime.key, "")}
              size="icon-sm"
              title={t("settings.conversation.runtimeClear")}
              type="button"
              variant="ghost"
            >
              <X size={15} />
            </Button>
          </label>
        ))}
        <p className="text-body-sm text-on-surface-variant">{t("settings.conversation.runtimeOverridesHint")}</p>
      </div>
    </SettingRow>
  );
}

function AdapterRuntimeStatusRow({
  error,
  loading,
  onRefresh,
  statuses,
  t,
}: {
  error: string;
  loading: boolean;
  onRefresh: () => void;
  statuses: ConversationAdapterRuntimeStatus[];
  t: Translator;
}) {
  const allAvailable = statuses.length > 0 && statuses.every((status) => status.available);
  const statusText = loading
    ? t("settings.conversation.runtimeLoading")
    : error
      ? t("settings.conversation.runtimeError")
      : allAvailable
        ? t("settings.conversation.runtimeReady")
        : t("settings.conversation.runtimeMissing");

  return (
    <SettingRow icon={<Terminal size={18} />} label={t("settings.conversation.runtimeStatus")}>
      <div className="flex w-[min(38rem,52vw)] min-w-0 items-start gap-2">
        <div className="min-w-0 flex-1 overflow-hidden rounded-lg border border-theme-control-border bg-theme-control">
          <div className="flex min-h-10 items-center justify-between gap-3 border-b border-theme-control-border px-3 py-2">
            <div className="flex min-w-0 items-center gap-2">
              <span
                className={clsx(
                  "size-2 shrink-0 rounded-full",
                  error ? "bg-status-remove" : loading ? "bg-status-update" : allAvailable ? "bg-status-create" : "bg-status-remove",
                )}
                aria-hidden="true"
              />
              <span className="truncate text-body-sm font-semibold text-on-surface">{statusText}</span>
            </div>
            <Button disabled={loading} onClick={onRefresh} size="sm" type="button" variant="ghost">
              <RefreshCw className={clsx("size-4", loading && "animate-spin")} />
              <span>{t("settings.conversation.runtimeRefresh")}</span>
            </Button>
          </div>
          {error ? (
            <p className="px-3 py-2 text-body-sm text-status-remove">{error}</p>
          ) : (
            <div className="divide-y divide-theme-control-border">
              {(statuses.length > 0 ? statuses : runtimeStatusPlaceholders()).map((status) => (
                <div className="grid min-h-11 grid-cols-[96px_1fr_auto] items-center gap-3 px-3 py-2" key={status.kind}>
                  <span className="font-mono text-code-md font-semibold uppercase text-on-surface">{status.kind}</span>
                  <div className="min-w-0">
                    <p className="truncate font-mono text-code-md text-on-surface-variant" title={status.program}>
                      {status.program}
                    </p>
                    {status.required_version && (
                      <p className="mt-0.5 truncate text-body-sm text-on-surface-variant" title={status.required_version}>
                        {t("settings.conversation.runtimeRequires")} {status.required_version}
                      </p>
                    )}
                    {(status.version || status.error) && (
                      <p
                        className={clsx("mt-0.5 truncate text-body-sm", status.available ? "text-on-surface-variant" : "text-status-remove")}
                        title={status.version ?? status.error ?? undefined}
                      >
                        {status.version ?? status.error}
                      </p>
                    )}
                    {!status.available && status.hint && (
                      <p className="mt-0.5 text-body-sm text-on-surface-variant" title={status.hint}>
                        {status.hint}
                      </p>
                    )}
                  </div>
                  <span className={clsx("text-body-sm font-semibold", status.available ? "text-status-create" : "text-status-remove")}>
                    {status.available ? t("settings.conversation.runtimeAvailable") : t("settings.conversation.runtimeUnavailable")}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </SettingRow>
  );
}

function runtimeStatusPlaceholders(): ConversationAdapterRuntimeStatus[] {
  return ["node", "python", "bash"].map((kind) => ({
    available: false,
    error: null,
    hint: null,
    kind: kind as ConversationAdapterRuntimeStatus["kind"],
    program: kind,
    required_version: kind === "node" ? ">=20" : null,
    version: null,
  }));
}

function ThemePaletteControl({
  onChange,
  t,
  value,
}: {
  onChange: (value: ThemeId) => void;
  t: Translator;
  value: ThemeId;
}) {
  return (
    <div className="grid w-[420px] grid-cols-2 gap-2" role="radiogroup" aria-label={t("settings.theme")}>
      {themeOptions.map((option) => {
        const selected = value === option.id;

        return (
          <button
            aria-checked={selected}
            className={clsx(
              "flex h-14 items-center gap-3 rounded-xl border bg-theme-control px-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55",
              selected
                ? "border-theme-nav-active-border bg-theme-nav-active text-theme-nav-active-fg"
                : "border-theme-control-border text-theme-control-fg hover:border-theme-nav-active-border hover:bg-theme-control-hover hover:text-on-surface",
            )}
            key={option.id}
            onClick={() => onChange(option.id)}
            role="radio"
            type="button"
          >
            <span className="grid h-8 w-14 shrink-0 grid-cols-4 overflow-hidden rounded-lg border border-theme-control-border" aria-hidden="true">
              {option.swatches.map((color) => (
                <span key={color} style={{ backgroundColor: color }} />
              ))}
            </span>
            <span className="min-w-0 truncate text-body-sm font-semibold">{t(option.labelKey)}</span>
          </button>
        );
      })}
    </div>
  );
}

function setLocalizedNavigationLabel(labels: LocalizedNavigationLabels | undefined, locale: Locale, label: string): LocalizedNavigationLabels {
  return {
    ...labels,
    [locale]: label,
  };
}


function stringifyIconSvg(iconSvg: AppShortcutIconSvg) {
  return JSON.stringify(iconSvg, null, 2);
}

function parseIconSvgInput(value: string) {
  const input = value.trim();
  if (!input) {
    return { iconSvg: null };
  }

  try {
    const iconSvg = normalizeIconSvgCandidate(JSON.parse(input));
    if (iconSvg) {
      return { iconSvg };
    }
  } catch {
    // Fall through and try parsing SVG markup.
  }

  return { iconSvg: parseSvgMarkup(input) };
}

function isPlainShortcutIconInput(value: string) {
  const input = value.trim();
  return Boolean(input) && !input.startsWith("<") && !input.startsWith("{") && !input.startsWith("[");
}

function parseSvgMarkup(value: string): AppShortcutIconSvg | null {
  if (!value.includes("<svg") || typeof DOMParser === "undefined") {
    return null;
  }

  const document = new DOMParser().parseFromString(value, "image/svg+xml");
  if (document.querySelector("parsererror")) {
    return null;
  }

  const svg = document.querySelector("svg");
  if (!svg) {
    return null;
  }

  const paths = Array.from(svg.querySelectorAll("path")).flatMap((path) => {
    const d = path.getAttribute("d")?.trim();
    if (!d) {
      return [];
    }

    const fillRule = normalizeSvgRule(path.getAttribute("fill-rule") ?? path.getAttribute("fillRule"));
    const clipRule = normalizeSvgRule(path.getAttribute("clip-rule") ?? path.getAttribute("clipRule"));
    return [
      {
        d,
        ...(clipRule ? { clipRule } : {}),
        ...(fillRule ? { fillRule } : {}),
      },
    ];
  });

  if (paths.length === 0) {
    return null;
  }

  const viewBox = svg.getAttribute("viewBox")?.trim();
  return {
    paths,
    ...(viewBox ? { viewBox } : {}),
  };
}

function normalizeIconSvgCandidate(value: unknown): AppShortcutIconSvg | null {
  if (!isRecord(value) || !Array.isArray(value.paths)) {
    return null;
  }

  const paths = value.paths.flatMap((path) => {
    if (!isRecord(path) || typeof path.d !== "string") {
      return [];
    }

    const d = path.d.trim();
    if (!d) {
      return [];
    }

    const fillRule = normalizeSvgRule(path.fillRule);
    const clipRule = normalizeSvgRule(path.clipRule);
    return [
      {
        d,
        ...(clipRule ? { clipRule } : {}),
        ...(fillRule ? { fillRule } : {}),
      },
    ];
  });

  if (paths.length === 0) {
    return null;
  }

  return {
    paths,
    ...(typeof value.viewBox === "string" && value.viewBox.trim() ? { viewBox: value.viewBox.trim() } : {}),
  };
}

function normalizeSvgRule(value: unknown): "evenodd" | "nonzero" | null {
  return value === "evenodd" || value === "nonzero" ? value : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === "object" && !Array.isArray(value));
}

function SegmentedControl({
  label,
  onChange,
  options,
  value,
}: {
  label: string;
  onChange: (value: string) => void;
  options: Array<{ label: string; value: string }>;
  value: string;
}) {
  return (
    <div className="aurora-segmented-control flex h-9 items-center gap-1 rounded-2xl border border-theme-control-border/70 bg-theme-control/68 p-1 shadow-[var(--theme-shadow-control-inset)] backdrop-blur-md" aria-label={label} role="group">
      {options.map((option) => (
        <Button
          className={clsx(
            "h-7 rounded-xl px-3",
            value === option.value
              ? "bg-theme-control-hover text-primary shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.2)] hover:bg-theme-control-hover hover:text-primary"
              : "text-theme-control-fg hover:bg-transparent hover:text-on-surface",
          )}
          key={option.value}
          onClick={() => onChange(option.value)}
          size="sm"
          type="button"
          variant="ghost"
        >
          {option.label}
        </Button>
      ))}
    </div>
  );
}

function SwitchControl({
  checked,
  label,
  onChange,
}: {
  checked: boolean;
  label: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <Switch
      aria-label={label}
      checked={checked}
      onCheckedChange={onChange}
    />
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

const configurableRailItemIds = new Set(["logs", "settings"]);

function isConfigurableRailItem(item: RailMenuItem) {
  return item.position === "secondary" && configurableRailItemIds.has(item.id);
}
