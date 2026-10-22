# conversation

Manage normalized AI conversation records, dialogue turn trees, and memory cards.

### Use this when
- Exploring dialogue history, investigating previous architectural decisions, or retrieving past troubleshooting sessions.
- Inspecting conversation hierarchy (sessions, turns, cards) through compact outlines.
- Synchronizing recent chat sessions from local AI assistants and IDE plugins.

### Avoid when
- Managing mounted prompt/rule/skill assets directly (use `aiwc skill` or `aiwc asset` instead).
- Managing long-term memory digests or recall queries directly (use `aiwc memory` instead).

### Examples
- `aiwc conversation list --limit 10`
- `aiwc conversation outline <SESSION_ID>`
- `aiwc conversation search --query "tantivy index"`
- `aiwc conversation sync`

### Related skills
- `aiwc skill builtin read assetiweave-conversation-recall`
- `aiwc skill builtin read assetiweave-conversation-organizer`
- `aiwc skill builtin read assetiweave-memory`

### Risk
Read-only by default across inspection commands (`list`, `search`, `outline`, `card`). Stateful background modifications are restricted to `sync`, `source sync`, and `data repair`.

## search

Search normalized conversation cards for memory retrieval.

### Use this when
- Searching past conversations by keyword, concept, or natural language query.
- Locating specific discussions, decisions, code snippets, or error traces across all indexed sessions.
- Filtering dialogue cards by time window (`--since`, `--until`) or record kinds (`--kinds card`).

### Avoid when
- You already have a session ID, turn ID, or card ID and want the structure (use `aiwc conversation outline <ID>` instead).
- You want to fetch the exact card body without searching (use `aiwc conversation card get <CARD_ID>`).
- Dumping an entire session's messages (use `aiwc conversation outline` to inspect structure first to avoid token waste).

### Examples
- `aiwc conversation search --query "tantivy index"`
- `aiwc conversation search --query "login issue" --limit 5`
- `aiwc conversation search --query "refactor" --since "2026-10-01" --timeline`

### Related skills
- `aiwc skill builtin read assetiweave-conversation-recall`
- `aiwc skill builtin read assetiweave-memory`

### Risk
Read-only: Safe, queries Tantivy index and SQLite cache without side effects or mutations.

## list

List normalized conversation sessions.

### Use this when
- Browsing recent conversation sessions across local IDEs and agents.
- Filtering dialogue history by time window (`--window 24h`, `--since yesterday`), project path, or adapter.
- Discovering session IDs to pass to `outline`.

### Avoid when
- Searching for specific keywords or message contents across conversations (use `aiwc conversation search` instead).
- Inspecting turns or cards within a known session (use `aiwc conversation outline <SESSION_ID>` instead).

### Examples
- `aiwc conversation list --limit 10`
- `aiwc conversation list --window 24h`
- `aiwc conversation list --current-project`
- `aiwc conversation list --adapter codex`

### Related skills
- `aiwc skill builtin read assetiweave-conversation-recall`
- `aiwc skill builtin read assetiweave-conversation-organizer`

### Risk
Read-only: Safe, queries existing SQLite index without modifying state.

## outline

Inspect conversation structure with compact columnar outline.

### Use this when
- Gaining an overview of conversation turns, participants, and cards with minimal token overhead.
- Navigating hierarchical conversation structure using any identifier (session ID, turn ID, or card ID / 8-hex prefix).
- Identifying target card IDs before pulling detailed message contents.

### Avoid when
- Broadly discovering unknown sessions or searching by topic keyword (use `aiwc conversation list` or `aiwc conversation search` first).
- You already know the exact card ID and only need its raw text (use `aiwc conversation card get <CARD_ID> --raw`).

### Examples
- `aiwc conversation outline <SESSION_ID>`
- `aiwc conversation outline <TURN_ID>`
- `aiwc conversation outline <CARD_ID>`

### Related skills
- `aiwc skill builtin read assetiweave-conversation-recall`
- `aiwc skill builtin read assetiweave-conversation-organizer`

### Risk
Read-only: Safe, performs matrix projection on SQLite store without side effects.

## card

Inspect and read individual conversation cards.

### Use this when
- Inspecting or extracting the exact message content/payload of a specific conversation card.
- Outputting raw card markdown/text directly (`--raw`) to pass into prompt context.

