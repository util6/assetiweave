import { useState } from "react";

export function useExpandedAssets() {
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());

  function toggleAsset(id: string) {
    setExpandedIds((current) => {
      if (current.has(id)) {
        return new Set();
      }
      return new Set([id]);
    });
  }

  return { expandedIds, toggleAsset };
}
