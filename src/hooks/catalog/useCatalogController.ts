import { useState } from "react";
import { type NotificationMessage } from "../../components/notifications/NotificationBanner";
import { revealPath } from "../../services/catalog";
import { useAssetFilter } from "./useAssetFilter";
import { useCatalogData } from "./useCatalogData";
import { useCatalogOperations } from "./useCatalogOperations";
import { useExpandedAssets } from "./useExpandedAssets";
import { useMountSelection } from "./useMountSelection";

export function useCatalogController() {
  const catalogData = useCatalogData();
  const operations = useCatalogOperations(catalogData.refreshOverview);
  const { expandedIds, toggleAsset } = useExpandedAssets();
  const { selectedMounts, toggleMountProfile } = useMountSelection();
  const [query, setQuery] = useState("");
  const [notification, setNotification] = useState<NotificationMessage | null>({
    id: "mvp-notification-outlet",
    tone: "success",
    messageKey: "notification.ready",
  });
  const filteredAssets = useAssetFilter(catalogData.assets, query);

  function dismissNotification(id: string) {
    setNotification((current) => (current?.id === id ? null : current));
  }

  return {
    ...catalogData,
    ...operations,
    dismissNotification,
    expandedIds,
    filteredAssets,
    notification,
    query,
    revealPath,
    selectedMounts,
    setQuery,
    toggleAsset,
    toggleMountProfile,
  };
}
