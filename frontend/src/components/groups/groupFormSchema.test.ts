import { describe, expect, it } from "vitest";
import {
  groupFormSchema,
  groupValuesToInput,
  parseGroupIconSvgInput,
} from "./groupFormSchema";

describe("groupFormSchema", () => {
  it("空名称拒绝，保存时只规范化表单字段", () => {
    const values = {
      name: " Review ",
      description: " ",
      color: "#10b981",
      displayIcon: " ",
      iconSvg: null,
      enabled: true,
    };
    expect(groupFormSchema.safeParse({ ...values, name: " " }).success).toBe(
      false,
    );
    expect(groupValuesToInput(groupFormSchema.parse(values))).toMatchObject({
      name: "Review",
      description: null,
      display_icon: null,
      enabled: true,
    });
  });

  it("非十六进制颜色拒绝", () => {
    const values = {
      name: "Valid Group",
      description: "Desc",
      color: "red",
      displayIcon: "",
      iconSvg: null,
      enabled: true,
    };
    expect(groupFormSchema.safeParse(values).success).toBe(false);

    const invalidHex = { ...values, color: "#ZZZZZZ" };
    expect(groupFormSchema.safeParse(invalidHex).success).toBe(false);

    const validHex = { ...values, color: "#3b82f6" };
    expect(groupFormSchema.safeParse(validHex).success).toBe(true);
  });

  it("解析合法的 JSON 和 SVG markup iconSvg", () => {
    const validJson = JSON.stringify({
      paths: [{ d: "M10 10" }],
      view_box: "0 0 20 20",
    });
    expect(parseGroupIconSvgInput(validJson)).toEqual({
      paths: [{ d: "M10 10" }],
      view_box: "0 0 20 20",
    });

    const invalidJson = JSON.stringify({ paths: [] });
    expect(parseGroupIconSvgInput(invalidJson)).toBeNull();

    expect(parseGroupIconSvgInput("")).toBeNull();
  });
});
