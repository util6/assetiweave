import { useEffect, useState } from "react";
import { ConfirmDialog } from "../common/ConfirmDialog";
import { useI18n } from "../../i18n/I18nProvider";
import type { Asset, AssetMountStatus } from "../../types";
import { displayAssetPath } from "../../utils/path";

export function AssetDeleteDialog({
  asset,
  busy,
  mountStatuses,
  onClose,
  onConfirm,
}: {
  asset: Asset | null;
  busy: boolean;
  mountStatuses: AssetMountStatus[];
  onClose: () => void;
  onConfirm: (unmount: boolean) => Promise<void>;
}) {
  const { t } = useI18n();
  const [unmount, setUnmount] = useState(false);
  const mountedCount = asset ? mountStatuses.filter((status) => status.asset_id === asset.id && status.state === "mounted").length : 0;

  useEffect(() => {
    setUnmount(false);
  }, [asset?.id]);

  if (!asset) {
    return null;
  }

  return (
    <ConfirmDialog
      busy={busy}
      confirmLabel={t("common.delete")}
      message={t("asset.deleteDialog.message", { name: asset.name })}
      onClose={onClose}
      onConfirm={() => void onConfirm(unmount)}
      open={Boolean(asset)}
      title={t("asset.deleteDialog.title")}
      tone="danger"
    >
      <div className="grid gap-3 rounded-xl border border-theme-card-border bg-theme-card/65 p-3">
        <div className="min-w-0">
          <div className="text-label-caps uppercase text-outline">{t("asset.deleteDialog.path")}</div>
          <div className="mt-1 truncate font-mono text-body-sm text-on-surface">{displayAssetPath(asset)}</div>
        </div>
        {mountedCount > 0 && (
          <>
            <div className="text-body-sm text-status-conflict">
              {t("asset.deleteDialog.mountedCount", { count: mountedCount })}
            </div>
            <label className="flex items-center gap-2 text-body-sm text-on-surface-variant">
              <input
                checked={unmount}
                className="size-4 rounded border-theme-control-border accent-primary"
                disabled={busy}
                onChange={(event) => setUnmount(event.target.checked)}
                type="checkbox"
              />
              {t("asset.deleteDialog.unmount")}
            </label>
          </>
        )}
      </div>
    </ConfirmDialog>
  );
}
