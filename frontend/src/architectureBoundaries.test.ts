import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const sourceRoot = fileURLToPath(new URL(".", import.meta.url));

describe("frontend architecture boundaries", () => {
  it("keeps Tauri event protocol details inside services", () => {
    const providers = [
      "app/backgroundTasks/ConversationSyncProvider.tsx",
      "app/backgroundTasks/SearchIndexProvider.tsx",
      "app/backgroundTasks/MemoryTaskProvider.tsx",
      "app/backgroundTasks/SkillBackupProvider.tsx",
      "app/backgroundTasks/AiExecutionTaskProvider.tsx",
      "app/backgroundTasks/AgentLifecycleTaskProvider.tsx",
    ];

    for (const provider of providers) {
      const source = readFileSync(new URL(provider, `file://${sourceRoot}`), "utf8");
      expect(source, provider).not.toContain("@tauri-apps/api/event");
    }
  });

  it("keeps settings on the Radix-backed fullscreen dialog path", () => {
    const source = readFileSync(new URL("components/settings/GlobalSettingsDialog.tsx", `file://${sourceRoot}`), "utf8");
    expect(source).toContain("FullscreenDialogFrame");
    expect(source).not.toContain("document.body.style.overflow");
  });
});
