import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  __resetConversationCommandProjectionCacheForTests,
  projectConversationCommandParts,
} from "./conversationCommandProjection";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

describe("conversation command projection service", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    __resetConversationCommandProjectionCacheForTests();
  });

  it("projects one bounded batch and reuses its runtime-only cache", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce([{
      part_id: "part-shell",
      schema_version: 1,
      projector_version: "shell-projector-v1",
      nodes: [
        { display_order: 0, command: "git status --short", command_label: "status" },
        { display_order: 1, command: "git diff" },
      ],
    }]);
    const request = {
      adapterId: "codex",
      adapterVersion: "1.6.1",
      parts: [{
        partId: "part-shell",
        command: "printf '%s\\n' '--- status ---'; git status --short; git diff",
      }],
    };

    const first = await projectConversationCommandParts(request);
    const second = await projectConversationCommandParts(request);

    expect(first).toEqual(second);
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("project_conversation_command_parts", {
      params: {
        adapter_id: "codex",
        parts: [{
          part_id: "part-shell",
          command: request.parts[0]!.command,
          command_label: null,
        }],
      },
    });
  });

  it("uses the same raw-node contract in browser preview without copying the shell parser", async () => {
    vi.stubGlobal("window", {});

    await expect(projectConversationCommandParts({
      adapterId: "codex",
      adapterVersion: "1.6.1",
      parts: [{ partId: "preview-part", command: "printf divider && git status" }],
    })).resolves.toEqual([{
      part_id: "preview-part",
      schema_version: 1,
      projector_version: "browser-preview-raw-v1",
      nodes: [{ display_order: 0, command: "printf divider && git status", command_label: null }],
    }]);
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
