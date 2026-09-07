import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import test from "node:test";

const ROOT = path.resolve(import.meta.dirname, "..");
const SCRIPT = path.join(ROOT, "scripts", "check-agent-catalog-release.mjs");
const CATALOG = path.join(ROOT, "builtin-assets", "agent-market", "catalog-v1.json");
const BUNDLED_CATALOG = JSON.parse(fs.readFileSync(CATALOG, "utf8"));

function run(catalogPath, mode = "--static", evidencePath) {
  try {
    const args = [SCRIPT, mode, "--catalog", catalogPath];
    if (evidencePath) args.push("--evidence", evidencePath);
    return { status: 0, stdout: execFileSync(process.execPath, args, { encoding: "utf8" }) };
  } catch (error) {
    return { status: error.status ?? 1, stdout: `${error.stdout ?? ""}${error.stderr ?? ""}` };
  }
}

test("production catalog passes artifact and release-evidence gates", () => {
  const result = run(CATALOG, "--release");
  assert.equal(result.status, 0, result.stdout);
  assert.match(
    result.stdout,
    new RegExp(`${BUNDLED_CATALOG.items.length} items, catalog ${BUNDLED_CATALOG.catalogVersion}`),
  );
});

test("release gate rejects fixture and placeholder catalog data", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-agent-catalog-"));
  const fixture = path.join(directory, "catalog.json");
  const catalog = JSON.parse(fs.readFileSync(CATALOG, "utf8"));
  catalog.items[0].upstream.homepage = "https://downloads.example.com/fixture";
  fs.writeFileSync(fixture, JSON.stringify(catalog));
  const result = run(fixture);
  assert.notEqual(result.status, 0);
  assert.match(result.stdout, /fixture|example\.com|placeholder/i);
  fs.rmSync(directory, { recursive: true, force: true });
});

test("release gate treats core compatibility bounds as observational metadata", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-agent-catalog-"));
  const fixture = path.join(directory, "catalog.json");
  const catalog = JSON.parse(fs.readFileSync(CATALOG, "utf8"));
  catalog.items[0].coreCompatibility = { min: "0.5.0", maxExclusive: "0.6.0" };
  fs.writeFileSync(fixture, JSON.stringify(catalog));
  const result = run(fixture);
  assert.equal(result.status, 0, result.stdout);
  fs.rmSync(directory, { recursive: true, force: true });
});

test("release gate treats Agent versions as opaque observational metadata", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-agent-catalog-"));
  const fixture = path.join(directory, "catalog.json");
  const catalog = JSON.parse(fs.readFileSync(CATALOG, "utf8"));
  catalog.items[0].version = "release-2026.08-current";
  fs.writeFileSync(fixture, JSON.stringify(catalog));
  const result = run(fixture);
  assert.equal(result.status, 0, result.stdout);
  fs.rmSync(directory, { recursive: true, force: true });
});

test("release gate accepts an Agent item without core compatibility bounds", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-agent-catalog-"));
  const fixture = path.join(directory, "catalog.json");
  const catalog = JSON.parse(fs.readFileSync(CATALOG, "utf8"));
  delete catalog.items[0].coreCompatibility;
  fs.writeFileSync(fixture, JSON.stringify(catalog));
  const result = run(fixture);
  assert.equal(result.status, 0, result.stdout);
  fs.rmSync(directory, { recursive: true, force: true });
});

test("release evidence core version is observational metadata", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-agent-catalog-"));
  const evidenceFixture = path.join(directory, "evidence.json");
  const evidence = JSON.parse(fs.readFileSync(path.join(ROOT, "builtin-assets", "agent-market", "release-evidence-v1.json"), "utf8"));
  evidence.coreVersion = "0.1.0";
  fs.writeFileSync(evidenceFixture, JSON.stringify(evidence));
  const result = run(CATALOG, "--release", evidenceFixture);
  assert.equal(result.status, 0, result.stdout);
  fs.rmSync(directory, { recursive: true, force: true });
});

test("release gate requires a real ACP package/release E2E record", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-agent-catalog-"));
  const evidenceFixture = path.join(directory, "evidence.json");
  const evidence = JSON.parse(fs.readFileSync(path.join(ROOT, "builtin-assets", "agent-market", "release-evidence-v1.json"), "utf8"));
  delete evidence.realE2e;
  fs.writeFileSync(evidenceFixture, JSON.stringify(evidence));
  const result = run(CATALOG, "--release", evidenceFixture);
  assert.notEqual(result.status, 0);
  assert.match(result.stdout, /real ACP package\/release E2E/i);
  fs.rmSync(directory, { recursive: true, force: true });
});

