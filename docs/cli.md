# AssetIWeave CLI

AssetIWeave CLI follows the Feishu/Lark CLI-style split:

```text
AI / user
  -> assetiweave-cli      Go, Cobra command surface
  -> assetiweave-engine   Rust JSON-RPC over stdio
  -> src-tauri/src/service.rs Rust service facade
  -> SQLite / filesystem / symlink targets
```

The Go CLI is only a client. It does not write SQLite directly, copy skills, or
create symlinks. Those operations stay in Rust so the desktop app and CLI share
the same business rules.

## Build

```bash
pnpm engine:build
pnpm cli:build
```

Or build both local binaries through the wrapper script:

```bash
pnpm cli:install
```

During development, `assetiweave-cli` resolves the engine in this order:

1. `ASSETIWEAVE_ENGINE`
2. `assetiweave-engine` on `PATH`
3. `target/debug/assetiweave-engine`

For isolated tests or smoke checks, set `ASSETIWEAVE_DB_PATH` and `HOME` to
temporary locations.

`pnpm cli:run -- <args>` runs `scripts/run.js`, which locates the local
`assetiweave-cli` binary and injects `ASSETIWEAVE_ENGINE` when a local engine
binary is available.

## Commands

```bash
assetiweave-cli overview
assetiweave-cli source list
assetiweave-cli source add --name LocalSkills --path ./skills --dry-run
assetiweave-cli source scan --kind skill
assetiweave-cli profile list
assetiweave-cli asset list --kind skill

assetiweave-cli skill list
assetiweave-cli skill import --from ./downloaded-skill --name downloaded-skill
assetiweave-cli skill mount downloaded-skill --profile codex
assetiweave-cli skill unmount downloaded-skill --profile codex
assetiweave-cli skill delete downloaded-skill --unmount --yes

assetiweave-cli skill group list
assetiweave-cli skill group mount <group-id> --profile codex
assetiweave-cli skill group unmount <group-id> --profile codex --yes

assetiweave-cli schema
assetiweave-cli schema skill.import
assetiweave-cli doctor
```

Success responses are JSON envelopes on stdout. Errors are JSON envelopes on
stderr. Mutating commands support `--dry-run`; destructive commands require
`--yes`.

## Full App API Coverage

The ergonomic commands above cover the common Skill workflow. Full desktop App
parity is exposed through the generic API command:

```bash
assetiweave-cli api call <method> --json '<params>'
assetiweave-cli api call <method> --json @params.json
cat params.json | assetiweave-cli api call <method> --json -
```

`assetiweave-cli schema` lists all callable methods. In addition to the
friendly methods, it includes every Tauri command used by the desktop App:
`get_app_overview`, `list_assets`, `create_source`, `update_source`,
`delete_source`, `create_profile`, `update_profile`, `delete_profile`,
`update_navigation_model`, `update_app_shortcuts`, `list_asset_mounts`,
`toggle_asset_mount`, `set_asset_mount`, all Skill group operations,
`create_plan`, `execute_plan`, log commands, and `reveal_path`.

For App command methods, pass the same JSON argument shape that the frontend
uses with `invoke`, for example:

```bash
assetiweave-cli api call list_asset_mounts --json '{"assetId":null}'
assetiweave-cli api call create_profile --json '{"input":{"id":"codex-test","name":"Codex Test","app_kind":"codex","target_paths":["/tmp/codex-skills"],"supported_kinds":["skill"],"deployment_strategy":"symlink_to_source","enabled":true}}'
```

First-version online search is intentionally outside the CLI. An AI agent should
search the web, install or download a skill according to the source site, then
call `assetiweave-cli skill import --from <installed-dir>`.
