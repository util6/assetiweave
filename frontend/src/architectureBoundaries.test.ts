import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const sourceRoot = fileURLToPath(new URL(".", import.meta.url));

describe("frontend architecture boundaries", () => {
  it("keeps settings on the Radix-backed fullscreen dialog path", () => {
    const source = readFileSync(new URL("components/settings/GlobalSettingsDialog.tsx", `file://${sourceRoot}`), "utf8");

    expect(source).toContain("FullscreenDialogFrame");
    expect(source).not.toContain("document.body.style.overflow");
    expect(source).not.toContain("document.documentElement.style.overflow");
  });

  it("keeps modal scrolling on Radix instead of feature-level style mutations", () => {
    const source = readFileSync(new URL("components/foundation/DialogFrame.tsx", `file://${sourceRoot}`), "utf8");
    expect(source).not.toContain("style.overflow");
    expect(source).not.toContain("previousDocumentOverflow");
    expect(source).not.toContain("zIndexClasses");
    expect(source).not.toContain("paddingClasses");
    expect(source).toContain("layerClassName");
  });

  it("keeps the conversation controller independent from presentation components", () => {
    const source = readFileSync(new URL("hooks/conversations/useConversationsController.ts", `file://${sourceRoot}`), "utf8");
    expect(source).not.toContain("../components/");
    expect(source).not.toContain("../../components/");
    expect(source).toContain("../../types");
  });

  it("keeps conversation selection transitions in the controller", () => {
    const source = readFileSync(new URL("pages/conversations/ConversationsPage.tsx", `file://${sourceRoot}`), "utf8");
    expect(source).not.toMatch(/setSelected(?:App|Project|Session|Question)/);
    expect(source).not.toContain("setSessionView(");
    expect(source).not.toContain("setActiveSearchTarget(");
  });
});
