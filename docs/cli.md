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
`larksuite/cli` at remote reference commit `8c3cba1`. The goal is architectural
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
| Structured error contract | Implemented | A typed `cli/errs/` taxonomy covers CLI-local and Engine-returned validation, config, policy, hook, confirmation, business, protocol, and internal failures. Explicit wire types preserve the existing agent-facing `error.type`, exit codes, details, hints, and invocation metadata. Declared subtype, raw subtype cast, and zero-baseline legacy construction gates prevent regression. |
| Command recovery | Implemented | Unknown root/nested commands, unknown or misplaced flags, invalid flag values, missing required flags, and positional argument errors return exit 2 typed validation envelopes with stable command/flag suggestions. Pure command groups remain outside metadata, policy, and plugin invocation semantics. |
| Bootstrap/global UX flags | Implemented | The CLI has root bootstrap, diagnostic bypasses, pre-parsed `--plugin-config`, global `--engine`/`--policy` overrides, completion bootstrap gating, and an opt-in profile command hiding policy for single-profile packaging. |
| Output adapters | Intentionally minimal | AssetIWeave is currently JSON-first for AI agents; Lark's table/CSV/JQ/color format layer is not copied. |
| Skill discovery and acquisition | Initial provider implemented | `skill search` uses provider-based internet discovery, resolves GitHub repositories into concrete `SKILL.md` directories when possible, and `skill acquire` downloads/imports a selected candidate through the shared Rust service. Confirmed acquisitions record remote source metadata for later drift checks. |
| Auth, credentials, transport | Product-specific gap | Lark's OAuth/keychain/HTTP transport stack should not be copied directly. AssetIWeave needs equivalent local workspace/profile/Engine endpoint config only if the desktop app requires it. |
| Event runtime | Product-specific gap | Lark's event consume/status/stop runtime maps to Feishu webhooks. AssetIWeave should add a local event or conversation runtime only when the desktop app exposes that workflow. |
| Release/update notices | Partial | `version` reports CLI release provenance, Engine compatibility, and optional remote updater-manifest diagnostics via `--check-updates`. Release builds inject cached `_notice.update` hints into JSON envelopes when a newer manifest version is known. `update --check` resolves the matching CLI tools release package, and `update --yes` downloads, checksum-verifies, and replaces local CLI tools. `skill remote check` now provides explicit Skill drift status; proactive background Skill notices remain future work. |
| Static architecture gates | Implemented | Contract drift, e2e, declared error subtype, raw subtype cast, zero-baseline legacy error-construction, command metadata, and release audit gates exist. |

Near-term work should add richer provider/ranking hooks and UI drift badges once
AssetIWeave has more external skill-state sources. Product-specific
Lark domains should be translated only when AssetIWeave has the matching desktop
workflow.

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
`cli/internal/schema/contract.json`. The Go CLI embeds this file to construct the
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

Shell completion runs through a lightweight bootstrap path. `completion`,
`__complete`, and `__completeNoDesc` build the command tree and metadata but
skip plugin config loading, plugin install, Startup/Shutdown hooks, command
policy pruning, and update notices so completion output is not corrupted by
runtime diagnostics or network work.

Single-profile builds can set `ASSETIWEAVE_CLI_HIDE_PROFILES=1` to hide the
`profile` command group from help and shell completion while keeping explicit
commands such as `assetiweave-cli profile list` executable for diagnostics and
automation.

CLI syntax failures use the same typed JSON error contract as Engine and plugin
failures. A mistyped nested command no longer falls through to help with exit
0, and Cobra parse errors no longer surface as internal failures. Error details
include `command_path`, the unknown token, available commands or flags, and
ranked suggestions where a plausible match exists.

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

`assetiweave-cli version --check-updates` reads the remote Tauri updater
manifest and reports non-blocking diagnostics under `data.update`. Successful
checks refresh `$HOME/.assetiweave-cli/update-state.json`. Normal release
commands read that cache and inject a structured `_notice.update` object into
success and error JSON envelopes when a newer CLI version is known. The notice
is suppressed for dev builds and CI environments; set
`ASSETIWEAVE_CLI_NO_UPDATE_NOTIFIER=1` to suppress it in other automation.
Tests and isolated runs can override `ASSETIWEAVE_UPDATE_STATE_PATH` and
`ASSETIWEAVE_UPDATE_MANIFEST_URL`.

`assetiweave-cli update --check` uses the same manifest to resolve the GitHub
release URL and the current platform's CLI tools archive, such as
`assetiweave-tools-v0.1.1-macos-arm64.tar.gz`, plus its `.sha256` checksum.
`assetiweave-cli update --yes` downloads both files, verifies SHA256, extracts
the archive into a temporary directory, and replaces `assetiweave-cli` and
`assetiweave-engine` in the running executable's directory with rollback on
installation failure.

## Commands

