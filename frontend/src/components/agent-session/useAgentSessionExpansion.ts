import { useCallback, useState } from "react";

export function useAgentSessionExpansion() {
  const [manualExpansion, setManualExpansion] = useState<
    Record<string, boolean>
  >({});

  const isExpanded = useCallback(
    (id: string, defaultExpanded: boolean) => {
      if (id in manualExpansion) {
        return manualExpansion[id];
      }
      return defaultExpanded;
    },
    [manualExpansion],
  );

  const toggleExpanded = useCallback((id: string, defaultExpanded: boolean) => {
    setManualExpansion((prev) => {
      const current = id in prev ? prev[id] : defaultExpanded;
      return {
        ...prev,
        [id]: !current,
      };
    });
  }, []);

  const setExpanded = useCallback((id: string, expanded: boolean) => {
    setManualExpansion((prev) => ({
      ...prev,
      [id]: expanded,
    }));
  }, []);

  return {
    isExpanded,
    toggleExpanded,
    setExpanded,
  };
}