test("release gate requires native availability evidence for tested Agent items", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-agent-catalog-"));
  const catalogFixture = path.join(directory, "catalog.json");
  const evidenceFixture = path.join(directory, "evidence.json");
  const catalog = JSON.parse(fs.readFileSync(CATALOG, "utf8"));
  catalog.items.push({
    id: "custom-native",
    displayName: "Custom Native",
    description: "Custom native agent",
    protocol: "native",
    version: "1.0.0",
    capabilities: { purposes: ["memory"], textPrompt: true, modelDiscovery: false, resume: false, historyReplay: false, liveEvents: false, richHistoryReplay: false },
    verification: { status: "tested", testedAt: "2026-08-20T00:00:00Z", evidenceId: "custom-native-evidence" },
    upstream: { registryId: "custom-native", homepage: "https://example.com/native", license: "MIT" },
    distributions: [{ id: "system-custom", type: "system", priority: 10, commandCandidates: ["custom"], versionArgs: ["--version"], launchArgs: [] }],
  });
  // Replace placeholder homepage for the mock
  catalog.items[catalog.items.length - 1].upstream.homepage = "https://example-native-agent.org";
  const evidence = JSON.parse(fs.readFileSync(path.join(ROOT, "builtin-assets", "agent-market", "release-evidence-v1.json"), "utf8"));
  evidence.items.push({
    evidenceId: "custom-native-evidence",
    catalogItemId: "custom-native",
    upstreamAgentId: "custom-native",
    agentVersion: "1.0.0",
    distributionId: "system-custom",
    distributionType: "system",
    install: { status: "passed", method: "system executable discovery", versionOutput: "1.0.0" },
    nativeConformance: { status: "passed", availabilityProbe: "failed" },
  });
  const catalogBytes = Buffer.from(JSON.stringify(catalog));
  evidence.catalogContentSha256 = crypto.createHash("sha256").update(catalogBytes).digest("hex");
  fs.writeFileSync(catalogFixture, catalogBytes);
  fs.writeFileSync(evidenceFixture, JSON.stringify(evidence));
  const result = run(catalogFixture, "--release", evidenceFixture);
  assert.notEqual(result.status, 0);
  assert.match(result.stdout, /tested native item requires availability evidence/i);
  fs.rmSync(directory, { recursive: true, force: true });
});

