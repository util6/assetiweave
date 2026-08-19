import { useCallback, useEffect, useRef, useState } from "react";
import type { SettingsPanelId } from "../../store/settings/settingsSchema";

export interface SettingsPanelGroup {
  id: string;
  panels: Array<{ id: SettingsPanelId }>;
}

export function useSettingsPanelController({
  groups,
  initialPanel,
  normalizePanel,
  open,
}: {
  groups: SettingsPanelGroup[];
  initialPanel: SettingsPanelId;
  normalizePanel: (panel: SettingsPanelId) => SettingsPanelId;
  open: boolean;
}) {
  const [activePanel, setActivePanel] = useState<SettingsPanelId>(() => normalizePanel(initialPanel));
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());
  const previousOpenRef = useRef(open);
  const previousInitialPanelRef = useRef(initialPanel);

  const toggleGroupCollapsed = useCallback((groupId: string) => {
    setCollapsedGroups((current) => {
      const next = new Set(current);
      if (next.has(groupId)) next.delete(groupId);
      else next.add(groupId);
      return next;
    });
  }, []);

  const openPanel = useCallback((panelId: SettingsPanelId) => {
    const normalizedPanelId = normalizePanel(panelId);
    const group = groups.find((candidate) => candidate.panels.some((panel) => panel.id === normalizedPanelId));
    setActivePanel(normalizedPanelId);
    if (!group) return;
    setCollapsedGroups((current) => {
      if (!current.has(group.id)) return current;
      const next = new Set(current);
      next.delete(group.id);
      return next;
    });
  }, [groups, normalizePanel]);

  useEffect(() => {
    const shouldSyncPanel = open && (!previousOpenRef.current || previousInitialPanelRef.current !== initialPanel);
    previousOpenRef.current = open;
    previousInitialPanelRef.current = initialPanel;
    if (shouldSyncPanel) {
      openPanel(initialPanel);
    }
  }, [initialPanel, open, openPanel]);

  return {
    activePanel,
    collapsedGroups,
    openPanel,
    toggleGroupCollapsed,
  };
}
