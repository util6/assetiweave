import clsx from "clsx";
import { toolbarIconRecipe } from "../../theme/recipes";

export function IconButton({
  icon,
  label,
  compact = false,
  onClick,
  disabled = false,
}: {
  icon: React.ReactNode;
  label: string;
  compact?: boolean;
  onClick?: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      className={clsx(
        toolbarIconRecipe({ className: compact ? "size-7" : "size-9 border border-theme-control-border bg-theme-control/95 shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)]" }),
      )}
      aria-label={label}
      onClick={onClick}
      disabled={disabled}
      type="button"
    >
      {icon}
    </button>
  );
}
