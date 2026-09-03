import { ESLint } from "eslint";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const cwd = fileURLToPath(new URL("../../", import.meta.url));

describe("ESLint configuration and import boundaries", () => {
  it("拒绝组件直接调用 Tauri，允许 services 调用", async () => {
    const eslint = new ESLint({ cwd });
    const source = 'import { invoke } from "@tauri-apps/api/core"; export const load = () => invoke("list_assets");';
    const [component] = await eslint.lintText(source, {
      filePath: "frontend/src/components/ImportBoundaryProbe.ts",
    });
    const [service] = await eslint.lintText(source, {
      filePath: "frontend/src/services/importBoundaryProbe.ts",
    });
    expect(component.messages.some((m) => m.ruleId === "no-restricted-imports")).toBe(true);
    expect(service.messages.some((m) => m.ruleId === "no-restricted-imports")).toBe(false);
  });

  it("拒绝组件直接动态导入 Tauri，允许 services 动态导入", async () => {
    const eslint = new ESLint({ cwd });
    const source = 'export const load = async () => { const { invoke } = await import("@tauri-apps/api/core"); return invoke("list_assets"); };';
    const [component] = await eslint.lintText(source, {
      filePath: "frontend/src/components/DynamicImportBoundaryProbe.ts",
    });
    const [service] = await eslint.lintText(source, {
      filePath: "frontend/src/services/dynamicImportBoundaryProbe.ts",
    });
    expect(component.messages.some((m) => m.ruleId === "no-restricted-syntax")).toBe(true);
    expect(service.messages.some((m) => m.ruleId === "no-restricted-syntax")).toBe(false);
  });

  it("允许非 services 模块进行 Tauri 类型导入", async () => {
    const eslint = new ESLint({ cwd });
    const source = 'import type { DownloadEvent } from "@tauri-apps/plugin-updater"; export type Handler = (e: DownloadEvent) => void;';
    const [component] = await eslint.lintText(source, {
      filePath: "frontend/src/components/TypeImportBoundaryProbe.ts",
    });
    expect(component.messages.some((m) => m.ruleId === "no-restricted-imports")).toBe(false);
  });
});
