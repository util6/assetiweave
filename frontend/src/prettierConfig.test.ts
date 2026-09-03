import { fileURLToPath } from "node:url";
import { format, resolveConfig } from "prettier";
import { describe, expect, it } from "vitest";

describe("prettier configuration", () => {
  it("配置符合本仓库 TypeScript 格式", async () => {
    const file = fileURLToPath(new URL("./formatProbe.ts", import.meta.url));
    const options = await resolveConfig(file);
    expect(options).toMatchObject({
      tabWidth: 2,
      singleQuote: false,
      semi: true,
    });
    expect(
      await format("const label='asset'", { ...options, parser: "typescript" }),
    ).toBe('const label = "asset";\n');
  });
});
