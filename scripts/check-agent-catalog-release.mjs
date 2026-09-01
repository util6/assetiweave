#!/usr/bin/env node

import crypto from "node:crypto";
import { execFileSync, spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import readline from "node:readline";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const DEFAULT_CATALOG = path.join(ROOT, "builtin-assets", "agent-market", "catalog-v1.json");
const DEFAULT_EVIDENCE = path.join(ROOT, "builtin-assets", "agent-market", "release-evidence-v1.json");
const args = new Set(process.argv.slice(2));
const catalogFlag = process.argv.indexOf("--catalog");
const evidenceFlag = process.argv.indexOf("--evidence");
const catalogPath = catalogFlag >= 0 ? path.resolve(process.argv[catalogFlag + 1]) : DEFAULT_CATALOG;
const evidencePath = evidenceFlag >= 0 ? path.resolve(process.argv[evidenceFlag + 1]) : DEFAULT_EVIDENCE;
const release = args.has("--release");
const network = args.has("--network");
const e2e = args.has("--e2e");
const MAX_NETWORK_BYTES = 256 * 1024 * 1024;
const NETWORK_TIMEOUT_MS = 120_000;

function parseVersion(value) {
  const match = /^(\d+)\.(\d+)\.(\d+)(?:[-+].*)?$/.exec(String(value));
  return match ? [Number(match[1]), Number(match[2]), Number(match[3])] : null;
}

function sha256(bytes) {
  return crypto.createHash("sha256").update(bytes).digest("hex");
}

function containsPlaceholder(value) {
  const text = JSON.stringify(value).toLowerCase();
  return /fixture|placeholder|example\.com|downloads\.example|localhost|127\.0\.0\.1|a{64}|0{64}/.test(text);
}

function checkCatalog(catalog) {
  const errors = [];
  if (catalog?.schema !== "assetiweave.agent-market/v1") errors.push("schema must be assetiweave.agent-market/v1");
  if (!/^\d{4}\.\d{2}\.\d{2}\.\d+$/.test(catalog?.catalogVersion ?? "")) errors.push("catalogVersion must be a fixed YYYY.MM.DD.N revision");
  if (!catalog?.generatedAt || Number.isNaN(Date.parse(catalog.generatedAt))) errors.push("generatedAt must be an RFC3339 date");
  if (catalog?.source?.upstream !== "agentclientprotocol/registry") errors.push("source.upstream must be agentclientprotocol/registry");
  if (!catalog?.source?.upstreamRevision || /fixture|placeholder|latest/i.test(catalog.source.upstreamRevision)) errors.push("source.upstreamRevision must identify a fixed upstream snapshot");
  if (!Array.isArray(catalog?.items) || catalog.items.length === 0) errors.push("catalog must contain at least one item");
  if (containsPlaceholder(catalog)) errors.push("catalog contains fixture, example.com, localhost, or placeholder SHA data");

  const ids = new Set();
  for (const item of catalog?.items ?? []) {
    if (!item?.id || ids.has(item.id)) errors.push(`duplicate or empty item id: ${item?.id ?? "<empty>"}`);
    ids.add(item?.id);
    if (typeof item?.version !== "string" || item.version.trim() === "" || item.version.length > 120 || item.version.includes("\0")) {
      errors.push(`${item?.id}: observed version must be a non-empty bounded string`);
    }
    if (!item?.verification || !["tested", "experimental"].includes(item.verification.status)) {
      errors.push(`${item?.id}: verification status must be tested or experimental`);
    }
    if (item?.verification?.status === "tested" && !item?.verification?.evidenceId) {
      errors.push(`${item?.id}: tested item needs evidenceId`);
    }
    if (!Array.isArray(item?.distributions) || item.distributions.length === 0) errors.push(`${item?.id}: no distributions`);
    for (const distribution of item?.distributions ?? []) {
      if (!distribution?.id) errors.push(`${item?.id}: distribution id is required`);
      if (distribution.type === "binary") {
        if (!/^https:\/\//.test(distribution.url ?? "")) errors.push(`${item.id}/${distribution.id}: binary URL must be HTTPS`);
        if (!/^[a-f0-9]{64}$/i.test(distribution.sha256 ?? "")) errors.push(`${item.id}/${distribution.id}: binary SHA256 must be 64 hex characters`);
        if (!Number.isInteger(distribution.size) || distribution.size <= 0) errors.push(`${item.id}/${distribution.id}: binary size must be a positive integer`);
        if (!["none", "zip", "tar.gz", "tgz", "tar.bz2", "tbz2"].includes(distribution.archive)) {
          errors.push(`${item.id}/${distribution.id}: unsupported archive format`);
        }
      }
      if (distribution.type === "npx") {
        if (!distribution.package || !parseVersion(distribution.version) || !distribution.bin) errors.push(`${item.id}/${distribution.id}: incomplete npm distribution`);
        if (JSON.stringify(distribution).toLowerCase().includes("latest")) errors.push(`${item.id}/${distribution.id}: npm distribution must not use latest`);
      }
      if (distribution.type === "uvx" && (!distribution.package || !parseVersion(distribution.version) || !distribution.command)) {
        errors.push(`${item.id}/${distribution.id}: incomplete uvx distribution`);
      }
    }
  }
  return errors;
}

function checkEvidence(catalog, catalogBytes, evidence) {
  const errors = [];
  if (!evidence || evidence.schema !== "assetiweave.agent-market/release-evidence/v1") {
    errors.push("release evidence schema is missing or unsupported");
    return errors;
  }
  if (evidence.catalogVersion !== catalog.catalogVersion) errors.push("evidence catalogVersion does not match catalog");
  if (evidence.catalogContentSha256 !== sha256(catalogBytes)) errors.push("evidence catalogContentSha256 does not match bundled catalog");
  if (evidence.upstream?.name !== catalog.source?.upstream) errors.push("evidence upstream name does not match catalog");
  if (evidence.upstream?.revision !== catalog.source?.upstreamRevision) errors.push("evidence upstream revision does not match catalog");
  if (!Array.isArray(evidence.items) || evidence.items.length === 0) {
    errors.push("release evidence must contain distribution records");
    return errors;
  }

  const evidenceById = new Map();
  for (const record of evidence.items) {
    if (!record.evidenceId || evidenceById.has(record.evidenceId)) errors.push(`duplicate or empty evidenceId: ${record.evidenceId ?? "<empty>"}`);
    evidenceById.set(record.evidenceId, record);
    if (containsPlaceholder(record)) errors.push(`${record.evidenceId}: evidence contains placeholder data`);
  }

  let passedManagedConformance = false;
  for (const item of catalog.items) {
    const itemRecords = evidence.items.filter((record) => record.catalogItemId === item.id);
    if (itemRecords.length !== item.distributions.length) errors.push(`${item.id}: evidence must cover every distribution`);
    if (item.verification?.evidenceId && !evidenceById.has(item.verification.evidenceId)) errors.push(`${item.id}: verification.evidenceId is not present in release evidence`);
    for (const distribution of item.distributions) {
      const record = itemRecords.find((candidate) => candidate.distributionId === distribution.id);
      if (!record) {
        errors.push(`${item.id}/${distribution.id}: missing evidence record`);
        continue;
      }
      if (record.install?.status !== "passed") errors.push(`${item.id}/${distribution.id}: install evidence must pass`);
      if (record.distributionType !== distribution.type) errors.push(`${item.id}/${distribution.id}: evidence distribution type does not match catalog`);
      if (distribution.type === "binary") {
        if (record.artifact?.url !== distribution.url) errors.push(`${item.id}/${distribution.id}: artifact URL does not match catalog`);
        if (record.artifact?.sha256 !== distribution.sha256) errors.push(`${item.id}/${distribution.id}: artifact SHA256 does not match catalog`);
        if (!Number.isInteger(record.artifact?.sizeBytes) || record.artifact.sizeBytes <= 0) errors.push(`${item.id}/${distribution.id}: artifact size evidence is missing`);
      }
      if (distribution.type === "npx") {
        if (record.package !== distribution.package || record.agentVersion !== distribution.version) errors.push(`${item.id}/${distribution.id}: package evidence does not match catalog`);
        if (record.bin !== distribution.bin) errors.push(`${item.id}/${distribution.id}: package bin evidence does not match catalog`);
        if (!record.packageIntegrity) errors.push(`${item.id}/${distribution.id}: package integrity evidence is missing`);
      }
      if (item.protocol === "native") {
        const nativeConformancePassed = record.nativeConformance?.status === "passed"
          && record.nativeConformance?.availabilityProbe === "passed";
        if (item.verification?.status === "tested" && !nativeConformancePassed) {
          errors.push(`${item.id}/${distribution.id}: tested native item requires availability evidence`);
        }
      } else {
        const conformance = record.acpConformance;
        const conformancePassed = conformance?.status === "passed"
          && ["initialize", "sessionNew", "sessionClose", "cleanShutdown"].every((step) => conformance[step] === "passed");
        if (item.verification?.status === "tested" && !conformancePassed) errors.push(`${item.id}/${distribution.id}: tested item requires complete ACP conformance evidence`);
        if (conformancePassed && ["binary", "npx", "uvx"].includes(distribution.type)) passedManagedConformance = true;
      }
    }
  }
  if (!passedManagedConformance) errors.push("release evidence must contain at least one passed managed ACP conformance");
  const realE2e = evidence.realE2e;
  if (realE2e?.status !== "passed") {
    errors.push("release evidence must contain a passed real ACP package/release E2E record");
  } else {
    const record = evidence.items.find(
      (candidate) =>
        candidate.catalogItemId === realE2e.catalogItemId &&
        candidate.distributionId === realE2e.distributionId,
    );
    const distribution = catalog.items
      .find((item) => item.id === realE2e.catalogItemId)
      ?.distributions.find((candidate) => candidate.id === realE2e.distributionId);
    if (!record || !distribution) {
      errors.push("real ACP E2E evidence must identify a catalog distribution");
    } else {
      if (distribution.type !== "binary") errors.push("real ACP E2E evidence must use a managed binary distribution");
      if (realE2e.artifact?.url !== distribution.url) errors.push("real ACP E2E artifact URL does not match catalog");
      if (realE2e.artifact?.sha256 !== distribution.sha256) errors.push("real ACP E2E artifact SHA256 does not match catalog");
      if (realE2e.artifact?.sizeBytes !== record.artifact?.sizeBytes) errors.push("real ACP E2E artifact size does not match release evidence");
      if (realE2e.install?.status !== "passed") errors.push("real ACP E2E install evidence must pass");
      if (realE2e.acpConformance?.status !== "passed") errors.push("real ACP E2E conformance evidence must pass");
      for (const step of ["initialize", "sessionNew", "sessionClose", "cleanShutdown"]) {
        if (realE2e.acpConformance?.[step] !== "passed") errors.push(`real ACP E2E conformance step ${step} must pass`);
      }
    }
  }
  return errors;
}

async function fetchJson(url) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), NETWORK_TIMEOUT_MS);
  try {
    const response = await fetch(url, { signal: controller.signal, headers: { "user-agent": "AssetIWeave-agent-market-release-check" } });
    if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
    return await response.json();
  } finally {
    clearTimeout(timeout);
  }
}

