/// <reference types="vite/client" />
import { describe, expect, it } from "vitest";

const production = import.meta.glob("./**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

describe("ecosystem adoption boundaries", () => {
  it("生产代码没有重新导入已删除共享缓存 asyncCache", () => {
    const offenders = Object.entries(production)
      .filter(([path]) => !/\.test\.[tj]sx?$/.test(path))
      .filter(([, source]) => /from\s+["'][^"']*\/asyncCache["']/.test(source))
      .map(([path]) => path);
    expect(offenders).toEqual([]);
  });

  it("生产代码没有重新导入旧 i18n context 或手动 interpolate 机制", () => {
    const offenders = Object.entries(production)
      .filter(([path]) => !/\.test\.[tj]sx?$/.test(path))
      .filter(([, source]) => /from\s+["'][^"']*\/legacyI18n["']/.test(source))
      .map(([path]) => path);
    expect(offenders).toEqual([]);
  });
});
