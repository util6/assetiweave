import { access, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const assetSourcePath = path.join(repositoryRoot, "assets", "app-icons", "icons.ts");
const bundledIconRoot = path.join(repositoryRoot, "assets", "assetiweave");
const bundledIconFiles = [
  "app-icon-display.png",
  "app-icon-minimized.png",
  "icon.png",
  "32x32.png",
  "128x128.png",
  "128x128@2x.png",
  "icon.icns",
  "icon.ico",
];

export function validateAppIconAssets(assets) {
  const errors = [];
  if (!assets || typeof assets !== "object" || Array.isArray(assets)) {
    return ["app icon source must export an object"];
  }

  for (const [appKind, asset] of Object.entries(assets)) {
    if (!/^[a-z][a-z0-9-]*$/.test(appKind)) {
      errors.push(`${appKind}: invalid app id`);
    }
    if (!asset || typeof asset !== "object") {
      errors.push(`${appKind}: entry must be an object`);
      continue;
    }
    if (typeof asset.legacyIcon !== "string" || asset.legacyIcon.length === 0) {
      errors.push(`${appKind}: legacyIcon must be a non-empty string`);
    }
    if (typeof asset.svg !== "string") {
      errors.push(`${appKind}: svg must be a string`);
      continue;
    }
    for (const error of validateSvgSource(asset.svg)) {
      errors.push(`${appKind}: ${error}`);
    }
  }

  return errors;
}

export function validateSvgSource(source) {
  const errors = [];
  if (!/^\s*<svg\b/i.test(source)) {
    errors.push("root element must be svg");
  }
  if (!/\bviewBox=["']\s*0\s+0\s+\d+\s+\d+\s*["']/i.test(source)) {
    errors.push("viewBox must be valid coordinates");
  }
  if (!/<path\b[^>]*\bd=["'][^"']+["']/i.test(source)) {
    errors.push("SVG must contain at least one path with d");
  }
  return errors;
}

export function evaluateAppIconSource(source) {
  const executable = source.replace(/export default\s+/, "return ");
  return new Function(executable)();
}

export async function checkAppIcons() {
  const source = await readFile(assetSourcePath, "utf8");
  const assets = evaluateAppIconSource(source);
  const errors = validateAppIconAssets(assets);
  if (errors.length > 0) {
    throw new Error(["App icon asset check failed:", ...errors.map((error) => `- ${error}`)].join("\n"));
  }

  await checkBundledAppIcons();
  return Object.keys(assets).length;
}

export async function checkBundledAppIcons() {
  for (const file of bundledIconFiles) {
    try {
      await access(path.join(bundledIconRoot, file));
    } catch (error) {
      if (error?.code === "ENOENT") {
        throw new Error(`Missing bundled app icon asset: ${file}`);
      }
      throw error;
    }
  }

  return bundledIconFiles.length;
}

async function main() {
  const count = await checkAppIcons();
  console.log(`Checked ${count} SVG app icons and ${bundledIconFiles.length} bundled app icon files.`);
}

if (path.resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}
