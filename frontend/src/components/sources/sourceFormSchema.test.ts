import { describe, expect, it } from "vitest";
import { sourceFormSchema } from "./sourceFormSchema";
import type { SourceImportFormValues } from "../../utils/sourceImport";

const valid: SourceImportFormValues = {
  rootPath: "/tmp/skills",
  name: "",
  priority: "10",
  enabled: true,
  includeGlobsText: "",
  excludeGlobsText: "",
};

describe("sourceFormSchema", () => {
  it("空路径和小数priority属于字段错误", () => {
    const result = sourceFormSchema.safeParse({
      ...valid,
      rootPath: " ",
      priority: "1.5",
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error.issues.map((i) => i.path[0]).sort()).toEqual([
        "priority",
        "rootPath",
      ]);
    }
  });

  it("空priority或整数字符串为合法输入", () => {
    expect(sourceFormSchema.safeParse({ ...valid, priority: "" }).success).toBe(
      true,
    );
    expect(
      sourceFormSchema.safeParse({ ...valid, priority: "0" }).success,
    ).toBe(true);
    expect(
      sourceFormSchema.safeParse({ ...valid, priority: "-5" }).success,
    ).toBe(true);
  });

  it("非数字priority判定为错误", () => {
    const result = sourceFormSchema.safeParse({
      ...valid,
      priority: "abc",
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error.issues[0]?.path[0]).toBe("priority");
    }
  });
});