### Avoid when
- Browsing conversation turn sequences or parent-child hierarchy (use `aiwc conversation outline` instead).
- Searching for cards by keywords (use `aiwc conversation search` instead).

### Examples
- `aiwc conversation card get <CARD_ID>`
- `aiwc conversation card get <CARD_ID> --raw`

### Related skills
- `aiwc skill builtin read assetiweave-conversation-recall`

### Risk
Read-only: Safe, returns exact card content without mutation.

## card get

Get exact content for one conversation card.

### Use this when
- Extracting the complete raw or formatted message body of a known conversation card.
- Passing verbatim dialogue turns or card snippets into another AI agent.

### Avoid when
- You do not have the card ID or hex prefix (use `aiwc conversation outline` or `aiwc conversation search` first).

### Examples
- `aiwc conversation card get <CARD_ID>`
- `aiwc conversation card get <CARD_ID> --raw`

### Related skills
- `aiwc skill builtin read assetiweave-conversation-recall`

### Risk
Read-only: Safe, returns exact card record content without side effects.

## sync

Incrementally synchronize conversations from local adapters.

### Use this when
- Pulling and normalizing the latest conversation sessions from local IDE / agent adapters into AssetIWeave.
- Updating local SQLite tables and search indexes after new chat turns in external tools.

### Avoid when
- Just inspecting or querying already synchronized conversations (use `list`, `search`, or `outline`).
- Repairing damaged conversation indexes or recovering from corrupted state (use `aiwc conversation data repair`).

### Examples
- `aiwc conversation sync`
- `aiwc conversation sync --adapter codex`
- `aiwc conversation sync --force`

### Related skills
- `aiwc skill builtin read assetiweave-conversation-organizer`
- `aiwc skill builtin read assetiweave-web-conversation-repair`

### Risk
Low to Medium: Updates local SQLite cache and conversation index; does not delete external IDE original history.

## adapter

Manage conversation adapters.

### Use this when
- Inspecting installed conversation adapters (e.g. Codex, OpenCode, Claude Code, Antigravity).
- Checking adapter runtime paths, enabled status, and environment health.

### Avoid when
- Synchronizing conversation sessions (use `aiwc conversation sync` instead).
- Managing target application profiles (use `aiwc profile` instead).

### Examples
- `aiwc conversation adapter list`
- `aiwc conversation adapter inspect codex`

### Related skills
- `aiwc skill builtin read assetiweave-conversation-organizer`

### Risk
Read-only: Safe, inspects adapter manifest and environment paths.

## source

Inspect and sync conversation sources.

### Use this when
- Inspecting registered conversation data sources (file system paths, workspace roots, web harvester exports).
- Triggering synchronization or full reparsing for a specific source ID.

### Avoid when
- Performing standard incremental synchronization across all active adapters (use `aiwc conversation sync`).
- Auditing database integrity across all sources (use `aiwc conversation data audit`).

### Examples
- `aiwc conversation source list`
- `aiwc conversation source sync <SOURCE_ID>`
- `aiwc conversation source sync <SOURCE_ID> --full`

### Related skills
- `aiwc skill builtin read assetiweave-conversation-organizer`

### Risk
Low: Synchronizes conversations from a specific source path into the AssetIWeave store.

## data

Audit and repair normalized conversation data.

### Use this when
- Auditing conversation database integrity and derived search indexes (`data audit`).
- Repairing broken references, normalizing legacy formats, or clearing corrupt entries (`data repair`).
- Rolling back from a previous maintenance backup (`data rollback`).

### Avoid when
- Performing normal queries or daily exploration (use `list`, `search`, `outline`).
- Routine conversation updates (use `aiwc conversation sync`).

### Examples
- `aiwc conversation data audit`
- `aiwc conversation data audit --source <SOURCE_ID>`
- `aiwc conversation data repair --dry-run`
- `aiwc conversation data repair --yes`
- `aiwc conversation data rollback --backup-path <PATH>`

### Related skills
- `aiwc skill builtin read assetiweave-web-conversation-repair`

### Risk
High: `repair` and `rollback` mutate local SQLite tables and conversation indexes. Always execute with `--dry-run` first before applying `--yes`.