```bash
assetiweave-cli overview
assetiweave-cli version
assetiweave-cli settings show
assetiweave-cli settings save --json '{"density":"compact"}'
assetiweave-cli update --check
assetiweave-cli update --yes
assetiweave-cli source list
assetiweave-cli source add --name LocalSkills --path ./skills --dry-run
assetiweave-cli source scan --kind skill
assetiweave-cli profile list
assetiweave-cli asset list --kind skill

assetiweave-cli skill list
assetiweave-cli skill import --from ./downloaded-skill --name downloaded-skill
assetiweave-cli skill search --query "browser automation skill"
assetiweave-cli skill acquire --url https://github.com/lackeyjb/playwright-skill/tree/main/skills/playwright-skill --dry-run
assetiweave-cli skill acquire --url https://github.com/lackeyjb/playwright-skill/tree/main/skills/playwright-skill --yes
assetiweave-cli skill remote list
assetiweave-cli skill remote check [asset-id]
assetiweave-cli skill backup <asset-id>
assetiweave-cli skill mount downloaded-skill --profile codex
assetiweave-cli skill unmount downloaded-skill --profile codex
assetiweave-cli skill delete downloaded-skill --unmount --yes

assetiweave-cli skill group list
assetiweave-cli skill group show <group-id>
assetiweave-cli skill group create --name Frontend --path-glob 'frontend/**'
assetiweave-cli skill group members set <group-id> --asset <asset-id>
assetiweave-cli skill group mount <group-id> --profile codex
assetiweave-cli skill group unmount <group-id> --profile codex --yes
assetiweave-cli skill group exclusive preview --group <group-id> --profile codex
assetiweave-cli skill group exclusive apply --group <group-id> --profile codex --yes

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
`toggle_asset_mount`, `set_asset_mount`, `search_skills`, `acquire_skill`,
all Skill group operations,
`create_plan`, `execute_plan`, log commands, and `reveal_path`.

For App command methods, pass the same JSON argument shape that the frontend
uses with `invoke`, for example:

```bash
assetiweave-cli api call list_asset_mounts --json '{"assetId":null}'
assetiweave-cli api call create_profile --json '{"input":{"id":"codex-test","name":"Codex Test","app_kind":"codex","target_paths":["/tmp/codex-skills"],"supported_kinds":["skill"],"deployment_strategy":"symlink_to_source","enabled":true}}'
```

## Internet Skill Discovery And Acquisition

The first provider-backed discovery path is built into the shared Engine so the
desktop app, CLI, and external agents use the same import rules:

```bash
assetiweave-cli skill search --query "browser automation skill" --provider github --limit 5
assetiweave-cli skill search --query "browser automation skill" --provider github-code --limit 5
assetiweave-cli skill acquire --url <github-repo-or-tree-url> --dry-run
assetiweave-cli skill acquire --url <github-repo-or-tree-url> --yes
assetiweave-cli skill remote list
assetiweave-cli skill remote check [asset-id]
```

`skill search --provider github` starts with GitHub repository search and then
inspects each candidate repository tree for `SKILL.md`. When concrete skills
are found, the candidate URL points at the specific GitHub tree path, so it can
be passed directly to `skill acquire`. If tree inspection fails or a repository
does not contain `SKILL.md`, the command falls back to a repository-level
candidate. `skill search --provider github-code` uses GitHub code search with a
`filename:SKILL.md` qualifier to find Skill files directly on default branches.
Each candidate includes `match_reason`; provider, code-search, or
tree-inspection problems are returned in `warnings` so agents can judge whether
the result set is strong or only a degraded fallback.

Unauthenticated GitHub requests work, but public API rate limits are low. Set
`GITHUB_TOKEN` or `GH_TOKEN` to let the Engine use an authenticated request
header. Tokens are read from the process environment and are not written to the
database or CLI output.

`skill acquire --dry-run` plans the clone, staging path, inferred Skill path,
import name, and `security_notice` without writing files. A confirmed acquire
clones the repository into the AssetIWeave staging area, resolves the selected
`SKILL.md` directory, copies it into `~/.assetiweave/library/skills/downloaded`,
registers the AssetIWeave library source, rescans it, returns the imported
asset and the same `security_notice`, and records the GitHub repository, branch,
Skill path, acquired tree SHA, and local content hash as remote-source metadata.
The notice reminds callers to review remote Skill contents before importing;
AssetIWeave does not execute or automatically trust remote code.

`skill remote list` shows acquired Skill remote-source records. `skill remote
check` fetches the current GitHub tree for each record, compares the selected
Skill directory tree SHA with the acquired tree SHA, and returns `current`,
`changed`, `unknown`, or `error` status. Passing an asset id checks only that
Skill. The check persists `last_checked_at`, `latest_tree_sha`, `status`, and
`message` so the desktop app can surface update reminders without reimplementing
provider logic.

This is not a hosted marketplace: AssetIWeave does not curate remote packages
or run an embedded LLM API in v1. It exposes a provider-based Agent chain that
can be driven from UI, CLI, or an external AI workflow.

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
6. Run `pnpm cli:test`, `cargo test --workspace`, and `pnpm cli:test:e2e`.

The Rust tests compare frontend invokes, Tauri handlers, executable registry
entries, and the committed Go contract. Any missing synchronization fails CI.
