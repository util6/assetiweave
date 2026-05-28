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
        "grid place-items-center text-on-surface-variant transition-colors hover:bg-surface-highest hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-50",
        compact ? "size-7 rounded-lg" : "size-9 rounded-xl border border-border bg-surface-high/90 shadow-[inset_0_1px_0_rgba(255,255,255,0.04)]",
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
