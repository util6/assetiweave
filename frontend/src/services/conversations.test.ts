import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mergeConversationQuestions } from "./conversations";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

describe("conversation services", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("throws write-operation errors in the Tauri runtime", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockRejectedValueOnce(new Error("merge failed"));

    await expect(mergeConversationQuestions(["question-1", "question-2"])).rejects.toThrow("merge failed");
  });

  it("keeps fallback behavior for non-Tauri previews", async () => {
    vi.stubGlobal("window", {});
    invokeMock.mockRejectedValueOnce(new Error("preview backend missing"));

    await expect(mergeConversationQuestions(["preview-question-1", "preview-question-2"])).resolves.toMatchObject({
      dry_run: false,
      affected_question_ids: ["preview-question-1", "preview-question-2"],
    });
  });
});
