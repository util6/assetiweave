# Issue #24 单卡交接模板

```markdown
## Ticket / Status

- Ticket: copy the selected card ID, for example `B2-R04`
- Status: `VERIFIED` | `DRIFT` | `BLOCKED`
- Start revision: paste the SHA recorded before edits
- End revision: paste the SHA after commit
- Commit: paste the SHA and Chinese Conventional Commit subject

## Authority

- Canonical authority after this card: copy the card's authority and name the implemented symbol
- Production consumers switched: list every production entry changed by this card
- Legacy mechanism deleted/delegated: paste the delete query result and disposition
- Retained matches and contract reason: list exact retained matches, or write `none`

## RED / GREEN / DELETE

- RED command and observed failure: paste the exact command and first relevant failure
- GREEN command and result: paste the same command and pass count
- DELETE query and result: paste the card's delete command and output

## Verification

- Target tests: paste each command and passed test count
- Card gate: copy the Gate ID and write PASS
- G-RUST-BASE: PASS
- Warning delta: write the numeric before and after counts

## Scope

- Files changed: list repository-relative paths from `git diff --name-only`
- Pre-existing dirty paths preserved: list the paths recorded at Preflight
- Public contract changed: `no` or exact generated diff
- Migration added: `no` or migration name and reason

## Next

- Next ready ticket: copy the next card ID from `03-ticket-map.md`
- Remaining risk or drift: write the exact risk, or `none`
```

模板中的说明句必须替换为真实证据；不适用项写 `no`，不删除字段。
