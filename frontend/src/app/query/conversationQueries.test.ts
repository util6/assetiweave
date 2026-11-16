import { describe, expect, it, vi } from "vitest";
import {
  conversationAdaptersQueryOptions,
  conversationKeys,
  conversationSessionDetailQueryOptions,
  isWebRecordAdapter,
} from "./conversationQueries";
import * as conversationService from "../../services/conversations";
import type {
  ConversationAdapter,
  ConversationSessionDetail,
} from "../../types";

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

  it("会话详情缓存按作用域、记录类型与会话标识隔离", () => {
    const a = { tenantId: "tenant-a", epoch: 1 };
    const b = { tenantId: "tenant-b", epoch: 1 };
    expect(conversationKeys.detail(a, "session", "s-1")).not.toEqual(
      conversationKeys.detail(b, "session", "s-1"),
    );
    expect(conversationKeys.detail(a, "session", "s-1")).not.toEqual(
      conversationKeys.detail(a, "session", "s-2"),
    );
    expect(conversationKeys.detail(a, "session", "s-1")).not.toEqual(
      conversationKeys.detail(a, "web", "s-1"),
    );
  });

  it("conversationSessionDetailQueryOptions 按记录类型调用对应详情服务", async () => {
    const detail = {
      questions: [],
      session: { id: "s-1" },
    } as unknown as ConversationSessionDetail;
    const getSessionSpy = vi
      .spyOn(conversationService, "getConversationSession")
      .mockResolvedValue(detail);
    const getWebSessionSpy = vi
      .spyOn(conversationService, "getWebRecordSession")
      .mockResolvedValue(detail);
    const scope = { tenantId: "default", epoch: 1 };

    const sessionOptions = conversationSessionDetailQueryOptions(
      scope,
      "session",
      "s-1",
    );
    expect(sessionOptions.queryKey).toEqual(
      conversationKeys.detail(scope, "session", "s-1"),
    );
    await expect((sessionOptions.queryFn as () => unknown)()).resolves.toBe(
      detail,
    );
    expect(getSessionSpy).toHaveBeenCalledWith("s-1");
    expect(getWebSessionSpy).not.toHaveBeenCalled();

    const webOptions = conversationSessionDetailQueryOptions(
      scope,
      "web",
      "s-1",
    );
    await expect((webOptions.queryFn as () => unknown)()).resolves.toBe(detail);
    expect(getWebSessionSpy).toHaveBeenCalledWith("s-1");
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
