import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Download } from "lucide-react";
import {
  DataToolbar,
  ToolbarActionButton,
  ToolbarCluster,
  ToolbarMultiSelectDropdown,
  ToolbarSearch,
  ToolbarSingleSelectDropdown,
  ToolbarTextButton,
} from "./DataToolbar";

describe("DataToolbar", () => {
  it("clips toolbar overflow instead of wrapping controls into another row", () => {
    const html = renderToStaticMarkup(
      <DataToolbar
        actions={
          <>
            <button type="button">Export</button>
            <button type="button">Sync</button>
          </>
        }
        ariaLabel="Conversation toolbar"
        leading={
          <>
            <span>Search</span>
            <span>Questions 83</span>
            <span>Selected 0</span>
          </>
        }
      />,
    );

    expect(html).toContain("overflow-hidden");
    expect(html).toContain("data-toolbar-leading");
    expect(html).toContain("data-toolbar-actions");
    expect(html).toContain("overflow-x-auto");
    expect(html).toContain("flex-nowrap");
    expect(html).not.toContain("max-[820px]:grid-cols-1");
  });

  it("fills the available toolbar width instead of sizing to content", () => {
    const html = renderToStaticMarkup(
      <DataToolbar
        actions={<button type="button">Sync</button>}
        ariaLabel="Conversation toolbar"
        leading={<span>Search</span>}
        sticky
        stickyBleed
      />,
    );

    expect(html).toContain('data-toolbar-root');
    expect(html).toContain("w-full");
    expect(html).toContain("sticky");
    expect(html).toContain("toolbar-bleed");
    expect(html).toContain("-mx-[var(--app-page-x)]");
  });

  it("keeps clustered toolbar controls on one clipped row", () => {
    const html = renderToStaticMarkup(
      <ToolbarCluster ariaLabel="Content visibility">
        <label>回答文字</label>
        <label>工具调用</label>
        <label>命令执行</label>
      </ToolbarCluster>,
    );

    expect(html).toContain("overflow-x-auto");
    expect(html).toContain("flex-nowrap");
    expect(html).toContain("[&amp;&gt;*]:whitespace-nowrap");
    expect(html).not.toContain("flex-wrap");
  });

  it("keeps toolbar control text horizontal when space is clipped", () => {
    const html = renderToStaticMarkup(
      <DataToolbar
        actions={
          <>
            <ToolbarActionButton icon={<Download size={17} />} label="批量导出" text="批量导出" />
            <ToolbarTextButton icon={<Download size={17} />} label="设置" />
          </>
        }
        ariaLabel="Conversation toolbar"
        leading={
          <>
            <ToolbarSearch onChange={() => undefined} placeholder="搜索当前 Session 的问题..." value="" />
          </>
        }
      />,
    );

    expect(html).toContain("shrink-0");
    expect(html).toContain("whitespace-nowrap");
    expect(html).toContain('data-toolbar-control="action"');
    expect(html).toContain('data-toolbar-control="text"');
    expect(html).toContain('data-toolbar-control="search"');
  });

  it("renders custom toolbar dropdown triggers instead of native select controls", () => {
    const html = renderToStaticMarkup(
      <DataToolbar
        actions={<button type="button">Refresh</button>}
        ariaLabel="Asset toolbar"
        leading={
          <>
            <ToolbarMultiSelectDropdown
              allLabel="全部"
              ariaLabel="筛选"
              clearLabel="清空筛选"
              emptyLabel="暂无"
              label="筛选"
              onClear={() => undefined}
              onToggleValue={() => undefined}
              options={[{ label: "Skill", value: "skill" }]}
              selectedValues={["skill"]}
            />
            <ToolbarSingleSelectDropdown
              ariaLabel="排序"
              onChange={() => undefined}
              options={[{ label: "按创建时间", value: "created" }]}
              value="created"
            />
          </>
        }
      />,
    );

    expect(html).toContain('data-toolbar-control="dropdown"');
    expect(html).toContain("筛选(1)");
    expect(html).toContain("按创建时间");
    expect(html).not.toContain("<select");
  });
});
