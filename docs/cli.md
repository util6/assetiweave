# AssetIWeave CLI

AssetIWeave CLI follows the Feishu/Lark CLI-style split:

```text
AI / user
  -> handwritten shortcuts         source / skill / asset / profile
  -> generated App commands        app <method>, from the Rust contract
  -> raw Engine API                api call <method>
  -> assetiweave-engine   Rust JSON-RPC over stdio
  -> src-tauri/src/service.rs Rust service facade
  -> SQLite / filesystem / symlink targets
```

The Go CLI is only a client. It does not write SQLite directly, copy skills, or
create symlinks. Those operations stay in Rust so the desktop app and CLI share
the same business rules.

## Architecture Parity Snapshot

This assessment compares AssetIWeave CLI against the reusable architecture of
`larksuite/cli` at local reference commit `c000dc3`. The goal is architectural
parity, not copying Lark-specific product domains such as cloud OAuth, OpenAPI
HTTP transport, or Feishu event resources.

| Area | AssetIWeave status | Notes |
| --- | --- | --- |
| Command surface split | Implemented | Handwritten shortcuts, generated App commands, and raw Engine API mirror the Lark shortcut/service/API layering. |
| Shared business core | Implemented | CLI calls the Rust Engine and shared `AppService`; it does not bypass desktop business rules. |
| Command contract generation | Implemented | Rust DTOs drive schema, validation, handler deserialization, and generated CLI flags. |
| Protocol compatibility | Implemented | CLI requests and Engine responses carry protocol and contract versions; `version` remains diagnostic. |
| Runtime policy and confirmation | Implemented | Engine centralizes policy, risk confirmation, parameter validation, handler execution, and invocation metadata. |
| Plugin platform | Mostly implemented | Observer, Wrapper, Lifecycle, Restrict rules, failure policy, version constraints, inventory, and local config are available. |
| Plugin diagnostics | Implemented | `config plugins show` exposes inventory and config key names without leaking config values. |
| User policy pruning | Implemented for CLI plugins | Plugin restrictions and parent aggregation are present; Lark's YAML user policy layer is represented by `ASSETIWEAVE_POLICY_PATH`. |
| Structured error contract | Partial | A typed `errs/` taxonomy now exists; plugin config, CLI-side Engine boundary errors, and local API/App JSON validation errors are typed. Subtype declarations and raw subtype casts are guarded, and legacy `ExitError` construction is frozen by a shrinking source baseline. Engine-returned business errors and promotion are still pending. |
| Bootstrap/global UX flags | Partial | The CLI has a minimal root bootstrap; it lacks Lark-style global option parsing, profile hiding policy, update notices, and richer completion gating. |
| Output adapters | Intentionally minimal | AssetIWeave is currently JSON-first for AI agents; Lark's table/CSV/JQ/color format layer is not copied. |
| Auth, credentials, transport | Product-specific gap | Lark's OAuth/keychain/HTTP transport stack should not be copied directly. AssetIWeave needs equivalent local workspace/profile/Engine endpoint config only if the desktop app requires it. |
| Event runtime | Product-specific gap | Lark's event consume/status/stop runtime maps to Feishu webhooks. AssetIWeave should add a local event or conversation runtime only when the desktop app exposes that workflow. |
| Release/update notices | Missing | Lark has update and skills-drift notices plus self-update support; AssetIWeave currently only builds and tests local binaries. |
| Static architecture gates | Partial | Contract drift and e2e gates exist. Error-contract lint, command metadata lint, and broader release audit are still missing. |

Near-term work should continue migrating legacy error producers into typed
errors, then add bootstrap/global config, release/update diagnostics, and
stricter static gates. Product-specific Lark domains should be translated only
when AssetIWeave has the matching desktop workflow.

## Build