async function fetchBytes(url) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), NETWORK_TIMEOUT_MS);
  try {
    const response = await fetch(url, { signal: controller.signal, headers: { "user-agent": "AssetIWeave-agent-market-release-check" } });
    if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
    const hash = crypto.createHash("sha256");
    let size = 0;
    for await (const chunk of response.body) {
      size += chunk.length;
      if (size > MAX_NETWORK_BYTES) throw new Error(`response exceeds ${MAX_NETWORK_BYTES} bytes`);
      hash.update(chunk);
    }
    return { size, sha256: hash.digest("hex") };
  } finally {
    clearTimeout(timeout);
  }
}

function upstreamAgentUrl(revision, registryId) {
  return `https://raw.githubusercontent.com/agentclientprotocol/registry/${revision}/${registryId}/agent.json`;
}

function targetKey(target) {
  return `${target.os}-${target.arch}`;
}

function splitPackageVersion(value) {
  const index = value.lastIndexOf("@");
  if (index <= 0) return { package: value, version: null };
  return { package: value.slice(0, index), version: value.slice(index + 1) };
}

async function checkNetwork(catalog, evidence) {
  const errors = [];
  const revision = catalog.source.upstreamRevision;
  try {
    const latestRegistry = await fetchJson(evidence.upstream.snapshotUrl);
    if (latestRegistry?.version !== "1.0.0") errors.push("remote ACP registry schema version is not 1.0.0");
    const latestById = new Map((latestRegistry.agents ?? []).map((item) => [item.id, item]));
    for (const item of catalog.items) {
      if (item.protocol !== "acp") continue;
      const registryId = item.upstream.registryId;
      const source = await fetchJson(upstreamAgentUrl(revision, registryId));
      if (source.id !== registryId) errors.push(`${item.id}: pinned upstream id mismatch`);
      const latest = latestById.get(registryId);
      if (!latest) errors.push(`${item.id}: CDN registry no longer contains the pinned registry item`);
      for (const distribution of item.distributions) {
        const record = evidence.items.find((candidate) => candidate.catalogItemId === item.id && candidate.distributionId === distribution.id);
        if (distribution.type === "binary") {
          const sourceTarget = source.distribution?.binary?.[targetKey(distribution.target)];
          if (!sourceTarget) {
            errors.push(`${item.id}/${distribution.id}: pinned upstream binary target is missing`);
            continue;
          }
          if (sourceTarget.archive !== distribution.url || sourceTarget.sha256 !== distribution.sha256) errors.push(`${item.id}/${distribution.id}: binary metadata differs from pinned upstream`);
          const artifact = await fetchBytes(distribution.url);
          if (artifact.sha256 !== distribution.sha256) errors.push(`${item.id}/${distribution.id}: downloaded SHA256 does not match catalog`);
          if (record?.artifact?.sizeBytes !== artifact.size) errors.push(`${item.id}/${distribution.id}: downloaded size does not match evidence`);
        } else if (distribution.type === "npx") {
          const sourcePackage = splitPackageVersion(source.distribution?.npx?.package ?? "");
          if (sourcePackage.package !== distribution.package || sourcePackage.version !== distribution.version) errors.push(`${item.id}/${distribution.id}: npm metadata differs from pinned upstream`);
          const packageUrl = `https://registry.npmjs.org/${encodeURIComponent(distribution.package)}/${distribution.version}`;
          const packageMetadata = await fetchJson(packageUrl);
          if (packageMetadata.version !== distribution.version) errors.push(`${item.id}/${distribution.id}: npm version lookup did not resolve the catalog version`);
          if (record?.packageIntegrity !== packageMetadata.dist?.integrity) errors.push(`${item.id}/${distribution.id}: npm integrity differs from evidence`);
          const bins = typeof packageMetadata.bin === "string" ? [packageMetadata.bin] : Object.keys(packageMetadata.bin ?? {});
          if (!bins.includes(distribution.bin)) errors.push(`${item.id}/${distribution.id}: catalog bin ${distribution.bin} is not published by npm package`);
        }
      }
    }
  } catch (error) {
    errors.push(`network release validation failed: ${error.message}`);
  }
  return errors;
}

