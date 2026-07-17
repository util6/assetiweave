import { lstat, opendir, rm } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const artifactDirectories = ["target", "dist"];
const defaultMaxArtifactMiB = 6144;

async function pathSize(path, seenFiles) {
  let info;
  try {
    info = await lstat(path);
  } catch (error) {
    if (error?.code === "ENOENT") {
      return 0;
    }
    throw error;
  }

  if (info.isSymbolicLink()) {
    return 0;
  }
  if (!info.isDirectory()) {
    const fileKey = `${info.dev}:${info.ino}`;
    if (seenFiles.has(fileKey)) {
      return 0;
    }
    seenFiles.add(fileKey);
    return typeof info.blocks === "number" ? info.blocks * 512 : info.size;
  }

  let total = 0;
  const directory = await opendir(path);
  for await (const entry of directory) {
    total += await pathSize(join(path, entry.name), seenFiles);
  }
  return total;
}

export async function artifactUsage(root) {
  const seenFiles = new Set();
  const sizes = await Promise.all(
    artifactDirectories.map((directory) => pathSize(join(root, directory), seenFiles)),
  );
  return sizes.reduce((total, size) => total + size, 0);
}

export function maxArtifactBytes(env = process.env) {
  const rawValue = env.ASSETIWEAVE_BUILD_ARTIFACT_MAX_MB ?? String(defaultMaxArtifactMiB);
  const maxMiB = Number(rawValue);
  if (!Number.isFinite(maxMiB) || maxMiB <= 0) {
    throw new Error("ASSETIWEAVE_BUILD_ARTIFACT_MAX_MB must be a positive number");
  }
  return Math.floor(maxMiB * 1024 * 1024);
}

export function formatBytes(bytes) {
  if (bytes < 1024) {
    return `${bytes} B`;
  }

  const units = ["KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unit = "B";
  for (const nextUnit of units) {
    value /= 1024;
    unit = nextUnit;
    if (value < 1024) {
      break;
    }
  }
  return `${value.toFixed(1)} ${unit}`;
}

export function assertWithinBudget(usage, maximum) {
  if (usage > maximum) {
    throw new Error(
      `Build artifacts use ${formatBytes(usage)} and exceed the ${formatBytes(maximum)} budget`,
    );
  }
}

export async function cleanArtifacts(root) {
  await Promise.all(
    artifactDirectories.map((directory) =>
      rm(join(root, directory), { recursive: true, force: true }),
    ),
  );
}

async function main() {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const command = process.argv[2] ?? "check";
  const maximum = maxArtifactBytes();
  const usage = await artifactUsage(root);

  if (command === "clean") {
    await cleanArtifacts(root);
    console.log(`Removed ${formatBytes(usage)} of build artifacts from target/ and dist/.`);
    return;
  }

  if (command === "guard") {
    if (usage > maximum) {
      await cleanArtifacts(root);
      console.log(
        `Build artifacts reached ${formatBytes(usage)}, above the ${formatBytes(maximum)} budget; cleaned target/ and dist/.`,
      );
      return;
    }
    console.log(`Build artifacts use ${formatBytes(usage)} of ${formatBytes(maximum)}.`);
    return;
  }

  if (command !== "check") {
    throw new Error(`Unknown command: ${command}. Use check, guard, or clean.`);
  }

  assertWithinBudget(usage, maximum);
  console.log(`Build artifacts use ${formatBytes(usage)} of ${formatBytes(maximum)}.`);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
