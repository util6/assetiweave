import clsx from "clsx";

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
        "grid place-items-center text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-50",
        compact ? "size-7 rounded-lg" : "size-9 rounded-xl border border-theme-control-border bg-theme-control/95 shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)]",
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
