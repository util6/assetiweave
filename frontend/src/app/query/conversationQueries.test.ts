import { describe, expect, it } from "vitest";
import { conversationKeys } from "./conversationQueries";

describe("conversationQueries", () => {
  it("会话缓存按租户、记录类型与搜索隔离", () => {
    const a = { tenantId: "tenant-a", epoch: 1 };
    const b = { tenantId: "tenant-b", epoch: 1 };
    expect(conversationKeys.sessions(a, "session", "alpha")).not.toEqual(
      conversationKeys.sessions(b, "session", "alpha"),
    );
    expect(conversationKeys.sessions(a, "session", "alpha")).not.toEqual(
      conversationKeys.sessions(a, "session", "beta"),
    );
    expect(conversationKeys.sessions(a, "session", "alpha")).not.toEqual(
      conversationKeys.sessions(a, "web", "alpha"),
    );
  });
});
