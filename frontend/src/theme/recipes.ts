import { cva } from "class-variance-authority";

export const panelRecipe = cva(
  "rounded-2xl border text-on-surface backdrop-blur-xl transition-[transform,box-shadow,border-color] duration-200",
  {
    variants: {
      variant: {
        default:
          "border-theme-card-border/75 bg-[linear-gradient(145deg,rgb(var(--theme-card-bg)/0.78),rgb(var(--theme-card-header)/0.72))] shadow-[var(--theme-shadow-card),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.14)] hover:translate-y-[var(--theme-hover-lift)] hover:border-theme-nav-active-border/60",
        muted:
          "border-theme-card-border/60 bg-[linear-gradient(145deg,rgb(var(--theme-card-bg)/0.58),rgb(var(--theme-card-header)/0.52))] shadow-[var(--theme-shadow-card),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.1)]",
        inset:
          "rounded-xl border-theme-card-border/70 bg-[linear-gradient(145deg,rgb(var(--theme-card-header)/0.72),rgb(var(--theme-control-bg)/0.58))] shadow-[var(--theme-shadow-control-inset)]",
        toolbar:
          "rounded-xl border-theme-card-border/70 bg-[linear-gradient(135deg,rgb(var(--theme-toolbar-bg)/0.76),rgb(var(--theme-card-header)/0.68))] shadow-[var(--theme-shadow-toolbar),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.12)] backdrop-blur-xl",
      },
      padding: {
        none: "p-0",
        sm: "p-3",
        md: "p-4",
        lg: "p-5",
      },
    },
    defaultVariants: {
      variant: "default",
      padding: "md",
    },
  },
);

export const controlRecipe = cva(
  "rounded-xl border border-theme-control-border/80 bg-[linear-gradient(145deg,rgb(var(--theme-control-bg)/0.78),rgb(var(--theme-card-header)/0.58))] text-on-surface shadow-[var(--theme-shadow-control-inset)] backdrop-blur-md transition-[background,border-color,box-shadow] placeholder:text-outline focus-visible:border-primary-strong/75 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/35 focus-visible:shadow-[0_0_0_1px_rgb(var(--theme-focus-ring)/0.24),0_0_20px_rgb(var(--theme-glow)/0.14)] disabled:cursor-not-allowed disabled:opacity-50",
  {
    variants: {
      variant: {
        input: "h-9 px-3 py-2 text-body-sm",
        textarea: "min-h-24 px-3 py-2 text-body-sm",
        select: "h-9 px-3 text-body-sm",
        frame: "px-3 py-3",
      },
    },
    defaultVariants: {
      variant: "input",
    },
  },
);

export const badgeRecipe = cva("inline-flex items-center rounded-full border px-2.5 py-1 text-label-caps uppercase shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.16)]", {
  variants: {
    tone: {
      neutral: "border-theme-control-border bg-theme-control text-on-surface-variant",
      primary: "border-primary/45 bg-primary/10 text-primary",
      create: "border-status-create/35 bg-status-create/15 text-status-create",
      update: "border-status-update/35 bg-status-update/15 text-status-update",
      remove: "border-status-remove/40 bg-status-remove/12 text-status-remove",
      conflict: "border-status-conflict/35 bg-status-conflict/12 text-status-conflict",
    },
  },
  defaultVariants: {
    tone: "neutral",
  },
});

export const dialogRecipe = cva(
  "fixed inset-0 z-50 grid place-items-center bg-[rgb(var(--theme-scrim)/0.62)] px-4 py-6 backdrop-blur-md",
);

export const iconButtonRecipe = cva(
  "grid place-items-center rounded-xl text-theme-control-fg transition-[transform,background-color,border-color,box-shadow,color] duration-200 hover:-translate-y-px hover:bg-theme-control-hover hover:text-on-surface active:translate-y-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55 disabled:cursor-not-allowed disabled:opacity-50",
  {
    variants: {
      size: {
        sm: "size-8",
        md: "size-9",
      },
      framed: {
        true: "border border-theme-control-border bg-theme-control shadow-[var(--theme-shadow-control-inset)]",
        false: "",
      },
      danger: {
        true: "hover:text-status-remove",
        false: "",
      },
    },
    defaultVariants: {
      size: "sm",
      framed: false,
      danger: false,
    },
  },
);

