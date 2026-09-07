import { describe, expect, it } from "vitest";
import {
  agentCatalog,
  marketItemToCatalogItem,
} from "./agentCatalog";
import type { AgentMarketItem } from "../../services/agentRuntime";

describe("agentCatalog presentation and projection", () => {
  it("legacy presentation presents antigravity as ACP Agent", () => {
    const antigravity = agentCatalog.find((agent) => agent.id === "antigravity");
    expect(antigravity).toBeDefined();
    expect(antigravity?.protocol).toBe("ACP Agent");
    expect(antigravity?.connectionMode).toBe("registry");
  });

  it("projects official antigravity ACP market item to catalog item", () => {
    const marketItem: AgentMarketItem = {
      id: "antigravity",
      displayName: "Google Antigravity",
      description: "Google Antigravity official ACP Server",
      protocol: "acp",
      version: "1.1.1",
      capabilities: {
        purposes: ["card_translation", "memory"],
        textPrompt: true,
        modelDiscovery: false,
        resume: false,
        historyReplay: false,
        teamTools: false,
        liveEvents: false,
        richHistoryReplay: false,
      },
      verification: {
        status: "experimental",
        testedAt: "2026-09-07T00:00:00Z",
        evidenceId: "acp-registry-81bf71b5-antigravity-1.1.1-binary-darwin-aarch64",
      },
      catalogVersion: "2026.08.29.1",
      recommendedDistributionId: "binary-darwin-aarch64",
      distributions: [
        {
          distributionId: "binary-darwin-aarch64",
          distributionType: "binary",
          selectable: true,
          recommended: true,
          ownership: "managed",
          reasonCode: null,
          requiredRuntime: null,
          resolvedVersion: "1.1.1",
          downloadSize: 316014828,
          targetPath: "/mock/bin/agy_acp_server.par",
        },
      ],
      installed: null,
      updateAvailable: false,
      installability: "installable",
    };

    const item = marketItemToCatalogItem(marketItem);
    expect(item.id).toBe("antigravity");
    expect(item.name).toBe("Google Antigravity");
    expect(item.protocol).toBe("ACP");
    expect(item.command).toBe("/mock/bin/agy_acp_server.par");
  });
});
