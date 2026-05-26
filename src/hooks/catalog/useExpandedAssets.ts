import { useState } from "react";

export function useExpandedAssets() {
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());

  function toggleAsset(id: string) {
    setExpandedIds((current) => {
      const next = new Set(current);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }

  return { expandedIds, toggleAsset };
}
