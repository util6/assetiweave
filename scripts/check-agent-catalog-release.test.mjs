import assert from "node:assert/strict";
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
  const evidenceFixture = path.join(directory, "evidence.json");
  const evidence = JSON.parse(fs.readFileSync(path.join(ROOT, "builtin-assets", "agent-market", "release-evidence-v1.json"), "utf8"));
  delete evidence.items.find((item) => item.catalogItemId === "antigravity").nativeConformance;
  fs.writeFileSync(evidenceFixture, JSON.stringify(evidence));
  const result = run(CATALOG, "--release", evidenceFixture);
  assert.notEqual(result.status, 0);
  assert.match(result.stdout, /tested native item requires availability evidence/i);
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
