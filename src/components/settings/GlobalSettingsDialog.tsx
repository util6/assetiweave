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
  ArrowDown,
  ArrowUp,
  Bell,
  Gauge,
  GripVertical,
  Languages,
  ListTree,
  Menu,
  MousePointerClick,
  Palette,
  PanelLeft,
  PanelTop,
  RotateCcw,
  Settings,
  ShieldCheck,
  X,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { useI18n, type Translator } from "../../i18n/I18nProvider";
import type { Locale } from "../../i18n/messages";
import type { HeaderTabItem, NavigationModel, RailMenuItem, SubNavItem } from "../../router/types";
import { useAppSettings, type InterfaceDensity } from "../../store/settings/AppSettingsProvider";
import type { AppShortcut } from "../../types";

type SettingsSection = "appearance" | "menu" | "shortcuts" | "deployment" | "notifications";
type MoveDirection = -1 | 1;

interface SettingsSectionConfig {
  id: SettingsSection;
  icon: LucideIcon;
  label: string;
}

export function GlobalSettingsDialog({
  appShortcuts,
  navigationModel,
  onClose,
  onAppShortcutsChange,
  onNavigationModelChange,
  open,
}: {
  appShortcuts: AppShortcut[];
  navigationModel: NavigationModel;
  onClose: () => void;
  onAppShortcutsChange: (shortcuts: AppShortcut[]) => void;
  onNavigationModelChange: (model: NavigationModel) => void;
  open: boolean;
}) {
  const { locale, setLocale, t } = useI18n();
  const { resetSettings, settings, updateSetting } = useAppSettings();
  const [activeSection, setActiveSection] = useState<SettingsSection>("appearance");

  useEffect(() => {
    if (!open) {
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

  const sections: SettingsSectionConfig[] = [
    { id: "appearance", icon: Palette, label: t("settings.section.appearance") },
    { id: "menu", icon: Menu, label: t("settings.section.menu") },
    { id: "shortcuts", icon: MousePointerClick, label: t("settings.section.shortcuts") },
    { id: "deployment", icon: ShieldCheck, label: t("settings.section.deployment") },
    { id: "notifications", icon: Bell, label: t("settings.section.notifications") },
  ];
  const activeSectionLabel = sections.find((section) => section.id === activeSection)?.label ?? t("settings.section.appearance");

  function commitNavigationModel(nextNavigationModel: NavigationModel) {
    onNavigationModelChange(nextNavigationModel);
  }

  function commitAppShortcuts(nextAppShortcuts: AppShortcut[]) {
    onAppShortcutsChange(nextAppShortcuts);
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

  function moveAppShortcut(profileId: string, direction: MoveDirection) {
    const index = appShortcuts.findIndex((shortcut) => shortcut.profileId === profileId);
    const targetIndex = index + direction;
    if (index < 0 || targetIndex < 0 || targetIndex >= appShortcuts.length) return;

    commitAppShortcuts(swapItems(appShortcuts, index, targetIndex));
  }

  return (
    <div className="fixed inset-0 z-50 bg-background text-on-surface">
      <section
        aria-labelledby="global-settings-title"
        aria-modal="true"
        className="grid h-screen w-screen grid-cols-[288px_minmax(0,1fr)] overflow-hidden bg-surface-low"
        role="dialog"
      >
        <aside className="flex min-h-0 flex-col border-r border-border bg-surface-lowest/90">
          <div className="border-b border-border px-6 py-6">
            <div className="flex items-center gap-3">
              <span className="grid size-10 place-items-center rounded-xl border border-status-update/25 bg-status-update/15 text-status-update">
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

          <nav className="flex flex-1 flex-col gap-1 px-4 py-5" aria-label={t("settings.navAria")}>
            {sections.map((section) => {
              const Icon = section.icon;

              return (
                <Button
                  variant="ghost"
                  className={clsx(
                    "h-10 justify-start px-3",
                    activeSection === section.id
                      ? "bg-surface-highest text-primary"
                      : "text-on-surface-variant hover:bg-surface-high hover:text-on-surface",
                  )}
                  key={section.id}
                  onClick={() => setActiveSection(section.id)}
                  type="button"
                >
                  <Icon size={17} />
                  <span>{section.label}</span>
                </Button>
              );
            })}
          </nav>

          <div className="border-t border-border p-4">
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
          <header className="flex h-20 shrink-0 items-center justify-between border-b border-border px-8">
            <div className="min-w-0">
              <p className="text-label-caps uppercase text-outline">{t("settings.scope")}</p>
              <h3 className="truncate text-h2 text-on-surface">{activeSectionLabel}</h3>
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

          <div className="min-h-0 flex-1 overflow-y-auto px-8 py-6">
            {activeSection === "appearance" && (
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
                <SettingRow icon={<Activity size={18} />} label={t("settings.reduceMotion")}>
                  <SwitchControl
                    checked={settings.reduceMotion}
                    label={t("settings.reduceMotion")}
                    onChange={(checked) => updateSetting("reduceMotion", checked)}
                  />
                </SettingRow>
              </SettingsGroup>
            )}

            {activeSection === "menu" && (
              <div className="flex flex-col gap-5">
                <MenuSection icon={<PanelLeft size={18} />} title={t("settings.menu.sideRail")}>
                  {(["primary", "secondary"] as const).map((position) => {
                    const items = navigationModel.railItems.filter((item) => item.position === position);

                    return (
                      <div className="border-b border-border last:border-b-0" key={position}>
                        <div className="border-b border-border/70 bg-surface-lowest/40 px-4 py-2 text-label-caps uppercase text-outline">
                          {position === "primary" ? t("settings.menu.primary") : t("settings.menu.secondary")}
                        </div>
                        <SortableMenuList itemIds={items.map((item) => item.id)} onReorder={(orderedIds) => reorderRailItems(position, orderedIds)}>
                          {items.map((item) => (
                            <SortableMenuEditRow
                              enabled={item.enabled}
                              id={item.id}
                              key={item.id}
                              label={item.label}
                              onEnabledChange={(enabled) => updateRailItem(item.id, { enabled })}
                              onLabelChange={(label) => updateRailItem(item.id, { label })}
                              t={t}
                            />
                          ))}
                        </SortableMenuList>
                      </div>
                    );
                  })}
                </MenuSection>

                <MenuSection icon={<PanelTop size={18} />} title={t("settings.menu.headerTabs")}>
                  <SortableMenuList itemIds={navigationModel.headerTabs.map((item) => item.id)} onReorder={reorderHeaderTabs}>
                    {navigationModel.headerTabs.map((item) => (
                      <SortableMenuEditRow
                        enabled={item.enabled}
                        id={item.id}
                        key={item.id}
                        label={item.label}
                        onEnabledChange={(enabled) => updateHeaderTab(item.id, { enabled })}
                        onLabelChange={(label) => updateHeaderTab(item.id, { label })}
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
                      <div className="border-b border-border last:border-b-0" key={tab.id}>
                        <div className="border-b border-border/70 bg-surface-lowest/40 px-4 py-2 text-label-caps uppercase text-outline">
                          {tab.label}
                        </div>
                        <SortableMenuList itemIds={items.map((item) => item.id)} onReorder={(orderedIds) => reorderSubNavItems(tab.id, orderedIds)}>
                          {items.map((item) => (
                            <SortableMenuEditRow
                              enabled={item.enabled}
                              id={item.id}
                              key={item.id}
                              label={item.label}
                              onEnabledChange={(enabled) => updateSubNavItem(tab.id, item.id, { enabled })}
                              onLabelChange={(label) => updateSubNavItem(tab.id, item.id, { label })}
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

            {activeSection === "shortcuts" && (
              <MenuSection icon={<MousePointerClick size={18} />} title={t("settings.shortcuts.title")}>
                {appShortcuts.map((shortcut, index) => (
                  <ShortcutEditRow
                    key={shortcut.profileId}
                    moveDownDisabled={index === appShortcuts.length - 1}
                    moveUpDisabled={index === 0}
                    onAccentColorChange={(accentColor) => updateAppShortcut(shortcut.profileId, { accentColor })}
                    onDisplayIconChange={(displayIcon) => updateAppShortcut(shortcut.profileId, { displayIcon })}
                    onEnabledChange={(enabled) => updateAppShortcut(shortcut.profileId, { enabled })}
                    onMoveDown={() => moveAppShortcut(shortcut.profileId, 1)}
                    onMoveUp={() => moveAppShortcut(shortcut.profileId, -1)}
                    shortcut={shortcut}
                    t={t}
                  />
                ))}
              </MenuSection>
            )}

            {activeSection === "deployment" && (
              <SettingsGroup>
                <SettingRow icon={<ShieldCheck size={18} />} label={t("settings.confirmBeforeDeploy")}>
                  <SwitchControl
                    checked={settings.confirmBeforeDeploy}
                    label={t("settings.confirmBeforeDeploy")}
                    onChange={(checked) => updateSetting("confirmBeforeDeploy", checked)}
                  />
                </SettingRow>
              </SettingsGroup>
            )}

            {activeSection === "notifications" && (
              <SettingsGroup>
                <SettingRow icon={<Bell size={18} />} label={t("settings.showStartupNotification")}>
                  <SwitchControl
                    checked={settings.showStartupNotification}
                    label={t("settings.showStartupNotification")}
                    onChange={(checked) => updateSetting("showStartupNotification", checked)}
                  />
                </SettingRow>
              </SettingsGroup>
            )}
          </div>
        </div>
      </section>
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
      <CardHeader className="flex h-12 flex-row items-center gap-3 bg-surface-lowest/35 px-4 py-0">
        <span className="grid size-8 place-items-center rounded-lg border border-border bg-surface-high text-primary">{icon}</span>
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
        "grid min-h-14 grid-cols-[32px_minmax(220px,1fr)_auto] items-center gap-4 border-b border-border px-4 py-2.5 last:border-b-0",
        isDragging && "relative z-10 border-outline-variant bg-surface-highest shadow-[0_18px_44px_rgba(0,0,0,0.34)]",
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

function ShortcutEditRow({
  moveDownDisabled,
  moveUpDisabled,
  onAccentColorChange,
  onDisplayIconChange,
  onEnabledChange,
  onMoveDown,
  onMoveUp,
  shortcut,
  t,
}: {
  moveDownDisabled: boolean;
  moveUpDisabled: boolean;
  onAccentColorChange: (accentColor: string) => void;
  onDisplayIconChange: (displayIcon: string) => void;
  onEnabledChange: (enabled: boolean) => void;
  onMoveDown: () => void;
  onMoveUp: () => void;
  shortcut: AppShortcut;
  t: Translator;
}) {
  const [draftIcon, setDraftIcon] = useState(shortcut.displayIcon);
  const [draftColor, setDraftColor] = useState(shortcut.accentColor);

  useEffect(() => {
    setDraftIcon(shortcut.displayIcon);
  }, [shortcut.displayIcon]);

  useEffect(() => {
    setDraftColor(shortcut.accentColor);
  }, [shortcut.accentColor]);

  function commitIcon(value: string) {
    const nextIcon = value.trim().slice(0, 4);
    if (!nextIcon) {
      setDraftIcon(shortcut.displayIcon);
      return;
    }

    if (nextIcon !== shortcut.displayIcon) {
      onDisplayIconChange(nextIcon);
    }
  }

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
    <div className="grid min-h-16 grid-cols-[minmax(200px,1fr)_120px_170px_auto_auto] items-center gap-4 border-b border-border px-4 py-3 last:border-b-0">
      <div className="flex min-w-0 items-center gap-3">
        <span
          className="grid size-9 shrink-0 place-items-center rounded-lg border text-[13px] font-bold"
          style={{
            borderColor: `${shortcut.accentColor}66`,
            backgroundColor: `${shortcut.accentColor}1f`,
            color: shortcut.accentColor,
          }}
          aria-hidden="true"
        >
          {shortcut.displayIcon}
        </span>
        <div className="min-w-0">
          <p className="truncate text-body-md font-bold text-on-surface">{shortcut.profileName}</p>
          <p className="truncate font-mono text-code-md uppercase text-outline">{shortcut.appKind}</p>
        </div>
      </div>

      <label className="flex min-w-0 flex-col gap-1">
        <span className="text-label-caps uppercase text-outline">{t("settings.shortcuts.icon")}</span>
        <Input
          aria-label={t("settings.shortcuts.icon")}
          className="font-semibold"
          onBlur={(event) => commitIcon(event.currentTarget.value)}
          onChange={(event) => setDraftIcon(event.target.value.slice(0, 4))}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              commitIcon(event.currentTarget.value);
              event.currentTarget.blur();
            }
            if (event.key === "Escape") {
              setDraftIcon(shortcut.displayIcon);
              event.currentTarget.blur();
            }
          }}
          value={draftIcon}
        />
      </label>

      <label className="flex min-w-0 flex-col gap-1">
        <span className="text-label-caps uppercase text-outline">{t("settings.shortcuts.color")}</span>
        <div className="flex h-9 items-center gap-2 rounded-lg border border-border bg-surface-high px-2 transition-colors focus-within:border-primary-strong/60">
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

      <div className="flex items-center gap-1">
        <Button
          aria-label={t("settings.menu.moveUp")}
          className="text-on-surface-variant hover:text-on-surface"
          disabled={moveUpDisabled}
          onClick={onMoveUp}
          size="icon-sm"
          title={t("settings.menu.moveUp")}
          type="button"
          variant="ghost"
        >
          <ArrowUp size={16} />
        </Button>
        <Button
          aria-label={t("settings.menu.moveDown")}
          className="text-on-surface-variant hover:text-on-surface"
          disabled={moveDownDisabled}
          onClick={onMoveDown}
          size="icon-sm"
          title={t("settings.menu.moveDown")}
          type="button"
          variant="ghost"
        >
          <ArrowDown size={16} />
        </Button>
      </div>
    </div>
  );
}

function SettingRow({ children, icon, label }: { children: ReactNode; icon: ReactNode; label: string }) {
  return (
    <div className="flex min-h-16 items-center justify-between gap-5 border-b border-border px-4 py-3 last:border-b-0">
      <div className="flex min-w-0 items-center gap-3">
        <span className="grid size-9 shrink-0 place-items-center rounded-xl border border-border bg-surface-high text-primary">{icon}</span>
        <span className="min-w-0 truncate text-body-md font-semibold text-on-surface">{label}</span>
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function swapItems<Item>(items: Item[], firstIndex: number, secondIndex: number) {
  const nextItems = [...items];
  [nextItems[firstIndex], nextItems[secondIndex]] = [nextItems[secondIndex], nextItems[firstIndex]];
  return nextItems;
}

function isHexColor(value: string) {
  return /^#[0-9a-fA-F]{6}$/.test(value);
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
    <div className="flex h-9 items-center gap-1 rounded-xl border border-border bg-surface-high p-1" aria-label={label} role="group">
      {options.map((option) => (
        <Button
          className={clsx(
            "h-7 rounded-lg px-3",
            value === option.value
              ? "bg-surface-highest text-primary hover:bg-surface-highest hover:text-primary"
              : "text-on-surface-variant hover:bg-transparent hover:text-on-surface",
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