function platformTarget() {
  const osName = process.platform === "darwin" ? "darwin" : process.platform === "win32" ? "windows" : process.platform;
  const arch = process.arch === "arm64" ? "aarch64" : process.arch === "x64" ? "x86_64" : process.arch;
  return { os: osName, arch };
}

async function downloadArtifactBytes(url) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), NETWORK_TIMEOUT_MS);
  try {
    const response = await fetch(url, {
      signal: controller.signal,
      headers: { "user-agent": "AssetIWeave-agent-market-real-e2e" },
    });
    if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
    const bytes = Buffer.from(await response.arrayBuffer());
    if (bytes.length > MAX_NETWORK_BYTES) throw new Error(`response exceeds ${MAX_NETWORK_BYTES} bytes`);
    return bytes;
  } finally {
    clearTimeout(timeout);
  }
}

function isolatedEnvironment(root) {
  return {
    ...process.env,
    HOME: root,
    XDG_CONFIG_HOME: path.join(root, "config"),
    XDG_DATA_HOME: path.join(root, "data"),
    XDG_CACHE_HOME: path.join(root, "cache"),
  };
}

function runVersion(program, cwd, env) {
  return execFileSync(program, ["--version"], {
    cwd,
    env,
    encoding: "utf8",
    timeout: 30_000,
  }).trim().split(/\r?\n/, 1)[0];
}

