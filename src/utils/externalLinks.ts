export function openExternalLink(url: string) {
  if (typeof window === "undefined") {
    return;
  }

  if (!isTauriRuntime()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }

  void import("@tauri-apps/plugin-opener")
    .then(({ openUrl }) => openUrl(url))
    .catch(() => {
      window.open(url, "_blank", "noopener,noreferrer");
    });
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
