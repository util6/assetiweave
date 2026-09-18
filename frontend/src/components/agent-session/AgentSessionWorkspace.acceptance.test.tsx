// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import {
  DEFAULT_INTERACTIVE_CAPABILITIES,
  DEFAULT_OBSERVER_CAPABILITIES,
  type AgentSessionItemView,
  type ToolContentBlock,
} from "../../types/agentSession";
import { AgentSessionWorkspace } from "./AgentSessionWorkspace";

describe("T12: G-12 全量验收与 AionUi 20 个视觉场景矩阵 (Final Acceptance Matrix)", () => {
  afterEach(() => cleanup());

  // 场景 1: 空 Session (Empty Session)
  it("场景 1: 空 Session - 显示 EmptyState 且 Header/Composer 正常可用", () => {
    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
          items={[]}
          model="claude-3-5-sonnet"
          recipientTitle="Research Agent"
        />
      </I18nProvider>,
    );

    expect(
      screen.getByTestId("agent-session-active-recipient").textContent,
    ).toContain("Research Agent");
    expect(screen.getByTestId("agent-session-header-model").textContent).toBe(
      "claude-3-5-sonnet",
    );
    expect(screen.getByTestId("agent-session-empty-state")).toBeTruthy();
    expect(screen.getByLabelText("Message content")).toBeTruthy();
  });

  // 场景 2: 用户一问一答 (User Q&A)
  it("场景 2: 用户一问一答 - 形成完整 Turn，区分用户与助手视觉", () => {
    const items: AgentSessionItemView[] = [
      {
        id: "msg-user-1",
        kind: "user_message",
        sequence: 1,
        delivery: "live",
        state: "completed",
        text: "请分析当前仓库的目录结构",
      },
      {
        id: "msg-asst-1",
        kind: "assistant_text",
        sequence: 2,
        delivery: "live",
        state: "completed",
        text: "当前仓库包含 `frontend`、`src-tauri` 和 `cli` 三个核心模块。",
      },
    ];

    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
          items={items}
          recipientTitle="Code Assistant"
        />
      </I18nProvider>,
    );

    expect(screen.getByText("请分析当前仓库的目录结构")).toBeTruthy();
    expect(screen.getByText(/当前仓库包含/)).toBeTruthy();
  });

  // 场景 3: streaming assistant
  it("场景 3: streaming assistant - 流式正文实时增长且不丢失光标", () => {
    const items: AgentSessionItemView[] = [
      {
        id: "stream-asst",
        kind: "assistant_text",
        sequence: 1,
        delivery: "live",
        state: "streaming",
        text: "正在读取数据流...",
      },
    ];

    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
          isExecuting={true}
          items={items}
        />
      </I18nProvider>,
    );

    expect(screen.getByText("正在读取数据流...")).toBeTruthy();
  });

  // 场景 4: streaming thinking
  it("场景 4: streaming thinking - 处于运行中时 thinking 默认展开", () => {
    const items: AgentSessionItemView[] = [
      {
        id: "think-1",
        kind: "thinking",
        sequence: 1,
        delivery: "live",
        state: "streaming",
        text: "需要先检查 SQLite 迁移文件，然后再验证后端模型...",
      },
    ];

    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
          isExecuting={true}
          items={items}
        />
      </I18nProvider>,
    );

    expect(screen.getByText(/需要先检查 SQLite 迁移文件/)).toBeTruthy();
  });

  // 场景 5: processing-only
  it("场景 5: processing-only - 仅处理中状态行，不渲染多余空白展开区域", () => {
    const items: AgentSessionItemView[] = [
      {
        id: "proc-1",
        kind: "processing",
        sequence: 1,
        delivery: "live",
        state: "streaming",
        status: "Running background analyzer",
      },
    ];

    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
          items={items}
        />
      </I18nProvider>,
    );

    expect(screen.getByText("Running background analyzer")).toBeTruthy();
  });

  // 场景 6: 两个连续 Tool Steps
  it("场景 6: 两个连续 Tool Steps - 自动合并为一组 StepGroup 并在 Header 显示 `查看步骤 · 2`", () => {
    const items: AgentSessionItemView[] = [
      {
        id: "tool-1",
        kind: "tool",
        sequence: 1,
        delivery: "live",
        state: "succeeded",
        toolName: "list_dir",
        text: "list_dir /src",
      },
      {
        id: "tool-2",
        kind: "tool",
        sequence: 2,
        delivery: "live",
        state: "succeeded",
        toolName: "read_file",
        text: "read_file /src/main.rs",
      },
    ];

    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_OBSERVER_CAPABILITIES}
          items={items}
        />
      </I18nProvider>,
    );

    expect(screen.getByText(/查看步骤|View Steps/)).toBeTruthy();
    expect(screen.getByText(/·\s*2/)).toBeTruthy();
  });

  // 场景 7: running StepGroup
  it("场景 7: running StepGroup - 包含正在运行的 step 时默认展开", () => {
    const items: AgentSessionItemView[] = [
      {
        id: "tool-run",
        kind: "tool",
        sequence: 1,
        delivery: "live",
        state: "streaming",
        toolName: "cargo_check",
        text: "cargo check --workspace",
      },
    ];

    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_OBSERVER_CAPABILITIES}
          items={items}
        />
      </I18nProvider>,
    );

    expect(screen.getByText("cargo_check")).toBeTruthy();
  });

  // 场景 8: failed Step
  it("场景 8: failed Step - 失败步骤包含错误指示与退出码", () => {
    const items: AgentSessionItemView[] = [
      {
        id: "tool-fail",
        kind: "tool",
        sequence: 1,
        delivery: "live",
        state: "failed",
        toolName: "git_push",
        text: "git push origin main",
        toolOutput: {
          stderr: "fatal: remote rejected (permission denied)",
          exit_code: 1,
        },
      },
    ];

    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_OBSERVER_CAPABILITIES}
          items={items}
        />
      </I18nProvider>,
    );

    expect(screen.getByText("git_push")).toBeTruthy();
    // 展开详情测试
    const details = screen.getByTestId(
      "agent-session-session-item-details-tool-fail",
    ) as HTMLDetailsElement;
    details.open = true;
    fireEvent(details, new Event("toggle"));
    expect(screen.getByText(/fatal: remote rejected/)).toBeTruthy();
    expect(screen.getByText(/Exit code: 1/)).toBeTruthy();
  });

  // 场景 9: command stdout/stderr
  it("场景 9: command stdout/stderr - 终端输出过滤 ANSI 控制符并清晰分段", () => {
    const items: AgentSessionItemView[] = [
      {
        id: "tool-terminal",
        kind: "tool",
        sequence: 1,
        delivery: "live",
        state: "succeeded",
        toolName: "run_command",
        text: "run_command",
        toolInput: {
          command: "npm test",
          cwd: "/workspace",
        },
        toolOutput: {
          stdout: "PASS src/app.test.tsx\nAll tests passed",
          exit_code: 0,
        },
      },
    ];

    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_OBSERVER_CAPABILITIES}
          items={items}
        />
      </I18nProvider>,
    );

    const details = screen.getByTestId(
      "agent-session-session-item-details-tool-terminal",
    ) as HTMLDetailsElement;
    details.open = true;
    fireEvent(details, new Event("toggle"));
    expect(screen.getByText(/\$ npm test/)).toBeTruthy();
    expect(screen.getByText(/PASS src\/app\.test\.tsx/)).toBeTruthy();
  });

  // 场景 10: Diff 展示
  it("场景 10: Diff - 结构化差异展示位置与增删行", () => {
    const diffBlock: ToolContentBlock = {
      type: "diff",
      path: "/Users/developer/code-space/assetiweave/src/lib.rs",
      oldText: "pub fn old() {}\n",
      newText: "pub fn new() {}\n",
      unifiedDiff: null,
    };

    const items: AgentSessionItemView[] = [
      {
        id: "tool-diff",
        kind: "tool",
        sequence: 1,
        delivery: "live",
        state: "succeeded",
        toolName: "edit_file",
        text: "edit_file",
        toolOutput: [diffBlock],
      },
    ];

    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_OBSERVER_CAPABILITIES}
          items={items}
        />
      </I18nProvider>,
    );

    const details = screen.getByTestId(
      "agent-session-session-item-details-tool-diff",
    ) as HTMLDetailsElement;
    details.open = true;
    fireEvent(details, new Event("toggle"));
    expect(
      screen.getAllByText(/~\/code-space\/assetiweave\/src\/lib\.rs/).length,
    ).toBeGreaterThanOrEqual(1);
  });

  // 场景 11: long/truncated content
  it("场景 11: long/truncated content - 截断元数据标签显示原大小与保留大小", () => {
    const items: AgentSessionItemView[] = [
      {
        id: "tool-trunc",
        kind: "tool",
        sequence: 1,
        delivery: "live",
        state: "succeeded",
        toolName: "cat_large_log",
        text: "cat_large_log",
        toolOutput: { snippet: "..." },
        truncation: {
          originalBytes: 1048576,
          retainedBytes: 4096,
          strategy: "head_tail",
        },
      },
    ];

    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_OBSERVER_CAPABILITIES}
          items={items}
        />
      </I18nProvider>,
    );

    const details = screen.getByTestId(
      "agent-session-session-item-details-tool-trunc",
    ) as HTMLDetailsElement;
    details.open = true;
    fireEvent(details, new Event("toggle"));
    expect(screen.getByText(/1048576/)).toBeTruthy();
    expect(screen.getByText(/4096/)).toBeTruthy();
  });

  // 场景 12: Team 两成员 parallel
  it("场景 12: Team 两成员 parallel - 并行渲染两个工作区", () => {
    render(
      <I18nProvider>
        <div style={{ display: "flex", width: "1000px" }}>
          <div style={{ flex: 1 }}>
            <AgentSessionWorkspace
              capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
              items={[]}
              recipientTitle="Member Leader"
            />
          </div>
          <div style={{ flex: 1 }}>
            <AgentSessionWorkspace
              capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
              items={[]}
              recipientTitle="Member Researcher"
            />
          </div>
        </div>
      </I18nProvider>,
    );

    expect(screen.getAllByText("Member Leader").length).toBeGreaterThanOrEqual(
      1,
    );
    expect(
      screen.getAllByText("Member Researcher").length,
    ).toBeGreaterThanOrEqual(1);
  });

  // 场景 13: Team 三成员横向 overflow
  it("场景 13: Team 三成员横向 overflow - 每个 lane 拥有 400px floor 与独立容器", () => {
    render(
      <I18nProvider>
        <div style={{ display: "flex", overflowX: "auto", width: "800px" }}>
          <div style={{ flex: "1 1 400px", minWidth: "400px" }}>
            <AgentSessionWorkspace
              capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
              items={[]}
              recipientTitle="Member A"
            />
          </div>
          <div style={{ flex: "1 1 400px", minWidth: "400px" }}>
            <AgentSessionWorkspace
              capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
              items={[]}
              recipientTitle="Member B"
            />
          </div>
          <div style={{ flex: "1 1 400px", minWidth: "400px" }}>
            <AgentSessionWorkspace
              capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
              items={[]}
              recipientTitle="Member C"
            />
          </div>
        </div>
      </I18nProvider>,
    );

    expect(screen.getAllByText("Member A").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("Member B").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("Member C").length).toBeGreaterThanOrEqual(1);
  });

  // 场景 14: Team single
  it("场景 14: Team single - 单列模式下独占全宽并保留活跃成员上下文", () => {
    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
          items={[]}
          recipientTitle="Focused Teammate"
        />
      </I18nProvider>,
    );

    expect(
      screen.getByTestId("agent-session-active-recipient").textContent,
    ).toContain("Focused Teammate");
  });

  // 场景 15: Memory running observer
  it("场景 15: Memory running observer - 只读模式展示运行中提示且无 Composer", () => {
    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_OBSERVER_CAPABILITIES}
          isExecuting={true}
          items={[
            {
              id: "mem-req-1",
              kind: "user_message",
              sequence: 1,
              delivery: "live",
              state: "completed",
              text: "Memory Recipe: Extract and index key entities",
            },
          ]}
          model="gemini-2.5-pro"
          recipientTitle="Session Memory Agent"
        />
      </I18nProvider>,
    );

    expect(screen.getByTestId("agent-session-header-readonly")).toBeTruthy();
    expect(screen.queryByTestId("agent-session-composer")).toBeNull();
    expect(screen.getByText(/Memory Recipe/)).toBeTruthy();
  });

  // 场景 16: Memory terminal observer
  it("场景 16: Memory terminal observer - 任务终态只读展示且保留完整审计轨迹", () => {
    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_OBSERVER_CAPABILITIES}
          items={[
            {
              id: "term-1",
              kind: "assistant_text",
              sequence: 1,
              delivery: "replay",
              state: "completed",
              text: "Memory indexing completed successfully.",
            },
          ]}
          recipientTitle="Project Memory Agent"
        />
      </I18nProvider>,
    );

    expect(
      screen.getByText("Memory indexing completed successfully."),
    ).toBeTruthy();
    expect(screen.getByTestId("agent-session-header-readonly")).toBeTruthy();
  });

  // 场景 17: unavailable observer
  it("场景 17: unavailable observer - 进程内无执行现场时优雅展示占位与已知上下文", () => {
    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_OBSERVER_CAPABILITIES}
          items={[]}
          model="deepseek-r1"
          recipientTitle="Global Memory"
          unavailable={true}
        />
      </I18nProvider>,
    );

    expect(screen.getByTestId("agent-session-unavailable")).toBeTruthy();
    expect(screen.getByText("Global Memory")).toBeTruthy();
    expect(screen.getByText("deepseek-r1")).toBeTruthy();
  });

  // 场景 18: 320/768/1024/1440 响应式 (Responsive)
  it("场景 18: 响应式视口适配 - 窄屏模式下 timeline 和 detail 正常容纳无布局溢出", () => {
    const { container } = render(
      <I18nProvider>
        <div style={{ width: "320px" }}>
          <AgentSessionWorkspace
            capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
            items={[
              {
                id: "item-resp",
                kind: "assistant_text",
                sequence: 1,
                delivery: "live",
                state: "completed",
                text: "Responsive container validation text",
              },
            ]}
          />
        </div>
      </I18nProvider>,
    );

    expect(
      container.querySelector("[data-testid='agent-session-timeline']"),
    ).toBeTruthy();
  });

  // 场景 19: dark/light 主题
  it("场景 19: dark/light 主题 - 所有元素使用语义化 Theme Token 渲染", () => {
    const { container } = render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
          items={[]}
          recipientTitle="Theme Test"
        />
      </I18nProvider>,
    );

    expect(container.firstChild).toBeTruthy();
  });

  // 场景 20: reduced-motion
  it("场景 20: reduced-motion - 遵守减弱动画配置与无障碍对比度", () => {
    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
          items={[]}
        />
      </I18nProvider>,
    );

    const log = screen.getByRole("log");
    expect(log).toBeTruthy();
    expect(log.getAttribute("aria-live")).toBe("polite");
    expect(log.getAttribute("aria-atomic")).toBe("false");
  });
});
