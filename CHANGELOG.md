# Changelog

## v0.1.1

- Add signed in-app update checks, downloads, installation, and restart.
- Publish `latest.json` and updater signatures from the GitHub Release workflow.
- Publish updater-enabled releases as stable releases so the configured `/releases/latest` endpoint resolves.

## v0.1.0

- Initial GitHub release.
- Desktop installers for Windows and Linux, macOS app archives, and optional macOS DMGs are produced by GitHub Actions.
- CLI tool archives include `assetiweave-cli` and `assetiweave-engine` for supported platforms.
