import assert from "node:assert/strict";
import test from "node:test";
import {
  checkBundledAppIcons,
  checkAppIcons,
  evaluateAppIconSource,
  validateAppIconAssets,
  validateSvgSource,
} from "./app-icons.mjs";

test("the checked-in app icon source file is valid", async () => {
  assert.ok((await checkAppIcons()) >= 1);
});

test("the bundled AssetIWeave app icon files are present", async () => {
  assert.equal(await checkBundledAppIcons(), 8);
});

test("asset validation reports malformed entries", () => {
  const errors = validateAppIconAssets({
    "Bad ID": { legacyIcon: "", accentColor: "invalid", svg: '<svg viewBox="invalid"></svg>' },
  });

  assert.deepEqual(errors, [
    "Bad ID: invalid app id",
    "Bad ID: legacyIcon must be a non-empty string",
    "Bad ID: accentColor must be a six-digit hex color",
    "Bad ID: viewBox must be valid coordinates",
    "Bad ID: SVG must contain at least one path with d",
  ]);
});

test("the source evaluator reads the central SVG code object", () => {
  const assets = evaluateAppIconSource(`export default {
    sample: { legacyIcon: "S", accentColor: "#123456", svg: '<svg viewBox="0 0 24 24"><path d="M0 0" /></svg>' },
  };`);

  assert.equal(assets.sample.legacyIcon, "S");
  assert.deepEqual(validateSvgSource(assets.sample.svg), []);
});