test("bundled catalog antigravity item satisfies official ACP specification across 5 platforms", () => {
  const antigravity = BUNDLED_CATALOG.items.find((item) => item.id === "antigravity");
  assert.ok(antigravity, "antigravity item must exist in catalog");
  assert.equal(antigravity.protocol, "acp");
  assert.equal(antigravity.version, "1.1.1");
  assert.equal(antigravity.upstream?.registryId, "antigravity-acp");
  assert.equal(antigravity.verification?.status, "experimental");
  assert.equal(antigravity.capabilities.textPrompt, true);
  assert.equal(antigravity.capabilities.modelDiscovery, false);
  assert.equal(antigravity.capabilities.resume, false);
  assert.equal(antigravity.capabilities.historyReplay, false);
  assert.equal(antigravity.capabilities.liveEvents, false);
  assert.equal(antigravity.capabilities.teamTools, false);

  const expectedDistributions = [
    {
      id: "binary-darwin-aarch64",
      target: { os: "darwin", arch: "aarch64" },
      archive: "zip",
      url: "https://dl.google.com/agy-extensions/releases/macos/agy-acp-server-agy_acp_server_1.1.1-darwin-arm64.zip",
      sha256: "fdfa915652cdb7ba8085cc8fffed072cbe009251aa2c951aabdda07a8c28a189",
      size: 316014828,
      executable: "agy_acp_server.par",
      launchArgs: [],
    },
    {
      id: "binary-linux-x86_64",
      target: { os: "linux", arch: "x86_64" },
      archive: "zip",
      url: "https://dl.google.com/agy-extensions/releases/linux/agy-acp-server-agy_acp_server_1.1.1-linux-x86_64.zip",
      sha256: "38f62d01b32deb0907b3d39a71ec301fd36369f6ffd1cf262d4af385177f79df",
      size: 681969407,
      executable: "agy_acp_server.par",
      launchArgs: ["--uid="],
    },
    {
      id: "binary-linux-aarch64",
      target: { os: "linux", arch: "aarch64" },
      archive: "zip",
      url: "https://dl.google.com/agy-extensions/releases/linux/agy-acp-server-agy_acp_server_1.1.1-linux-arm64.zip",
      sha256: "ed69e64b308fcb123ab54bf3277bf9cb0d651064f885ea5aab0ff520c7175398",
      size: 656572786,
      executable: "agy_acp_server.par",
      launchArgs: ["--uid="],
    },
    {
      id: "binary-windows-x86_64",
      target: { os: "windows", arch: "x86_64" },
      archive: "zip",
      url: "https://dl.google.com/agy-extensions/releases/windows/agy-acp-server-agy_acp_server_1.1.1-windows-x86_64.zip",
      sha256: "47cb50eef14f0a4655d78cfcfda869bcea7aaee5f9787e936bc2935ea612c3b8",
      size: 468238392,
      executable: "agy_acp_server.exe",
      launchArgs: [],
    },
    {
      id: "binary-windows-aarch64",
      target: { os: "windows", arch: "aarch64" },
      archive: "zip",
      url: "https://dl.google.com/agy-extensions/releases/windows/agy-acp-server-agy_acp_server_1.1.1-windows-arm64.zip",
      sha256: "35f4b1f47ba6a3fea7b0a3e30010df5ea73a64b4f0e7cf991cddc673ddfbcafc",
      size: 468521191,
      executable: "agy_acp_server.exe",
      launchArgs: [],
    },
  ];

  assert.equal(antigravity.distributions.length, 5);
  for (const expected of expectedDistributions) {
    const actual = antigravity.distributions.find((d) => d.id === expected.id);
    assert.ok(actual, `distribution ${expected.id} must exist`);
    assert.deepEqual(actual.target, expected.target);
    assert.equal(actual.archive, expected.archive);
    assert.equal(actual.url, expected.url);
    assert.equal(actual.sha256, expected.sha256);
    assert.equal(actual.size, expected.size);
    assert.equal(actual.executable, expected.executable);
    assert.deepEqual(actual.launchArgs, expected.launchArgs);
  }
});

test("release gate rejects fraudulent passed ACP conformance evidence", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-agent-catalog-"));
  const evidenceFixture = path.join(directory, "evidence.json");
  const evidence = JSON.parse(fs.readFileSync(path.join(ROOT, "builtin-assets", "agent-market", "release-evidence-v1.json"), "utf8"));
  const opencodeRecord = evidence.items.find((item) => item.catalogItemId === "opencode");
  opencodeRecord.acpConformance.sessionClose = "not_run";
  fs.writeFileSync(evidenceFixture, JSON.stringify(evidence));
  const result = run(CATALOG, "--release", evidenceFixture);
  assert.notEqual(result.status, 0);
  assert.match(result.stdout, /passed ACP conformance evidence cannot contain incomplete or non-passed steps|tested item requires complete ACP conformance/i);
  fs.rmSync(directory, { recursive: true, force: true });
});

test("release gate requires evidence to match the bundled catalog content", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-agent-catalog-"));
  const fixture = path.join(directory, "catalog.json");
  const catalog = JSON.parse(fs.readFileSync(CATALOG, "utf8"));
  catalog.items[0].distributions[0].sha256 = "1".repeat(64);
  fs.writeFileSync(fixture, JSON.stringify(catalog));
  const result = run(fixture, "--release");
  assert.notEqual(result.status, 0);
  assert.match(result.stdout, /catalogContentSha256|artifact SHA256/i);
  fs.rmSync(directory, { recursive: true, force: true });
});

test("release gate requires the published npm bin to match the catalog", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-agent-catalog-"));
  const fixture = path.join(directory, "catalog.json");
  const catalog = JSON.parse(fs.readFileSync(CATALOG, "utf8"));
  catalog.items.find((item) => item.id === "qoder").distributions[0].bin = "qoder";
  fs.writeFileSync(fixture, JSON.stringify(catalog));
  const result = run(fixture, "--release");
  assert.notEqual(result.status, 0);
  assert.match(result.stdout, /catalogContentSha256|package bin evidence/i);
  fs.rmSync(directory, { recursive: true, force: true });
});
