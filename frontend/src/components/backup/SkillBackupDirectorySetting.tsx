import { Archive, FolderOpen } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import { abbreviateHomePath } from "../../utils/path";
import { Button } from "../ui/button";

export function SkillBackupDirectorySetting({
  onOpen,
  rootPath,
}: {
  onOpen: () => void;
  rootPath?: string;
}) {
  const { t } = useI18n();
  const displayPath = rootPath ? abbreviateHomePath(rootPath) : t("common.loading");

  return (
    <div className="grid min-h-20 grid-cols-[minmax(0,1fr)_auto] items-center gap-5 border-b border-theme-card-border px-4 py-3 last:border-b-0 max-[720px]:grid-cols-1">
      <div className="flex min-w-0 items-center gap-3">
        <span className="grid size-9 shrink-0 place-items-center rounded-xl border border-theme-control-border bg-theme-control text-primary">
          <Archive size={18} />
        </span>
        <div className="min-w-0">
          <div className="text-body-md font-semibold text-on-surface">{t("backup.setting.directory")}</div>
          <div
            className="mt-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm text-on-surface-variant"
            title={displayPath}
          >
            {displayPath}
          </div>
        </div>
      </div>
      <Button className="max-[720px]:w-fit" onClick={onOpen} type="button" variant="outline">
        <FolderOpen size={16} />
        {t("backup.action.changeDirectory")}
      </Button>
    </div>
  );
}