```bash
pnpm engine:build
pnpm cli:contract
pnpm cli:build
pnpm cli:test:e2e
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

`pnpm cli:contract` exports the Rust command registry to
`internal/schema/contract.json`. The Go CLI embeds this file to construct the
generated App command layer. Rust tests fail when the committed contract drifts
from the registry.

Command contract v2 derives parameter types, required fields, enum values, and
nested object schemas from the Rust request DTO bound to each registered
method. The registry only overlays command risk, descriptions, CLI exposure,
accepted aliases, and the executable handler. The same `command!` declaration
uses its bound DTO for schema generation, preflight validation, and typed
handler deserialization. Engine dispatch performs no second string-based method
match, so a registered schema cannot silently route into another handler or
request type.

Every CLI request includes the supported Engine protocol and command-contract
versions. Every Engine response includes the actual versions in `meta`. Normal
commands reject incompatible Engine binaries before dispatch; `version` remains
available as a diagnostic probe:

```bash
assetiweave-cli version
```

The Engine applies one centralized runtime pipeline to every registered method:

```text
command registry lookup
  -> command policy
  -> high-risk confirmation
  -> runtime parameter-contract validation
  -> registered typed handler
  -> shared AppService business operation
  -> postflight hooks and invocation metadata
```

Both success and error envelopes preserve Engine metadata. `meta.invocation`
contains the requested and canonical methods, risk, exposure, outcome, applied
runtime hooks, and duration. This makes denied and invalid requests observable
without opening the database or entering business dispatch.

The Go CLI also has a Lark CLI-style extension platform:

```text
registered plugins
  -> atomic install through extension/platform.Registrar
  -> optional plugin Restrict rules
  -> command-tree policy denial stubs
  -> Observer / Wrapper command hooks
  -> Startup and Shutdown lifecycle hooks
```

Plugins declare a failure policy. `FailOpen` plugins that fail installation are
skipped with a warning. `FailClosed` plugins block startup with a structured
`plugin_install` error. Any plugin that contributes `Registrar.Restrict` must
declare `Capabilities.Restricts=true` and `FailClosed`; mismatches fail closed
so a missing policy plugin cannot silently remove a safety boundary.
External plugins can either implement `platform.Plugin` directly or use
`platform.NewPlugin(...).Observer(...).Wrap(...).On(...).Restrict(...).Build()`
to construct the same contract. Plugins may also declare
`Capabilities.RequiredCLIVersion` or `Builder.RequireCLI`; unsatisfied
constraints follow the plugin failure policy, while malformed constraints fail
closed as invalid plugin capabilities.

Installed plugin metadata is snapshotted at bootstrap and exposed through:

```bash
assetiweave-cli config plugins show
```

The inventory includes plugin name, version, declared capabilities, registered
observers/wrappers/lifecycle hooks, and `Restrict` rules. The diagnostic command
is allowed under CLI plugin policy so operators can inspect a restrictive
plugin even when normal commands are denied.

Plugins can read local configuration during `Install` through
`Registrar.Config()`. The CLI loads this from
`$HOME/.assetiweave-cli/plugins.json`; set `ASSETIWEAVE_CLI_PLUGIN_CONFIG` to
override the path:

```json
{
  "plugins": {
    "audit": {
      "endpoint": "https://example.com",
      "enabled": true,
      "batch_size": 50
    }
  }
}
```

The public config API exposes copied raw JSON plus typed `String`, `Bool`, and
`Int` helpers. `config plugins show` reports only configured key names, never
values. Malformed plugin config fails closed before any registered plugin is
installed; if no plugins are registered, the file is ignored and built-in CLI
commands continue normally.

`Restrict` rules are evaluated against Cobra command metadata generated from the
Rust command contract. Friendly and generated App commands inherit the contract
risk level; raw `api call` is treated as `high-risk-write` because the concrete
Engine method is only known at runtime. A denied command returns
`command_denied` before the Engine is called. Observers still see denied
attempts, but Wrappers are bypassed on denied commands so a plugin cannot
rewrite or suppress the denial. If every runnable command under a parent group
is denied, the parent group is denied too with `all_children_denied`; this keeps
help output and shell completion from advertising empty command groups.

This Go-layer policy is additive only. It can make the CLI stricter for a given
binary or embedding, but it cannot permit an Engine method that Rust policy,
confirmation, or parameter validation would reject.

Set `ASSETIWEAVE_POLICY_PATH` to opt into a fail-closed command policy:

```json
{
  "version": 1,
  "name": "read-mostly-agent",
  "allow": ["overview.*", "profile.*", "skill.*", "schema.*", "system.*"],
  "deny": ["skill.delete", "source.remove"],
  "max_risk": "write"
}
```

Policy globs match both the requested method and its canonical method. Deny
rules take priority, then the allow list and maximum risk are evaluated.
Malformed configured policies block normal commands. Diagnostic methods
`system.version`, `schema.list`, `schema.get`, and `doctor.run` remain available
so an invalid policy can be diagnosed and repaired.

Release builds inject the CLI product version. `pnpm cli:test:e2e` runs the
compiled CLI and Engine together and verifies product-version alignment,
protocol compatibility, generated App commands, runtime parameter validation,
command policy, invocation metadata, and high-risk confirmation.

## Commands

```bash
assetiweave-cli overview
assetiweave-cli version
assetiweave-cli source list
assetiweave-cli source add --name LocalSkills --path ./skills --dry-run
assetiweave-cli source scan --kind skill
assetiweave-cli profile list
assetiweave-cli asset list --kind skill

