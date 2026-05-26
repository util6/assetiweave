import { useState } from "react";

export function useMountSelection() {
  const [selectedMounts, setSelectedMounts] = useState<Record<string, string[]>>({});

  function toggleMountProfile(assetId: string, profileId: string) {
    setSelectedMounts((current) => {
      const selected = new Set(current[assetId] ?? []);
      if (selected.has(profileId)) {
        selected.delete(profileId);
      } else {
        selected.add(profileId);
      }
      return {
        ...current,
        [assetId]: [...selected],
      };
    });
  }

  return { selectedMounts, toggleMountProfile };
}
