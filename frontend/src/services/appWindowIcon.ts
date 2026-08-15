export type AppWindowIconState = "display" | "minimized";

const appWindowIconAssets: Record<AppWindowIconState, string> = {
  display: new URL("../../../assets/assetiweave/app-icon-display.png", import.meta.url).href,
  minimized: new URL("../../../assets/assetiweave/app-icon-minimized.png", import.meta.url).href,
};

let appliedState: AppWindowIconState | null = null;
let iconUpdateQueue = Promise.resolve();

export function appWindowIconAsset(state: AppWindowIconState) {
  return appWindowIconAssets[state];
}

export function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function setAppWindowIcon(state: AppWindowIconState) {
  if (!isTauriRuntime() || appliedState === state) {
    return Promise.resolve();
  }

  iconUpdateQueue = iconUpdateQueue.then(async () => {
    if (appliedState === state) {
      return;
    }

    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const response = await fetch(appWindowIconAsset(state));
      if (!response.ok) {
        throw new Error(`Failed to load app icon (${response.status})`);
      }

      const iconBytes = new Uint8Array(await response.arrayBuffer());
      await getCurrentWindow().setIcon(iconBytes);
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_app_window_icon", { icon: Array.from(iconBytes) });
      appliedState = state;
    } catch (error) {
      console.error("Failed to update AssetIWeave window icon", error);
    }
  });

  return iconUpdateQueue;
}

export async function observeAppWindowIconState(onStateChange: (state: AppWindowIconState) => void) {
  if (!isTauriRuntime()) {
    return () => undefined;
  }

  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const currentWindow = getCurrentWindow();
  let disposed = false;
  let syncing = false;

  const sync = async () => {
    if (disposed || syncing) {
      return;
    }

    syncing = true;
    try {
      const state: AppWindowIconState = (await currentWindow.isMinimized()) ? "minimized" : "display";
      if (!disposed) {
        onStateChange(state);
        await setAppWindowIcon(state);
      }
    } catch (error) {
      console.error("Failed to read AssetIWeave window state", error);
    } finally {
      syncing = false;
    }
  };

  const unlistenResized = await currentWindow.onResized(() => {
    void sync();
  });
  const unlistenFocusChanged = await currentWindow.onFocusChanged(() => {
    void sync();
  });
  const poll = window.setInterval(() => {
    void sync();
  }, 750);

  void sync();

  return () => {
    disposed = true;
    window.clearInterval(poll);
    unlistenResized();
    unlistenFocusChanged();
  };
}
