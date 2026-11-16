export type AppWindowIconState = "display";

const mainAppWindowIconAsset = new URL(
  "../../../assets/assetiweave/app-icon-display.png",
  import.meta.url,
).href;

let iconApplied = false;
let iconUpdateQueue = Promise.resolve();

export function appWindowIconAsset(_state: AppWindowIconState = "display") {
  return mainAppWindowIconAsset;
}

export function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function isMacOsRuntime() {
  if (typeof navigator === "undefined") {
    return false;
  }
  const platform =
    `${navigator.platform ?? ""} ${navigator.userAgent ?? ""}`.toLowerCase();
  return platform.includes("mac");
}

export function setAppWindowIcon() {
  if (!isTauriRuntime() || iconApplied) {
    return Promise.resolve();
  }

  iconUpdateQueue = iconUpdateQueue.then(async () => {
    if (iconApplied) {
      return;
    }

    try {
      const response = await fetch(appWindowIconAsset("display"));
      if (!response.ok) {
        throw new Error(`Failed to load app icon (${response.status})`);
      }

      const iconBytes = new Uint8Array(await response.arrayBuffer());

      // macOS window.setIcon is a no-op and requires desktop-only window icon updates on Windows/Linux.
      if (!isMacOsRuntime()) {
        try {
          const { getCurrentWindow } = await import("@tauri-apps/api/window");
          await getCurrentWindow().setIcon(iconBytes);
        } catch (error) {
          console.error("Failed to update native window icon", error);
        }
      }

      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_app_window_icon", { icon: Array.from(iconBytes) });
      iconApplied = true;
    } catch (error) {
      console.error("Failed to update AssetIWeave window icon", error);
    }
  });

  return iconUpdateQueue;
}
