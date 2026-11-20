import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { ESLint } from "eslint";
import { describe, expect, it } from "vitest";

const sourceRoot = fileURLToPath(new URL(".", import.meta.url));
const repoRoot = path.resolve(sourceRoot, "../..");

function collectCssFiles(dir: string): string[] {
  const results: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      results.push(...collectCssFiles(fullPath));
    } else if (entry.isFile() && entry.name.endsWith(".css")) {
      results.push(fullPath);
    }
  }
  return results;
}

describe("frontend architecture boundaries", () => {
  it("keeps settings on the Radix-backed fullscreen dialog path", () => {
    const source = readFileSync(
      new URL(
        "components/settings/GlobalSettingsDialog.tsx",
        `file://${sourceRoot}`,
      ),
      "utf8",
    );

    expect(source).toContain("FullscreenDialogFrame");
    expect(source).not.toContain("document.body.style.overflow");
    expect(source).not.toContain("document.documentElement.style.overflow");
  });

  it("keeps modal scrolling on Radix instead of feature-level style mutations", () => {
    const source = readFileSync(
      new URL("components/foundation/DialogFrame.tsx", `file://${sourceRoot}`),
      "utf8",
    );
    expect(source).not.toContain("style.overflow");
    expect(source).not.toContain("previousDocumentOverflow");
    expect(source).not.toContain("zIndexClasses");
    expect(source).not.toContain("paddingClasses");
    expect(source).toContain("layerClassName");
  });

  it("keeps the conversation controller independent from presentation components", () => {
    const source = readFileSync(
      new URL(
        "hooks/conversations/useConversationsController.ts",
        `file://${sourceRoot}`,
      ),
      "utf8",
    );
    expect(source).not.toContain("../components/");
    expect(source).not.toContain("../../components/");
    expect(source).toContain("../../types");
  });

  it("keeps conversation selection transitions in the controller", () => {
    const source = readFileSync(
      new URL(
        "pages/conversations/ConversationsPage.tsx",
        `file://${sourceRoot}`,
      ),
      "utf8",
    );
    expect(source).not.toMatch(/setSelected(?:App|Project|Session|Question)/);
    expect(source).not.toContain("setSessionView(");
    expect(source).not.toContain("setActiveSearchTarget(");
  });

  it("blocks raw truncation classes in business code via ESLint while preserving whitelists and Tauri import guard", async () => {
    const eslint = new ESLint({
      cwd: repoRoot,
      overrideConfigFile: path.join(repoRoot, "eslint.config.mjs"),
    });

    const lintSnippet = async (code: string, relativePath: string) => {
      const [result] = await eslint.lintText(code, {
        filePath: path.join(repoRoot, relativePath),
      });
      return result.messages;
    };

    const blockedSamples = [
      `export const A = () => <span className="truncate">hi</span>;`,
      `export const B = () => <span titleClassName="line-clamp-2">hi</span>;`,
      "export const C = ({ extra }: { extra: string }) => <span className={`block text-ellipsis ${extra}`}>hi</span>;",
      `import { cn } from "@/lib/utils"; export const D = () => cn("line-clamp-1");`,
      "import { clsx } from 'clsx'; export const E = () => clsx(`line-clamp-3`);",
      `import { cva } from "class-variance-authority"; export const F = cva("truncate");`,
    ];

    for (const sample of blockedSamples) {
      const messages = await lintSnippet(
        sample,
        "frontend/src/components/example/SampleCard.tsx",
      );
      const truncationErrors = messages.filter(
        (m) =>
          m.ruleId === "no-restricted-syntax" &&
          m.message.includes("TruncatedText"),
      );
      expect(truncationErrors.length, sample).toBeGreaterThan(0);
    }

    // Long body previews (line-clamp-6 / line-clamp-8) remain allowed
    const allowedLongPreview = await lintSnippet(
      `export const Long = () => <p className="line-clamp-6">body</p>;`,
      "frontend/src/components/prompts/PromptPreview.tsx",
    );
    expect(
      allowedLongPreview.filter((m) => m.ruleId === "no-restricted-syntax"),
    ).toEqual([]);

    // Whitelisted paths remain allowed to use raw truncation classes
    for (const whitelistedPath of [
      "frontend/src/components/foundation/TruncatedText.tsx",
      "frontend/src/components/ui/select.tsx",
      "frontend/src/components/example/SampleCard.test.tsx",
    ]) {
      const messages = await lintSnippet(
        `export const Allowed = () => <span className="truncate line-clamp-2">ok</span>;`,
        whitelistedPath,
      );
      expect(
        messages.filter((m) => m.ruleId === "no-restricted-syntax"),
        whitelistedPath,
      ).toEqual([]);
    }

    // Existing Tauri dynamic import guard must not be overwritten in business files,
    // while services/** remains allowed to dynamically import Tauri and still checked for truncation.
    const tauriViolation = await lintSnippet(
      `export const load = () => import("@tauri-apps/api/core");`,
      "frontend/src/components/example/SampleCard.tsx",
    );
    expect(
      tauriViolation.some(
        (m) =>
          m.ruleId === "no-restricted-syntax" &&
          m.message.includes("frontend/src/services/**"),
      ),
    ).toBe(true);

    const serviceTauriAllowed = await lintSnippet(
      `export const load = () => import("@tauri-apps/api/core");`,
      "frontend/src/services/exampleService.ts",
    );
    expect(
      serviceTauriAllowed.filter((m) => m.ruleId === "no-restricted-syntax"),
    ).toEqual([]);

    const serviceTruncationBlocked = await lintSnippet(
      `import { cn } from "@/lib/utils"; export const x = () => cn("truncate");`,
      "frontend/src/services/exampleService.ts",
    );
    expect(
      serviceTruncationBlocked.some(
        (m) =>
          m.ruleId === "no-restricted-syntax" &&
          m.message.includes("TruncatedText"),
      ),
    ).toBe(true);
  });

  it("forbids CSS stylesheet truncation bypasses under frontend/src", () => {
    const cssFiles = collectCssFiles(sourceRoot);
    expect(cssFiles.length).toBeGreaterThan(0);

    for (const cssFile of cssFiles) {
      const content = readFileSync(cssFile, "utf8");
      const relativePath = path.relative(repoRoot, cssFile);
      expect(content, relativePath).not.toMatch(/text-overflow:\s*ellipsis/i);
      expect(content, relativePath).not.toMatch(/-webkit-line-clamp\s*:/i);
    }
  });

  it("blocks new raw native/control title= attributes via ESLint and ratchets legacy occurrences", async () => {
    const eslint = new ESLint({
      cwd: repoRoot,
      overrideConfigFile: path.join(repoRoot, "eslint.config.mjs"),
    });

    const lintSnippet = async (code: string, relativePath: string) => {
      const [result] = await eslint.lintText(code, {
        filePath: path.join(repoRoot, relativePath),
      });
      return result.messages;
    };

    for (const blocked of [
      `export const A = () => <span title="原生提示">text</span>;`,
      `export const B = () => <button type="button" title="关闭">x</button>;`,
      `export const C = ({ Button }: any) => <Button title="按钮">ok</Button>;`,
      `export const D = ({ Badge }: any) => <Badge title="徽标">tag</Badge>;`,
      `export const E = ({ TruncatedText }: any) => <TruncatedText title="文本">hi</TruncatedText>;`,
    ]) {
      const messages = await lintSnippet(
        blocked,
        "frontend/src/components/example/SampleCard.tsx",
      );
      expect(
        messages.some(
          (m) =>
            m.ruleId === "no-restricted-syntax" &&
            m.message.includes("GlobalNativeTitleBridge"),
        ),
        blocked,
      ).toBe(true);
    }

    // Custom component heading props and SVG <title> elements remain allowed
    for (const allowed of [
      `export const F = ({ DialogFrame }: any) => <DialogFrame title="标题" />;`,
      `export const G = ({ PageHeader }: any) => <PageHeader title="页面标题" />;`,
      `export const H = () => <svg><title>图标语义</title></svg>;`,
    ]) {
      const messages = await lintSnippet(
        allowed,
        "frontend/src/components/example/SampleCard.tsx",
      );
      expect(
        messages.filter((m) => m.ruleId === "no-restricted-syntax"),
        allowed,
      ).toEqual([]);
    }

    // Legacy files cannot grow above their documented native title= ratchet baseline
    const baseline = JSON.parse(
      readFileSync(
        path.join(repoRoot, "scripts/native-title-baseline.json"),
        "utf8",
      ),
    ) as { selector: string; counts: Record<string, number> };

    const tseslint = await import("typescript-eslint");
    const ratchetEslint = new ESLint({
      cwd: repoRoot,
      overrideConfigFile: true,
      overrideConfig: [
        tseslint.default.configs.base,
        {
          files: ["frontend/src/**/*.{ts,tsx}"],
          ignores: [
            "frontend/src/**/*.test.{ts,tsx}",
            "frontend/src/**/__fixtures__/**",
          ],
          rules: {
            "no-restricted-syntax": [
              "warn",
              { selector: baseline.selector, message: "NATIVE_TITLE_HIT" },
            ],
          },
        },
      ],
    });

    const results = await ratchetEslint.lintFiles(Object.keys(baseline.counts));
    for (const result of results) {
      const rel = path
        .relative(repoRoot, result.filePath)
        .split(path.sep)
        .join("/");
      const actual = result.messages.filter(
        (m) => m.message === "NATIVE_TITLE_HIT",
      ).length;
      const limit = baseline.counts[rel] ?? 0;
      expect(
        actual,
        `${rel} exceeded native title= baseline`,
      ).toBeLessThanOrEqual(limit);
    }
  });
});
