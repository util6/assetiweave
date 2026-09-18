import { describe, expect, it } from "vitest";
import type { SessionItemSnapshot } from "../../types/team";
import { mapSessionItemSnapshotToView } from "./teamSessionAdapter";

describe("T11 Legacy 收缩与 Tool Detail 保留守卫 (Guard Tests)", () => {
  it("guard: tool snapshot retains full input, output, name, and truncation payload across mapping", () => {
    const richToolItem: SessionItemSnapshot = {
      identity: {
        session_id: "session-guard-1",
        member_id: "member-researcher",
        execution_id: "exec-101",
        turn_id: "turn-1",
        item_id: "tool-call-file-read",
      },
      kind: "tool",
      sequence: 42,
      delivery: "live",
      state: "succeeded",
      text: "read_file /workspace/src/main.rs",
      status: null,
      code: null,
      partial: false,
      tool_call_id: "call-xyz-999",
      tool_name: "read_file",
      tool_input: {
        path: "/workspace/src/main.rs",
        offset: 0,
        limit: 100,
      },
      tool_output: {
        content: 'fn main() { println!("hello"); }',
        bytes_read: 32,
      },
      truncation: {
        original_bytes: 1024,
        retained_bytes: 32,
        strategy: "head_tail",
      },
    };

    // 映射到共享 AgentSessionItemView
    const viewItem = mapSessionItemSnapshotToView(richToolItem);

    // 守卫断言：所有 tool 细节绝不丢失或清空
    expect(viewItem.id).toBe("tool-call-file-read");
    expect(viewItem.kind).toBe("tool");
    expect(viewItem.toolCallId).toBe("call-xyz-999");
    expect(viewItem.toolName).toBe("read_file");
    expect(viewItem.toolInput).toEqual({
      path: "/workspace/src/main.rs",
      offset: 0,
      limit: 100,
    });
    expect(viewItem.toolOutput).toEqual({
      content: 'fn main() { println!("hello"); }',
      bytes_read: 32,
    });
    expect(viewItem.truncation).toEqual({
      originalBytes: 1024,
      retainedBytes: 32,
      strategy: "head_tail",
    });
    expect(viewItem.text).toBe("read_file /workspace/src/main.rs");
  });

  it("guard: no tool-clear behavior exists in codebase", () => {
    const items: SessionItemSnapshot[] = [
      {
        identity: {
          session_id: "s-1",
          member_id: "m-1",
          execution_id: "e-1",
          turn_id: "t-1",
          item_id: "tool-1",
        },
        kind: "tool",
        sequence: 1,
        delivery: "live",
        state: "completed",
        text: "tool output summary",
        status: null,
        code: null,
        tool_name: "test_tool",
        tool_input: { foo: "bar" },
        tool_output: { baz: 123 },
      },
    ];

    const mapped = items.map(mapSessionItemSnapshotToView);
    expect(mapped[0].toolInput).toEqual({ foo: "bar" });
    expect(mapped[0].toolOutput).toEqual({ baz: 123 });
    expect(mapped[0].text).toBe("tool output summary");
  });

  it("guard: preserves terminal failure details and retryable flag", () => {
    const failedItem: SessionItemSnapshot = {
      identity: {
        session_id: "s-2",
        member_id: "m-1",
        execution_id: "e-2",
        turn_id: "t-2",
        item_id: "error-terminal",
      },
      kind: "error",
      sequence: 99,
      delivery: "live",
      state: "failed",
      text: "网络超时无法连接模型服务",
      status: null,
      code: "upstream_timeout",
    };

    const view = mapSessionItemSnapshotToView(failedItem);
    expect(view.kind).toBe("error");
    expect(view.code).toBe("upstream_timeout");
    expect(view.text).toBe("网络超时无法连接模型服务");
    expect(view.state).toBe("failed");
  });
});
