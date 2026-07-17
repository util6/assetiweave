import assert from "node:assert/strict";
import { link, mkdir, mkdtemp, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  artifactUsage,
  assertWithinBudget,
  cleanArtifacts,
  maxArtifactBytes,
} from "./build-artifacts.mjs";

test("artifactUsage totals target and dist without counting dependencies", async () => {
  const root = await mkdtemp(join(tmpdir(), "assetiweave-artifacts-"));
  await mkdir(join(root, "target", "debug"), { recursive: true });
  await mkdir(join(root, "dist"), { recursive: true });
  await mkdir(join(root, "node_modules"), { recursive: true });
  await writeFile(join(root, "target", "debug", "app"), Buffer.alloc(7));
  await writeFile(join(root, "dist", "index.js"), Buffer.alloc(5));
  await writeFile(join(root, "node_modules", "dependency"), Buffer.alloc(11));

  const targetFile = await stat(join(root, "target", "debug", "app"));
  const distFile = await stat(join(root, "dist", "index.js"));
  const allocatedSize = (info) =>
    typeof info.blocks === "number" ? info.blocks * 512 : info.size;

  assert.equal(
    await artifactUsage(root),
    allocatedSize(targetFile) + allocatedSize(distFile),
  );
});

test("assertWithinBudget rejects usage above the configured maximum", () => {
  assert.doesNotThrow(() => assertWithinBudget(12, 12));
  assert.throws(() => assertWithinBudget(13, 12), /exceed the 12 B budget/);
});

test("artifactUsage counts hard-linked build files once", async () => {
  const root = await mkdtemp(join(tmpdir(), "assetiweave-artifacts-"));
  await mkdir(join(root, "target"), { recursive: true });
  await mkdir(join(root, "dist"), { recursive: true });
  const targetFile = join(root, "target", "shared-binary");
  await writeFile(targetFile, Buffer.alloc(7));
  await link(targetFile, join(root, "dist", "shared-binary"));

  const info = await stat(targetFile);
  const expected = typeof info.blocks === "number" ? info.blocks * 512 : info.size;
  assert.equal(await artifactUsage(root), expected);
});

test("maxArtifactBytes accepts a positive MiB override", () => {
  assert.equal(maxArtifactBytes({ ASSETIWEAVE_BUILD_ARTIFACT_MAX_MB: "128" }), 128 * 1024 * 1024);
  assert.throws(
    () => maxArtifactBytes({ ASSETIWEAVE_BUILD_ARTIFACT_MAX_MB: "0" }),
    /positive number/,
  );
});

test("cleanArtifacts removes generated roots and preserves dependencies", async () => {
  const root = await mkdtemp(join(tmpdir(), "assetiweave-artifacts-"));
  await mkdir(join(root, "target"), { recursive: true });
  await mkdir(join(root, "dist"), { recursive: true });
  await mkdir(join(root, "node_modules"), { recursive: true });

  await cleanArtifacts(root);

  await assert.rejects(stat(join(root, "target")), { code: "ENOENT" });
  await assert.rejects(stat(join(root, "dist")), { code: "ENOENT" });
  assert.equal((await stat(join(root, "node_modules"))).isDirectory(), true);
});
