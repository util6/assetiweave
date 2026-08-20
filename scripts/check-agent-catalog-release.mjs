#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const DEFAULT_CATALOG = path.join(ROOT, "builtin-assets", "agent-market", "catalog-v1.json");
const DEFAULT_EVIDENCE = path.join(ROOT, "builtin-assets", "agent-market", "release-evidence-v1.json");
const CURRENT_CORE = JSON.parse(fs.readFileSync(path.join(ROOT, "package.json"), "utf8")).version;
const args = new Set(process.argv.slice(2));
const catalogFlag = process.argv.indexOf("--catalog");
const evidenceFlag = process.argv.indexOf("--evidence");
const catalogPath = catalogFlag >= 0 ? path.resolve(process.argv[catalogFlag + 1]) : DEFAULT_CATALOG;
const evidencePath = evidenceFlag >= 0 ? path.resolve(process.argv[evidenceFlag + 1]) : DEFAULT_EVIDENCE;
const release = args.has("--release");
const network = args.has("--network");
const MAX_NETWORK_BYTES = 256 * 1024 * 1024;
const NETWORK_TIMEOUT_MS = 30_000;

function parseVersion(value) {
  const match = /^(\d+)\.(\d+)\.(\d+)(?:[-+].*)?$/.exec(String(value));
  return match ? [Number(match[1]), Number(match[2]), Number(match[3])] : null;
}

function compareVersion(left, right) {
  for (let index = 0; index < 3; index += 1) {
    if (left[index] !== right[index]) return left[index] - right[index];
  }
  return 0;
}

function isCompatible(version, range) {
  const current = parseVersion(version);
  const match = /^\s*>=\s*([^,]+)\s*,\s*<\s*(.+?)\s*$/.exec(String(range));
  const minimum = match && parseVersion(match[1]);
  const maximum = match && parseVersion(match[2]);
  return Boolean(current && minimum && maximum)
    && compareVersion(current, minimum) >= 0
    && compareVersion(current, maximum) < 0;
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
    if (!parseVersion(item?.version)) errors.push(`${item?.id}: version must be semver`);
    if (!isCompatible(CURRENT_CORE, `>=${item?.coreCompatibility?.min}, <${item?.coreCompatibility?.maxExclusive}`)) {
      errors.push(`${item?.id}: coreCompatibility does not include ${CURRENT_CORE}`);
    }
    if (!parseVersion(item?.coreCompatibility?.min) || !parseVersion(item?.coreCompatibility?.maxExclusive)) {
      errors.push(`${item?.id}: coreCompatibility must contain fixed semver bounds`);
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
  if (evidence.coreVersion !== CURRENT_CORE) errors.push(`evidence coreVersion does not match ${CURRENT_CORE}`);
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
      if (record.agentVersion !== item.version) errors.push(`${item.id}/${distribution.id}: evidence version does not match catalog`);
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
      const conformance = record.acpConformance;
      const conformancePassed = conformance?.status === "passed"
        && ["initialize", "sessionNew", "sessionClose", "cleanShutdown"].every((step) => conformance[step] === "passed");
      if (item.verification?.status === "tested" && !conformancePassed) errors.push(`${item.id}/${distribution.id}: tested item requires complete ACP conformance evidence`);
      if (conformancePassed && ["binary", "npx", "uvx"].includes(distribution.type)) passedManagedConformance = true;
    }
  }
  if (!passedManagedConformance) errors.push("release evidence must contain at least one passed managed ACP conformance");
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
      const registryId = item.upstream.registryId;
      const source = await fetchJson(upstreamAgentUrl(revision, registryId));
      if (source.id !== registryId) errors.push(`${item.id}: pinned upstream id mismatch`);
      if (source.version !== item.version) errors.push(`${item.id}: pinned upstream version ${source.version} does not match catalog ${item.version}`);
      const latest = latestById.get(registryId);
      if (!latest || latest.version !== item.version) errors.push(`${item.id}: CDN registry version does not match pinned catalog version`);
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

if (errors.length > 0) {
  for (const error of errors) console.error(`- ${error}`);
  process.exitCode = 1;
} else {
  const mode = network ? "network release" : release ? "release" : "static";
  console.log(`Agent catalog ${mode} check passed: ${catalog.items.length} items, catalog ${catalog.catalogVersion}, core ${CURRENT_CORE}`);
}
