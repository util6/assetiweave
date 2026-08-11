import { mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const cliResourceDir = join(root, "src-tauri", "bundled-cli", "cli");

mkdirSync(cliResourceDir, { recursive: true });
console.log(`Ensured Tauri CLI resource directory: ${cliResourceDir}`);