function runAcpConformance(program, args, cwd, env) {
  return new Promise((resolve, reject) => {
    const child = spawn(program, args, { cwd, env, stdio: ["pipe", "pipe", "pipe"] });
    const output = readline.createInterface({ input: child.stdout });
    const pending = new Map();
    let nextId = 1;
    let settled = false;
    let closing = false;
    let timer;

    const finish = (error, value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      output.close();
      if (error) {
        child.kill("SIGTERM");
        reject(error);
      } else {
        resolve(value);
      }
    };

    child.stderr.on("data", () => {});
    child.once("error", (error) => finish(error));
    child.once("exit", (code, signal) => {
      if (!settled && !closing) finish(new Error(`ACP process exited before clean shutdown: ${code ?? signal}`));
    });
    output.on("line", (line) => {
      let message;
      try {
        message = JSON.parse(line);
      } catch {
        finish(new Error("ACP process emitted a non-JSON line"));
        return;
      }
      if (!Object.hasOwn(message, "id") || !pending.has(message.id)) return;
      const pendingRequest = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) pendingRequest.reject(new Error(message.error.message || "ACP request failed"));
      else pendingRequest.resolve(message.result);
    });

    const request = (method, params) => new Promise((requestResolve, requestReject) => {
      const id = nextId++;
      pending.set(id, { resolve: requestResolve, reject: requestReject });
      child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    });

    timer = setTimeout(() => finish(new Error("real ACP conformance timed out")), NETWORK_TIMEOUT_MS);
    (async () => {
      try {
        const initialize = await request("initialize", {
          protocolVersion: 1,
          clientInfo: { name: "AssetIWeave", version: "0.6.1" },
          clientCapabilities: { terminal: false, fs: { readTextFile: false, writeTextFile: false } },
        });
        if (initialize?.protocolVersion !== 1) throw new Error("ACP initialize returned an unsupported protocol version");
        const session = await request("session/new", { cwd, mcpServers: [] });
        if (!session?.sessionId) throw new Error("ACP session/new did not return a session id");
        if (initialize.agentCapabilities?.sessionCapabilities?.close) {
          await request("session/close", { sessionId: session.sessionId });
        }
        closing = true;
        child.stdin.end();
        const exitCode = await new Promise((resolveExit, rejectExit) => {
          const onExit = (code, signal) => resolveExit(code ?? (signal ? 1 : 0));
          child.once("exit", onExit);
          setTimeout(() => rejectExit(new Error("ACP process did not exit after stdin close")), 10_000);
        });
        if (exitCode !== 0) throw new Error(`ACP process exited with status ${exitCode}`);
        finish(null, {
          status: "passed",
          initialize: "passed",
          sessionNew: "passed",
          sessionClose: initialize.agentCapabilities?.sessionCapabilities?.close ? "passed" : "not_advertised",
          cleanShutdown: "passed",
        });
      } catch (error) {
        finish(error);
      }
    })();
  });
}

