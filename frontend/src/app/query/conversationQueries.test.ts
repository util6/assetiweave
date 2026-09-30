import { describe, expect, it, vi } from "vitest";
import {
  conversationAdaptersQueryOptions,
  conversationKeys,
  isWebRecordAdapter,
} from "./conversationQueries";
import * as conversationService from "../../services/conversations";
import type { ConversationAdapter } from "../../types";

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

  it("isWebRecordAdapter 正确识别网页记录适配器", () => {
    expect(
      isWebRecordAdapter({
        id: "chatgpt-web",
        capabilities: ["web_records", "read_session"],
      }),
    ).toBe(true);
    expect(
      isWebRecordAdapter({
        id: "custom-site-web",
        capabilities: ["read_session"],
      }),
    ).toBe(true);
    expect(
      isWebRecordAdapter({ id: "antigravity", capabilities: ["read_session"] }),
    ).toBe(false);
  });

  it("conversationAdaptersQueryOptions 根据 recordKind 隔离会话与网页记录", async () => {
    const mockAdapters: ConversationAdapter[] = [
      {
        id: "antigravity",
        name: "Antigravity",
        capabilities: ["read_session"],
        created_at: "2026-01-01",
        enabled: true,
        input_kinds: ["directory"],
        kind: "external",
        protocol_version: 1,
        trust_state: "trusted",
        updated_at: "2026-01-01",
        version: "1.0",
      },
      {
        id: "chatgpt-web",
        name: "ChatGPT Web",
        capabilities: ["read_session", "web_records"],
        created_at: "2026-01-01",
        enabled: true,
        input_kinds: ["directory"],
        kind: "external",
        protocol_version: 1,
        trust_state: "trusted",
        updated_at: "2026-01-01",
        version: "1.0",
      },
      {
        id: "gemini-web",
        name: "Gemini Web",
        capabilities: ["read_session", "web_records"],
        created_at: "2026-01-01",
        enabled: true,
        input_kinds: ["directory"],
        kind: "external",
        protocol_version: 1,
        trust_state: "trusted",
        updated_at: "2026-01-01",
        version: "1.0",
      },
      {
        id: "claude-code",
        name: "Claude Code",
        capabilities: ["read_session"],
        created_at: "2026-01-01",
        enabled: true,
        input_kinds: ["directory"],
        kind: "external",
        protocol_version: 1,
        trust_state: "trusted",
        updated_at: "2026-01-01",
        version: "1.0",
      },
    ];

    vi.spyOn(conversationService, "listConversationAdapters").mockResolvedValue(
      mockAdapters,
    );

    const scope = { tenantId: "default", epoch: 1 };

    const sessionOptions = conversationAdaptersQueryOptions(scope, "session");
    const sessionResult = await (sessionOptions.queryFn as any)();
    expect(sessionResult.map((a: ConversationAdapter) => a.id)).toEqual([
      "antigravity",
      "claude-code",
    ]);

    const webOptions = conversationAdaptersQueryOptions(scope, "web");
    const webResult = await (webOptions.queryFn as any)();
    expect(webResult.map((a: ConversationAdapter) => a.id)).toEqual([
      "chatgpt-web",
      "gemini-web",
    ]);
  });
});
