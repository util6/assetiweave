#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const DEFAULT_CATALOG = path.join(ROOT, "builtin-assets", "agent-market", "catalog-v1.json");
const CURRENT_CORE = JSON.parse(fs.readFileSync(path.join(ROOT, "package.json"), "utf8")).version;

const args = new Set(process.argv.slice(2));
const catalogFlag = process.argv.indexOf("--catalog");
const catalogPath = catalogFlag >= 0 ? path.resolve(process.argv[catalogFlag + 1]) : DEFAULT_CATALOG;
const release = args.has("--release");

function fail(message) {
  console.error(`Agent catalog release check failed: ${message}`);
  process.exitCode = 1;
}

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

function containsPlaceholder(value) {
  const text = JSON.stringify(value).toLowerCase();
  return /fixture|placeholder|example\.com|downloads\.example|a{64}|0{64}/.test(text);
}

function checkCatalog(catalog) {
  const errors = [];
  if (catalog?.schema !== "assetiweave.agent-market/v1") errors.push("schema must be assetiweave.agent-market/v1");
  if (!/^\d{4}\.\d{2}\.\d{2}\.\d+$/.test(catalog?.catalogVersion ?? "")) errors.push("catalogVersion must be a fixed YYYY.MM.DD.N revision");
  if (!catalog?.generatedAt || Number.isNaN(Date.parse(catalog.generatedAt))) errors.push("generatedAt must be an RFC3339 date");
  if (catalog?.source?.upstream !== "agentclientprotocol/registry") errors.push("source.upstream must be agentclientprotocol/registry");
  if (!catalog?.source?.upstreamRevision || /fixture|placeholder|latest/i.test(catalog.source.upstreamRevision)) errors.push("source.upstreamRevision must identify a fixed upstream snapshot");
  if (!Array.isArray(catalog?.items) || catalog.items.length === 0) errors.push("catalog must contain at least one item");
  if (containsPlaceholder(catalog)) errors.push("catalog contains fixture, example.com, placeholder, or placeholder SHA data");

  const ids = new Set();
  for (const item of catalog?.items ?? []) {
    if (!item?.id || ids.has(item.id)) errors.push(`duplicate or empty item id: ${item?.id ?? "<empty>"}`);
    ids.add(item?.id);
    if (!parseVersion(item?.version)) errors.push(`${item?.id}: version must be semver`);
    if (!isCompatible(CURRENT_CORE, `>=${item?.coreCompatibility?.min}, <${item?.coreCompatibility?.maxExclusive}`)) {
      errors.push(`${item?.id}: coreCompatibility does not include ${CURRENT_CORE}`);
    }
    if (item?.verification?.status === "tested" && !item?.verification?.evidenceId) errors.push(`${item?.id}: tested item needs evidenceId`);
    if (!Array.isArray(item?.distributions) || item.distributions.length === 0) errors.push(`${item?.id}: no distributions`);
    for (const distribution of item?.distributions ?? []) {
      if (distribution.type === "binary") {
        if (!/^https:\/\//.test(distribution.url ?? "")) errors.push(`${item.id}/${distribution.id}: binary URL must be HTTPS`);
        if (!/^[a-f0-9]{64}$/i.test(distribution.sha256 ?? "")) errors.push(`${item.id}/${distribution.id}: binary SHA256 must be 64 hex characters`);
        if (!["none", "zip", "tar.gz", "tgz", "tar.bz2", "tbz2"].includes(distribution.archive)) errors.push(`${item.id}/${distribution.id}: unsupported archive format`);
      }
      if (distribution.type === "npx" && (!distribution.package || !parseVersion(distribution.version) || !distribution.bin)) errors.push(`${item.id}/${distribution.id}: incomplete npm distribution`);
      if (distribution.type === "uvx" && (!distribution.package || !parseVersion(distribution.version) || !distribution.command)) errors.push(`${item.id}/${distribution.id}: incomplete uvx distribution`);
    }
  }
  return errors;
}

let catalog;
try {
  catalog = JSON.parse(fs.readFileSync(catalogPath, "utf8"));
} catch (error) {
  fail(`could not read ${catalogPath}: ${error.message}`);
}

if (catalog) {
  const errors = checkCatalog(catalog);
  if (release && catalog?.source?.upstreamRevision === "") errors.push("release requires upstream revision evidence");
  if (errors.length > 0) {
    for (const error of errors) console.error(`- ${error}`);
    process.exitCode = 1;
  } else {
    console.log(`Agent catalog release check passed: ${catalog.items.length} items, catalog ${catalog.catalogVersion}, core ${CURRENT_CORE}`);
  }
}
