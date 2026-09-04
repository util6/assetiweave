import { describe, expect, it } from "vitest";
import { createAppI18n } from "./createAppI18n";

describe("createAppI18n", () => {
  it("initializes an i18next instance with chinese locale", async () => {
    const i18n = await createAppI18n("zh");
    expect(i18n.language).toBe("zh");
    expect(i18n.t("app.title")).toBe("资产目录");
    expect(i18n.t("common.confirm")).toBe("确认");
  });

  it("initializes an i18next instance with english locale", async () => {
    const i18n = await createAppI18n("en");
    expect(i18n.language).toBe("en");
    expect(i18n.t("app.title")).toBe("Asset Catalog");
    expect(i18n.t("common.confirm")).toBe("Confirm");
  });

  it("handles interpolation parameters with {{param}} syntax", async () => {
    const i18n = await createAppI18n("zh");
    expect(i18n.t("ai.execution.global.title", { count: 3 })).toBe(
      "Agent 任务运行中 · 3",
    );
    expect(i18n.t("error.assetNotFound", { assetId: "asset-123" })).toBe(
      "未找到资产：asset-123",
    );

    const i18nEn = await createAppI18n("en");
    expect(i18nEn.t("ai.execution.global.title", { count: 5 })).toBe(
      "Agent tasks running · 5",
    );
    expect(i18nEn.t("error.assetNotFound", { assetId: "asset-123" })).toBe(
      "Asset not found: asset-123",
    );
  });

  it("falls back to zh when translation key is missing in en", async () => {
    const i18n = await createAppI18n("en");
    // Change language works
    await i18n.changeLanguage("zh");
    expect(i18n.language).toBe("zh");
    expect(i18n.t("common.save")).toBe("保存");
  });
});
