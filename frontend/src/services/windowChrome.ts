export type WindowAction = "close" | "minimize" | "toggleMaximize";

export async function runWindowAction(action: WindowAction) {
  if (!isTauriRuntime()) {
    return;
  }

  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const currentWindow = getCurrentWindow();

    if (action === "minimize") {
      await currentWindow.minimize();
      return;
    }

    if (action === "toggleMaximize") {
      await currentWindow.toggleMaximize();
      return;
    }

    await currentWindow.close();
  } catch (error) {
    console.error("Window action failed", error);
  }
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