async function runRealE2e(catalog) {
  const target = platformTarget();
  const candidates = catalog.items
    .filter((item) => item.protocol === "acp")
    .flatMap((item) => item.distributions
      .filter((distribution) => distribution.type === "binary" && distribution.target?.os === target.os && distribution.target?.arch === target.arch)
      .map((distribution) => ({ item, distribution })));
  const selected = candidates.find(({ item }) => item.id === "opencode") ?? candidates[0];
  if (!selected) throw new Error(`catalog has no ACP binary for ${target.os}-${target.arch}`);
  const { item, distribution } = selected;
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-agent-market-e2e-"));
  const workspace = path.join(root, "workspace");
  const home = path.join(root, "home");
  fs.mkdirSync(workspace, { recursive: true });
  fs.mkdirSync(home, { recursive: true });
  const env = isolatedEnvironment(home);
  try {
    const bytes = await downloadArtifactBytes(distribution.url);
    if (sha256(bytes) !== distribution.sha256) throw new Error("real ACP artifact SHA256 does not match catalog");
    if (distribution.size !== bytes.length) throw new Error("real ACP artifact size does not match catalog");
    const archive = path.join(root, "artifact.zip");
    fs.writeFileSync(archive, bytes);
    execFileSync("unzip", ["-q", "-o", archive, "-d", root], { timeout: 60_000 });
    const program = path.join(root, distribution.executable);
    fs.chmodSync(program, 0o755);
    const versionOutput = runVersion(program, workspace, env);
    const conformance = await runAcpConformance(program, distribution.launchArgs, workspace, env);
    return {
      status: "passed",
      observedAt: new Date().toISOString(),
      host: target,
      catalogItemId: item.id,
      distributionId: distribution.id,
      artifact: { url: distribution.url, sha256: distribution.sha256, sizeBytes: bytes.length },
      install: {
        status: "passed",
        steps: ["download", "sha256", "extract", "executable-version"],
        versionOutput,
      },
      acpConformance: conformance,
    };
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

let catalogBytes;
let catalog;
try {
  catalogBytes = fs.readFileSync(catalogPath);
  catalog = JSON.parse(catalogBytes);
} catch (error) {
  console.error(`Agent catalog release check failed: could not read ${catalogPath}: ${error.message}`);
  process.exit(1);
}

const errors = checkCatalog(catalog);
let evidence;
if (release) {
  if (!/^[a-f0-9]{40}$/i.test(catalog?.source?.upstreamRevision ?? "")) errors.push("release requires a pinned 40-character upstream git revision");
  try {
    evidence = JSON.parse(fs.readFileSync(evidencePath, "utf8"));
  } catch (error) {
    errors.push(`release evidence could not be read: ${error.message}`);
  }
  if (evidence) errors.push(...checkEvidence(catalog, catalogBytes, evidence));
}

if (network) {
  if (!release || !evidence) errors.push("--network requires --release and a readable release evidence file");
  else errors.push(...await checkNetwork(catalog, evidence));
}

if (e2e) {
  if (!release || !evidence) {
    errors.push("--e2e requires --release and a readable release evidence file");
  } else {
    try {
      const result = await runRealE2e(catalog);
      const expected = evidence.realE2e;
      if (expected?.catalogItemId !== result.catalogItemId || expected?.distributionId !== result.distributionId) {
        errors.push("real ACP E2E result does not identify the evidence distribution");
      }
      if (expected?.artifact?.url !== result.artifact.url || expected?.artifact?.sha256 !== result.artifact.sha256 || expected?.artifact?.sizeBytes !== result.artifact.sizeBytes) {
        errors.push("real ACP E2E result does not match release evidence artifact identity");
      }
      if (result.acpConformance.status !== "passed") errors.push("real ACP E2E conformance did not pass");
      if (errors.length === 0) console.log(`Real ACP E2E passed: ${result.catalogItemId}/${result.distributionId} ${result.install.versionOutput}`);
    } catch (error) {
      errors.push(`real ACP E2E failed: ${error.message}`);
    }
  }
}

if (errors.length > 0) {
  for (const error of errors) console.error(`- ${error}`);
  process.exitCode = 1;
} else {
  const mode = network ? "network release" : release ? "release" : "static";
  console.log(`Agent catalog ${mode} check passed: ${catalog.items.length} items, catalog ${catalog.catalogVersion}`);
}
