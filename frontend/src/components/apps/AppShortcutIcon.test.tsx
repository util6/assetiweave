/* @vitest-environment jsdom */

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { AppShortcutIconSvg } from "../../types";
import { AppShortcutIcon } from "./AppShortcutIcon";

describe("AppShortcutIcon", () => {
  it("renders a built-in icon from the unified SVG asset catalog", () => {
    const html = renderToStaticMarkup(
      <AppShortcutIcon appKind="claude" className="icon" displayIcon="app:claude" />,
    );

    expect(html).toContain('class="icon"');
    expect(html).toContain('viewBox="0 0 24 24"');
    expect(html).toContain("M4.709 15.955");
  });

  it("keeps a saved custom SVG ahead of the built-in app icon", () => {
    const customIcon: AppShortcutIconSvg = {
      paths: [{ d: "M0 0h24v24H0z" }],
      viewBox: "0 0 24 24",
    };
    const html = renderToStaticMarkup(
      <AppShortcutIcon appKind="claude" displayIcon="app:claude" iconSvg={customIcon} />,
    );

    expect(html).toContain("M0 0h24v24H0z");
    expect(html).not.toContain("M4.709 15.955");
  });

  it("renders newly added app icons such as kiro and zcode", () => {
    const kiroHtml = renderToStaticMarkup(
      <AppShortcutIcon appKind="kiro" className="icon" displayIcon="app:kiro" />,
    );
    expect(kiroHtml).toContain('viewBox="0 0 1024 1024"');
    expect(kiroHtml).toContain("M507.03104 244.768");

    const zcodeHtml = renderToStaticMarkup(
      <AppShortcutIcon appKind="zcode" className="icon" displayIcon="app:zcode" />,
    );
    expect(zcodeHtml).toContain('viewBox="0 0 1024 1024"');
    expect(zcodeHtml).toContain("M515.072 154.624L437.76");
  });

  it("resolves custom profiles with matched profileId and legacy single-letter icon", () => {
    const zcodeCustomHtml = renderToStaticMarkup(
      <AppShortcutIcon appKind="custom" className="icon" displayIcon="Z" profileId="zcode" profileName="Zcode" />,
    );
    expect(zcodeCustomHtml).toContain('viewBox="0 0 1024 1024"');
    expect(zcodeCustomHtml).toContain("M515.072 154.624L437.76");

    const kiroCustomHtml = renderToStaticMarkup(
      <AppShortcutIcon appKind="custom" className="icon" displayIcon="K" profileId="kiro" profileName="Kiro" />,
    );
    expect(kiroCustomHtml).toContain('viewBox="0 0 1024 1024"');
    expect(kiroCustomHtml).toContain("M507.03104 244.768");
  });

  it("keeps text fallback for unmatched custom profiles", () => {
    const fallbackHtml = renderToStaticMarkup(
      <AppShortcutIcon appKind="custom" className="icon" displayIcon="+" profileId="custom" profileName="Custom" />,
    );
    expect(fallbackHtml).toContain('<span class="icon">+</span>');
  });
});
