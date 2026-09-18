import { useEffect, useState } from "react";
import type { TeamViewMode } from "./TeamViewToggle";

const VIEW_MODE_KEY_PREFIX = "assetiweave:team-view-mode:";
const COMPACT_BREAKPOINT_PX = 768;

export function useTeamViewMode(teamId: string): {
  preferredMode: TeamViewMode;
  effectiveMode: TeamViewMode;
  setMode: (mode: TeamViewMode) => void;
  isCompactScreen: boolean;
} {
  const [preferredMode, setPreferredModeState] = useState<TeamViewMode>(() => {
    try {
      const stored = localStorage.getItem(`${VIEW_MODE_KEY_PREFIX}${teamId}`);
      if (stored === "parallel" || stored === "single") {
        return stored;
      }
    } catch {
      // ignore localStorage read error
    }
    return "parallel";
  });

  const [isCompactScreen, setIsCompactScreen] = useState<boolean>(() => {
    if (typeof window === "undefined") return false;
    return window.innerWidth < COMPACT_BREAKPOINT_PX;
  });

  useEffect(() => {
    if (typeof window === "undefined") return;

    const handleResize = () => {
      setIsCompactScreen(window.innerWidth < COMPACT_BREAKPOINT_PX);
    };

    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, []);

  const setMode = (mode: TeamViewMode) => {
    setPreferredModeState(mode);
    try {
      localStorage.setItem(`${VIEW_MODE_KEY_PREFIX}${teamId}`, mode);
    } catch {
      // ignore localStorage write error
    }
  };

  // If on compact screen, force single view without modifying preferredMode
  const effectiveMode: TeamViewMode = isCompactScreen ? "single" : preferredMode;

  return {
    preferredMode,
    effectiveMode,
    setMode,
    isCompactScreen,
  };
}
