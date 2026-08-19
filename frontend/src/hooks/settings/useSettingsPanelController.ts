import { useCallback, useEffect, useState } from "react";
import type { SettingsPanelId } from "../../store/settings/settingsSchema";

export interface SettingsPanelGroup {
  id: string;
  panels: Array<{ id: SettingsPanelId }>;
}

export function useSettingsPanelController({
  initialPanel,
  normalizePanel,
  open,
}: {
  initialPanel: SettingsPanelId;
  normalizePanel: (panel: SettingsPanelId) => SettingsPanelId;
  open: boolean;
}) {
  const [activePanel, setActivePanel] = useState<SettingsPanelId>(() => normalizePanel(initialPanel));
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (open) {
      setActivePanel(normalizePanel(initialPanel));
    }
  }, [initialPanel, normalizePanel, open]);

  const toggleGroupCollapsed = useCallback((groupId: string) => {
    setCollapsedGroups((current) => {
      const next = new Set(current);
      if (next.has(groupId)) next.delete(groupId);
      else next.add(groupId);
      return next;
    });
  }, []);

  const ensureGroupExpanded = useCallback((panelId: SettingsPanelId, groups: SettingsPanelGroup[]) => {
    const group = groups.find((candidate) => candidate.panels.some((panel) => panel.id === panelId));
    if (!group) return;
    setCollapsedGroups((current) => {
      if (!current.has(group.id)) return current;
      const next = new Set(current);
      next.delete(group.id);
      return next;
    });
  }, []);

  return {
    activePanel,
    collapsedGroups,
    ensureGroupExpanded,
    setActivePanel,
    toggleGroupCollapsed,
  };
}
