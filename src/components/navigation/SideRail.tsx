import clsx from "clsx";
import { useI18n } from "../../i18n/I18nProvider";
import { railLabel } from "../../i18n/navigation";
import { MenuIcon } from "../../navigation/icons";
import type { RailMenuItem } from "../../navigation/types";

export function SideRail({
  activeId,
  items,
  onItemSelect,
}: {
  activeId: string;
  items: RailMenuItem[];
  onItemSelect?: (item: RailMenuItem) => void;
}) {
  const { t } = useI18n();
  const primaryItems = items.filter((item) => item.enabled && item.position === "primary");
  const secondaryItems = items.filter((item) => item.enabled && item.position === "secondary");

  return (
    <aside
      className="fixed inset-y-0 left-0 z-30 flex w-sidebar-width flex-col items-center justify-between border-r border-outline-variant bg-surface-low/95 px-2 py-4 backdrop-blur"
      aria-label={t("nav.aria.main")}
    >
      <div className="flex w-full flex-col items-center gap-2">
        <button
          className="mb-4 grid size-10 place-items-center rounded-xl border border-status-update/20 bg-status-update/15 text-status-update transition-transform active:scale-95"
          aria-label="AssetIWeave"
        >
          <MenuIcon name="rocket" size={22} />
        </button>
        <RailGroup activeId={activeId} items={primaryItems} onItemSelect={onItemSelect} />
      </div>

      <RailGroup activeId={activeId} items={secondaryItems} onItemSelect={onItemSelect} />
    </aside>
  );
}

function RailGroup({
  activeId,
  items,
  onItemSelect,
}: {
  activeId: string;
  items: RailMenuItem[];
  onItemSelect?: (item: RailMenuItem) => void;
}) {
  const { t } = useI18n();

  return (
    <nav className="flex w-full flex-col items-center gap-2">
      {items.map((item) => {
        const label = railLabel(item, t);

        return (
          <button
            className={clsx(
              "grid size-10 place-items-center rounded-xl border transition-all active:scale-95",
              item.id === activeId
                ? "border-outline-variant bg-surface-highest/70 text-primary"
                : "border-transparent text-on-surface-variant/75 hover:border-outline-variant hover:bg-surface-highest/70 hover:text-primary",
            )}
            key={item.id}
            aria-label={label}
            aria-current={item.id === activeId ? "page" : undefined}
            onClick={() => onItemSelect?.(item)}
            title={label}
            type="button"
          >
            <MenuIcon name={item.icon} />
          </button>
        );
      })}
    </nav>
  );
}