assetiweave-cli skill list
assetiweave-cli skill import --from ./downloaded-skill --name downloaded-skill
assetiweave-cli skill backup <asset-id>
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

Stable exit codes for automation:

| Exit code | Meaning |
| --- | --- |
| `0` | Success |
| `2` | CLI or Engine parameter validation failed |
| `3` | Engine process, protocol, or operation failure |
| `5` | Internal CLI failure |
| `6` | Command denied or configured policy invalid |
| `10` | Explicit high-risk confirmation required |

## Generated App Commands

Every desktop App command is exposed as a generated typed command:

```bash
assetiweave-cli app list-profiles
assetiweave-cli app create-profile --input @profile.json
assetiweave-cli app delete-source --id <source-id> --yes
assetiweave-cli app execute-plan --plan @plan.json --action-ids '["action-id"]' --yes
```

Generated scalar parameters become typed flags. Object and array parameters
accept inline JSON, `@file`, or `-` for stdin. The command registry supplies the
parameter schema, description, risk level, dry-run support, and confirmation
policy.

## Full App API Coverage

The ergonomic commands above cover the common Skill workflow. Full desktop App
parity is exposed through the generic API command:

```bash
assetiweave-cli api call <method> --json '<params>'
assetiweave-cli api call <method> --json @params.json
cat params.json | assetiweave-cli api call <method> --json -
```

Raw API params must be a JSON object. High-risk methods are rejected by the
Rust Engine unless the request explicitly includes confirmation. Prefer the
CLI flag so the confirmation is visible:

```bash
assetiweave-cli api call delete_source --json '{"id":"source-id"}' --yes
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
call `assetiweave-cli skill import --from <installed-dir>`. Existing local
skills can be copied into the backup library with `assetiweave-cli skill backup`.

## Adding App Operations

When adding or changing an App operation:

1. Put shared business behavior behind `AppService`.
2. Register the Tauri handler.
3. Derive `Deserialize` and `JsonSchema` for its Rust request DTO.
4. Bind that DTO, accurate risk metadata, and the typed `AppService` handler in
   `src-tauri/src/command_registry.rs`; field types, required fields, enums,
   nested schemas, Engine dispatch, and generated App CLI flags all come from
   this registration.
5. Run `pnpm cli:contract`.
6. Run `go test ./...`, `cargo test --workspace`, and `pnpm cli:test:e2e`.

The Rust tests compare frontend invokes, Tauri handlers, executable registry
entries, and the committed Go contract. Any missing synchronization fails CI.
