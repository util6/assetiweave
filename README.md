# AssetIWeave

AssetIWeave is a local-first desktop app for organizing AI file assets, sources, mount targets, and skill groups.

## Downloads

The v0.1 release is distributed through GitHub Releases:

- macOS: download the `.app.zip` for Apple Silicon or Intel Macs. A `.dmg` is uploaded too when the GitHub macOS runner completes Tauri's DMG packaging step.
- Windows: download the `.msi` or `.exe` installer.
- Linux: download the `.AppImage`, `.deb`, or `.rpm` package.
- CLI tools: download the `assetiweave-tools-*` archive for your platform. It contains `assetiweave-cli` and the required `assetiweave-engine` binary.

Latest release: https://github.com/util6/assetiweave/releases/latest

This v0.1 release is not OS-code-signed. macOS and Windows can show additional trust prompts until platform signing is configured.

## Development

Install dependencies:

```sh
pnpm install
```

Run the desktop app in development:

```sh
pnpm tauri:dev
```

Run checks:

```sh
pnpm typecheck
pnpm test
pnpm build
go test ./...
cargo test --workspace
```

Build desktop bundles locally:

```sh
pnpm tauri build
```

Build local CLI tools:

```sh
pnpm cli:install
pnpm cli:run -- doctor
```

## Release

The GitHub release workflow runs when a `v*` tag is pushed or when it is started manually from the Actions tab.

Before publishing an app update, configure these repository secrets:

- `TAURI_SIGNING_PRIVATE_KEY`: the contents of the Tauri updater private key.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: optional; leave empty when the key has no password.

The updater public key is stored in `src-tauri/tauri.conf.json`, and the app checks `https://github.com/util6/assetiweave/releases/latest/download/latest.json`.

To publish v0.1.0:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The release workflow builds Windows and Linux installers, macOS updater archives, macOS app archives, optional macOS DMGs, CLI tool archives, updater signatures, and `latest.json`, then uploads them to the same GitHub Release.
