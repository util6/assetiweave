import { CircleHelp } from "lucide-react";
import { useI18n } from "../i18n/I18nProvider";

export function ManualHelpButton({
  disabled = false,
  onOpen,
}: {
  disabled?: boolean;
  onOpen: () => void;
}) {
  const { locale } = useI18n();
  const label = locale === "zh" ? "打开当前页面使用手册" : "Open this page manual";

  return (
    <button
      aria-label={label}
      className="grid size-8 place-items-center rounded-lg border border-theme-control-border bg-theme-control/95 text-theme-control-fg shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)] transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-55"
      disabled={disabled}
      onClick={onOpen}
      title={label}
      type="button"
    >
      <CircleHelp size={17} />
    </button>
  );
}
