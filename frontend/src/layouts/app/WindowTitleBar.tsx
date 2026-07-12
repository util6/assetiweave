import { Minus, Square, X } from "lucide-react";
import { useEffect, useState, type ComponentType, type SVGProps } from "react";
import clsx from "clsx";
import { runWindowAction, type WindowAction } from "../../services/windowChrome";

export type WindowChromeMode = "native" | "windows-frameless";
type WindowControlIcon = ComponentType<SVGProps<SVGSVGElement>>;
type NavigatorWithUserAgentData = Navigator & {
  userAgentData?: {
    platform?: string;
  };
};

const windowControls: Array<{
  action: WindowAction;
  ariaLabel: string;
  Icon: WindowControlIcon;
  title: string;
  variant?: "danger";
}> = [
  { action: "minimize", ariaLabel: "Minimize window", Icon: Minus, title: "Minimize" },
  { action: "toggleMaximize", ariaLabel: "Toggle maximize window", Icon: Square, title: "Maximize" },
  { action: "close", ariaLabel: "Close window", Icon: X, title: "Close", variant: "danger" },
];

export function WindowTitleBar({ mode: controlledMode }: { mode?: WindowChromeMode }) {
  const [detectedMode, setDetectedMode] = useState<WindowChromeMode>(() => controlledMode ?? detectWindowChromeMode());
  const mode = controlledMode ?? detectedMode;
  const customControls = mode === "windows-frameless";

  useEffect(() => {
    if (!controlledMode) {
      setDetectedMode(detectWindowChromeMode());
    }
  }, [controlledMode]);

  if (mode === "native") {
    return null;
  }

  return (
    <header
      className="fixed inset-x-0 top-0 z-[80] flex h-[var(--app-window-titlebar-height)] select-none items-center border-b border-theme-card-border bg-theme-toolbar/95 text-on-surface shadow-[0_10px_24px_rgb(var(--theme-panel-shadow)/0.24)] backdrop-blur"
      data-window-chrome={mode}
    >
      <div
        className={clsx("absolute inset-y-0 left-0", customControls ? "right-[8.25rem]" : "right-0")}
        data-tauri-drag-region="true"
      />
      <div className="pointer-events-none relative z-10 flex min-w-0 items-center gap-2 px-3 text-label-caps font-semibold">
        <span className="size-3 rounded-[3px] border border-theme-nav-active-border bg-primary-strong" />
        <span className="truncate">AssetIWeave</span>
      </div>
      {customControls ? (
        <div className="relative z-20 ml-auto flex h-full" aria-label="Window controls" role="group">
          {windowControls.map(({ action, ariaLabel, Icon, title, variant }) => (
            <button
              aria-label={ariaLabel}
              className={
                variant === "danger"
                  ? "grid h-full w-11 place-items-center text-on-surface-variant transition-colors hover:bg-status-remove hover:text-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-primary/60"
                  : "grid h-full w-11 place-items-center text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-on-surface focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-primary/60"
              }
              key={action}
              onClick={() => {
                void runWindowAction(action);
              }}
              title={title}
              type="button"
            >
              <Icon aria-hidden="true" className="size-3.5" />
            </button>
          ))}
        </div>
      ) : null}
    </header>
  );
}

export function applyWindowChromeMode() {
  if (typeof document === "undefined") {
    return;
  }

  document.documentElement.dataset.windowChrome = detectWindowChromeMode();
}

export function detectWindowChromeMode(): WindowChromeMode {
  const platform = detectPlatform();

  if (platform === "windows") {
    return "windows-frameless";
  }

  return "native";
}

function detectPlatform(): "linux" | "macos" | "unknown" | "windows" {
  if (typeof navigator === "undefined") {
    return "unknown";
  }

  const runtimeNavigator = navigator as NavigatorWithUserAgentData;
  const userAgentPlatform = runtimeNavigator.userAgentData?.platform;
  const platform = `${userAgentPlatform ?? runtimeNavigator.platform ?? ""} ${runtimeNavigator.userAgent ?? ""}`.toLowerCase();

  if (platform.includes("win")) {
    return "windows";
  }

  if (platform.includes("mac")) {
    return "macos";
  }

  if (platform.includes("linux")) {
    return "linux";
  }

  return "unknown";
}
