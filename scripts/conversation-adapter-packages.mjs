import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const catalogRoot = path.join(root, "parser-catalog");
const indexPath = path.join(catalogRoot, "index.json");
const legacyCatalogPath = path.join(catalogRoot, "catalog.json");
const outputRoot = path.join(root, "target", "conversation-adapter-packages");
const command = process.argv[2] ?? "check";
const update = process.argv.includes("--update");
const crc32Table = Array.from({ length: 256 }, (_, index) => {
  let value = index;
  for (let bit = 0; bit < 8; bit += 1) {
    value = (value & 1) === 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
  }
  return value >>> 0;
});

if (!['check', 'build'].includes(command)) {
  throw new Error("usage: node scripts/conversation-adapter-packages.mjs <check|build> [--update]");
}

const index = readJson(indexPath);
const legacyCatalog = readJson(legacyCatalogPath);
assert(index.schema_version === 2, "Catalog v2 index schema_version must be 2");
assert(Array.isArray(index.packages) && index.packages.length > 0, "Catalog v2 index packages are required");

if (command === "build") {
  fs.rmSync(outputRoot, { force: true, recursive: true });
  fs.mkdirSync(outputRoot, { recursive: true });
}

const seenPackageVersions = new Set();
for (const packageIndex of index.packages) {
  validatePackageId(packageIndex.package_id);
  const historyPath = resolveCatalogPath(packageIndex.history_url);
  const history = readJson(historyPath);
  assert(history.schema_version === 2, `history schema_version must be 2: ${packageIndex.package_id}`);
  assert(history.package_id === packageIndex.package_id, `history package_id mismatch: ${packageIndex.package_id}`);
  assert(Array.isArray(history.releases) && history.releases.length > 0, `history releases are required: ${packageIndex.package_id}`);

  const adapterDir = adapterDirectoryFromHistory(history);
  const packageManifestPath = path.join(adapterDir, history.package_manifest_file ?? "conversation-adapter-package.json");
  const adapterManifestPath = path.join(adapterDir, history.adapter_manifest_file ?? "conversation-adapter.json");
  const packageManifest = readJson(packageManifestPath);
  const adapterManifest = readJson(adapterManifestPath);
  assert(packageManifest.package_id === history.package_id, `package manifest id mismatch: ${history.package_id}`);
  assert(adapterManifest.id === history.adapter_id, `adapter manifest id mismatch: ${history.package_id}`);
  assert(packageManifest.adapter_manifest === path.basename(adapterManifestPath), `adapter manifest path mismatch: ${history.package_id}`);
  assert(packageManifest.runtime?.protocol === "stdio-ndjson-v1", `unsupported runtime protocol: ${history.package_id}`);
  assertVersion(packageManifest.version, `package manifest version: ${history.package_id}`);
  assertVersion(packageManifest.min_core_version, `minimum Core version: ${history.package_id}`);

  const release = history.releases.find((candidate) => candidate.version === packageManifest.version);
  assert(release, `history is missing active package version: ${history.package_id}@${packageManifest.version}`);
  assert(packageIndex.stable_version === release.version || packageIndex.beta_version === release.version,
    `index does not reference active package version: ${history.package_id}@${release.version}`);
  assert(release.runtime_protocol === packageManifest.runtime.protocol, `history runtime protocol mismatch: ${history.package_id}`);
  assert(typeof release.core_compatibility === "string" && release.core_compatibility.trim(),
    `Core compatibility is required: ${history.package_id}@${release.version}`);
  const immutableKey = `${history.package_id}@${release.version}`;
  assert(!seenPackageVersions.has(immutableKey), `duplicate immutable package version: ${immutableKey}`);
  seenPackageVersions.add(immutableKey);

  const contentHash = hashDirectory(adapterDir);
  const legacyItem = legacyCatalog.items.find((item) => item.id === history.package_id);
  assert(legacyItem, `legacy compatibility catalog is missing ${history.package_id}`);
  if (update) {
    legacyItem.expected_package_hash = contentHash;
  } else {
    assert(legacyItem.expected_package_hash === contentHash,
      `package content hash mismatch: ${history.package_id}; run pnpm conversation-adapters:build --update`);
  }

  const artifactName = `${history.package_id}-${release.version}-universal.zip`;
  assert(release.artifact_url.endsWith(`/${artifactName}`), `artifact filename mismatch: ${immutableKey}`);
  const artifactPath = command === "build"
    ? path.join(outputRoot, artifactName)
    : path.join(fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-adapter-artifact-")), artifactName);
  buildZip(adapterDir, artifactPath);
  const artifactBytes = fs.readFileSync(artifactPath);
  const artifactHash = sha256(artifactBytes);
  if (update) {
    release.artifact_sha256 = artifactHash;
    release.artifact_size = artifactBytes.length;
  } else {
    assert(release.artifact_sha256 === artifactHash,
      `artifact hash mismatch: ${immutableKey}; run pnpm conversation-adapters:build --update`);
    assert(release.artifact_size === artifactBytes.length,
      `artifact size mismatch: ${immutableKey}; run pnpm conversation-adapters:build --update`);
  }
  if (command === "check") {
    fs.rmSync(path.dirname(artifactPath), { force: true, recursive: true });
  }
  if (update) {
    writeJson(historyPath, history);
  }
}

if (update) {
  writeJson(legacyCatalogPath, legacyCatalog);
}

const verb = command === "build" ? "built" : "checked";
console.log(`${verb} ${index.packages.length} conversation adapter packages${update ? " and updated hashes" : ""}`);

function adapterDirectoryFromHistory(history) {
  const source = history.releases[0]?.source;
  assert(source?.type === "github", `a GitHub source is required to build ${history.package_id}`);
  const marker = "/parser-catalog/adapters/";
  const markerIndex = source.url.indexOf(marker);
  assert(markerIndex >= 0, `source URL must point into parser-catalog/adapters: ${history.package_id}`);
  const relative = source.url.slice(markerIndex + 1);
  const directory = path.join(root, relative);
  assert(fs.statSync(directory).isDirectory(), `adapter directory does not exist: ${directory}`);
  return directory;
}

function buildZip(directory, artifactPath) {
  fs.mkdirSync(path.dirname(artifactPath), { recursive: true });
  fs.rmSync(artifactPath, { force: true });
  const files = listFiles(directory).map((file) => path.relative(directory, file).split(path.sep).join("/"));
  assert(files.length <= 0xffff, "ZIP contains too many files");

  const localParts = [];
  const centralParts = [];
  let localOffset = 0;
  for (const relative of files) {
    const name = Buffer.from(relative, "utf8");
    const contents = fs.readFileSync(path.join(directory, relative));
    assert(name.length <= 0xffff, `ZIP path is too long: ${relative}`);
    assert(contents.length <= 0xffffffff, `ZIP file is too large: ${relative}`);
    const checksum = crc32(contents);
    const localHeader = Buffer.alloc(30);
    localHeader.writeUInt32LE(0x04034b50, 0);
    localHeader.writeUInt16LE(20, 4);
    localHeader.writeUInt16LE(0x0800, 6);
    localHeader.writeUInt16LE(0, 8);
    localHeader.writeUInt16LE(0, 10);
    localHeader.writeUInt16LE(0x0021, 12);
    localHeader.writeUInt32LE(checksum, 14);
    localHeader.writeUInt32LE(contents.length, 18);
    localHeader.writeUInt32LE(contents.length, 22);
    localHeader.writeUInt16LE(name.length, 26);
    localHeader.writeUInt16LE(0, 28);
    localParts.push(localHeader, name, contents);

    const centralHeader = Buffer.alloc(46);
    centralHeader.writeUInt32LE(0x02014b50, 0);
    centralHeader.writeUInt16LE(20, 4);
    centralHeader.writeUInt16LE(20, 6);
    centralHeader.writeUInt16LE(0x0800, 8);
    centralHeader.writeUInt16LE(0, 10);
    centralHeader.writeUInt16LE(0, 12);
    centralHeader.writeUInt16LE(0x0021, 14);
    centralHeader.writeUInt32LE(checksum, 16);
    centralHeader.writeUInt32LE(contents.length, 20);
    centralHeader.writeUInt32LE(contents.length, 24);
    centralHeader.writeUInt16LE(name.length, 28);
    centralHeader.writeUInt16LE(0, 30);
    centralHeader.writeUInt16LE(0, 32);
    centralHeader.writeUInt16LE(0, 34);
    centralHeader.writeUInt16LE(0, 36);
    centralHeader.writeUInt32LE(0, 38);
    centralHeader.writeUInt32LE(localOffset, 42);
    centralParts.push(centralHeader, name);
    localOffset += localHeader.length + name.length + contents.length;
  }

  const centralDirectory = Buffer.concat(centralParts);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(0, 4);
  end.writeUInt16LE(0, 6);
  end.writeUInt16LE(files.length, 8);
  end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(centralDirectory.length, 12);
  end.writeUInt32LE(localOffset, 16);
  end.writeUInt16LE(0, 20);
  fs.writeFileSync(artifactPath, Buffer.concat([...localParts, centralDirectory, end]));
}

function crc32(bytes) {
  let value = 0xffffffff;
  for (const byte of bytes) {
    value = crc32Table[(value ^ byte) & 0xff] ^ (value >>> 8);
  }
  return (value ^ 0xffffffff) >>> 0;
}

function hashDirectory(directory) {
  const hash = crypto.createHash("sha256");
  for (const file of listFiles(directory)) {
    const relative = path.relative(directory, file).split(path.sep).join("/");
    hash.update(relative);
    hash.update("\0");
    hash.update(fs.readFileSync(file));
    hash.update("\0");
  }
  return hash.digest("hex");
}

function listFiles(directory) {
  const files = [];
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isSymbolicLink()) {
      throw new Error(`package contains a symbolic link: ${entryPath}`);
    }
    if (entry.isDirectory()) {
      files.push(...listFiles(entryPath));
    } else if (entry.isFile()) {
      files.push(entryPath);
    }
  }
  return files.sort((left, right) => left.localeCompare(right));
}

function resolveCatalogPath(relativePath) {
  const resolved = path.resolve(catalogRoot, relativePath);
  assert(resolved.startsWith(`${catalogRoot}${path.sep}`), `history path escapes parser-catalog: ${relativePath}`);
  return resolved;
}

function validatePackageId(value) {
  assert(/^[a-z0-9](?:[a-z0-9._-]*[a-z0-9])?$/.test(value) && value.includes("."),
    `package_id must be publisher scoped and path safe: ${value}`);
}

function assertVersion(value, label) {
  assert(/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(value), `${label} must be SemVer: ${value}`);
}

function sha256(bytes) {
  return crypto.createHash("sha256").update(bytes).digest("hex");
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function writeJson(file, value) {
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