export const surfaceButtonRecipe = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-xl text-body-sm font-semibold transition-[transform,background,box-shadow,border-color,color] duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55 disabled:pointer-events-none disabled:opacity-50",
  {
    variants: {
      variant: {
        default:
          "theme-primary-gradient border border-primary/30 text-theme-button-primary-fg shadow-[0_10px_26px_rgb(var(--theme-glow)/0.2),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.28)] hover:-translate-y-px hover:shadow-[0_14px_32px_rgb(var(--theme-glow)/0.28)] active:translate-y-0",
        destructive:
          "theme-danger-gradient text-theme-button-primary-fg hover:-translate-y-px active:translate-y-0",
        outline:
          "border border-theme-control-border/80 bg-theme-control/70 text-theme-control-fg shadow-[var(--theme-shadow-control-inset)] backdrop-blur-md hover:-translate-y-px hover:border-primary-strong/45 hover:bg-theme-control-hover hover:text-on-surface active:translate-y-0",
        secondary: "border border-theme-control-border/40 bg-theme-control-hover/75 text-on-surface shadow-[var(--theme-shadow-control-inset)] hover:-translate-y-px hover:bg-theme-card-header active:translate-y-0",
        ghost: "text-theme-control-fg hover:bg-theme-control-hover/70 hover:text-on-surface",
        link: "text-primary underline-offset-4 hover:underline",
      },
      size: {
        default: "h-10 px-4 py-2",
        sm: "h-9 rounded-lg px-3",
        lg: "h-11 rounded-xl px-5",
        icon: "size-9",
        "icon-sm": "size-8 rounded-lg",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

export const toolbarSurfaceRecipe = cva(
  "rounded-2xl border border-theme-control-border/70 bg-theme-control/68 text-theme-control-fg shadow-[var(--theme-shadow-control-inset)] backdrop-blur-md transition-[transform,background-color,border-color,box-shadow,color] duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55 disabled:cursor-not-allowed disabled:opacity-55",
  {
    variants: {
      tone: {
        container: "",
        neutral: "hover:-translate-y-px hover:bg-theme-control-hover hover:text-on-surface active:translate-y-0",
        active: "border-primary/45 bg-theme-control-hover text-primary hover:-translate-y-px active:translate-y-0",
        primary:
          "theme-primary-gradient border-primary/30 text-theme-button-primary-fg shadow-[0_10px_24px_rgb(var(--theme-glow)/0.18),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.24)] hover:-translate-y-px hover:shadow-[0_14px_32px_rgb(var(--theme-glow)/0.28)] active:translate-y-0",
      },
    },
    defaultVariants: {
      tone: "neutral",
    },
  },
);

export const toolbarIconRecipe = cva(
  "grid place-items-center rounded-xl text-theme-control-fg transition-[transform,background-color,border-color,box-shadow,color] duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55 disabled:cursor-not-allowed disabled:opacity-55",
  {
    variants: {
      active: {
        true: "bg-theme-control-hover text-primary shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.18)]",
        false: "hover:-translate-y-px hover:bg-theme-control-hover/70 hover:text-on-surface active:translate-y-0",
      },
    },
    defaultVariants: {
      active: false,
    },
  },
);

export const switchRecipe = cva(
  "peer inline-flex h-7 w-12 shrink-0 cursor-pointer items-center rounded-full border border-theme-control-border/70 bg-theme-switch p-0.5 shadow-[var(--theme-shadow-control-inset)] transition-[background,border-color,box-shadow] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55 disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:border-primary-strong/70 data-[state=checked]:bg-theme-switch-checked",
);

export const switchThumbRecipe = cva(
  "pointer-events-none grid size-5 place-items-center rounded-full bg-theme-switch-thumb shadow-[0_2px_8px_rgb(var(--theme-panel-shadow)/0.3),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.5)] transition-[transform,background-color,box-shadow] data-[state=checked]:translate-x-5 data-[state=checked]:bg-theme-switch-checked-thumb",
);
