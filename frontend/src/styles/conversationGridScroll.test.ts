import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

// 左栏「问题」列的滚动面是 display: grid，但只写过一次隐式轨道。
// 若不显式声明 minmax(0, 1fr)，auto 轨道的最小尺寸取子项 min-content，
// Turn 轨里的单行省略文本（white-space: nowrap）会把整列撑到内容宽度，
// 造成内容在可视边界处被硬裁、省略号画在可视区之外（回归见 width 相关修复）。
const stylesheet = readFileSync(path.join(__dirname, "index.css"), "utf8");

function readRule(selector: string): string {
  const start = stylesheet.indexOf(`${selector} {`);
  expect(start, `missing rule ${selector}`).toBeGreaterThan(-1);
  return stylesheet.slice(start, stylesheet.indexOf("}", start));
}

describe("conversation grid scroll surfaces", () => {
  it("pins the question column to a single minmax(0,1fr) track", () => {
    const rule = readRule(".conversation-column-scroll");
    expect(rule).toContain("display: grid");
    expect(rule).toContain("grid-template-columns: minmax(0, 1fr)");
  });

  it("pins the search hit group to a single minmax(0,1fr) track", () => {
    const rule = readRule(".conversation-search-group");
    expect(rule).toContain("display: grid");
    expect(rule).toContain("grid-template-columns: minmax(0, 1fr)");
  });
});
