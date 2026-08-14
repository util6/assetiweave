#!/usr/bin/env python3
"""Build bounded, host-synthesized AssetIWeave conversation recall evidence."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from typing import Any


CONTRACTS = [
    "conversation.search",
    "conversation.search.incremental",
    "conversation.block.list",
    "conversation.block.get",
]


class RecallError(RuntimeError):
    def __init__(self, message: str, *, command: list[str] | None = None, detail: Any = None):
        super().__init__(message)
        self.command = command
        self.detail = detail


def cli_path() -> str:
    configured = os.environ.get("ASSETIWEAVE_CLI")
    if configured:
        return configured
    if shutil.which("assetiweave-cli"):
        return "assetiweave-cli"
    if shutil.which("aiwc"):
        return "aiwc"
    return "assetiweave-cli"


def call_cli(arguments: list[str]) -> dict[str, Any]:
    command = [cli_path(), *arguments]
    try:
        completed = subprocess.run(command, capture_output=True, text=True, check=False)
    except OSError as error:
        raise RecallError(
            f"failed to start AssetIWeave CLI: {error}",
            command=command,
        ) from error
    stdout = completed.stdout.strip()
    try:
        payload = json.loads(stdout) if stdout else None
    except json.JSONDecodeError as error:
        raise RecallError(
            "AssetIWeave CLI returned non-JSON output",
            command=command,
            detail={"stdout": stdout[:2_000], "stderr": completed.stderr[:2_000]},
        ) from error
    if completed.returncode != 0 or not isinstance(payload, dict) or not payload.get("ok"):
        response_error = payload.get("error", {}) if isinstance(payload, dict) else {}
        message = response_error.get("message") or completed.stderr.strip() or "AssetIWeave CLI command failed"
        raise RecallError(message, command=command, detail=response_error or None)
    return payload


def doctor() -> dict[str, Any]:
    version = call_cli(["version"]).get("data", {})
    if version.get("compatible") is False:
        raise RecallError("AssetIWeave CLI and Engine report incompatible protocol contracts")
    contracts = []
    for method in CONTRACTS:
        value = call_cli(["schema", method]).get("data", {})
        if value.get("method") != method:
            raise RecallError(f"AssetIWeave contract is missing: {method}")
        contracts.append(method)
    call_cli(["conversation", "session", "list", "--limit", "1"])
    return {
        "ready": True,
        "cli": cli_path(),
        "cli_version": version.get("cli_version"),
        "engine_version": version.get("engine_version"),
        "compatible": version.get("compatible", True),
        "contracts": contracts,
    }


def search_arguments(
    args: argparse.Namespace,
    query: str,
    record_kind: str,
    *,
    incremental: bool = False,
) -> list[str]:
    question_limit = 100 if args.session else args.question_limit_per_query
    command = ["conversation", "search"]
    if incremental:
        command.extend(["incremental", "--recent-runs", str(args.recent_sync_runs)])
    command.extend(
        [
            "--query",
            query,
            "--record-kind",
            record_kind,
            "--kind",
            "question",
            "--limit",
            str(question_limit),
        ]
    )
    if not incremental:
        command.extend(["--format", "json"])
    if args.current_project:
        command.append("--current-project")
    elif args.project:
        command.extend(["--project", args.project])
    if args.adapter:
        command.extend(["--adapter", args.adapter])
    if args.source:
        command.extend(["--source", args.source])
    if args.since and not incremental:
        command.extend(["--since", args.since])
    if args.until and not incremental:
        command.extend(["--until", args.until])
    return command


def record_kinds(args: argparse.Namespace) -> list[str]:
    if args.session and args.session.strip().lower().startswith("web-record-session-"):
        return ["web"]
    if args.record_kind == "both":
        if args.project or args.current_project:
            return ["session"]
        return ["session", "web"]
    return [args.record_kind]


def is_internal_hit(hit: dict[str, Any]) -> bool:
    title = str(hit.get("question_title") or "").strip().lower()
    snippet = str(hit.get("snippet") or "").lower()
    return (
        title.startswith("<codex_internal_context")
        or title.startswith("<system")
        or "<codex_internal_context source=\"goal\">" in snippet
    )


def session_item(hit: dict[str, Any]) -> dict[str, Any]:
    value = hit.get("session")
    return value if isinstance(value, dict) else {}


def session_id_fragment(value: str) -> str:
    normalized = value.strip().lower()
    for prefix in ("conversation-session-", "web-record-session-"):
        if normalized.startswith(prefix):
            normalized = normalized[len(prefix):]
            break
    return normalized[:8]


def matches_session(hit: dict[str, Any], locator: str) -> bool:
    session_id = str(session_item(hit).get("id") or hit.get("session_id") or "")
    return bool(session_id) and session_id_fragment(session_id) == session_id_fragment(locator)


def session_matched_queries(hit: dict[str, Any], searches: list[str]) -> list[str]:
    text = "\n".join(
        [
            str(hit.get("question_title") or ""),
            str(hit.get("snippet") or ""),
        ]
    ).lower()
    return [query for query in searches if query.strip().lower() in text]


def question_rank(hit: dict[str, Any]) -> tuple[int, int, int, int, str]:
    relevance = (
        int(hit.get("_session_relevance") or 0)
        if "_session_relevance" in hit
        else len(hit.get("_matched_queries") or [])
    )
    lane_priority = 0 if hit.get("_lane") == "incremental" else 1
    score = int(hit.get("score") or 0)
    question_index = int(hit.get("question_index") or 0)
    return -relevance, lane_priority, -score, question_index, str(hit.get("block_id") or "")


def search_question_hits(args: argparse.Namespace) -> tuple[list[dict[str, Any]], dict[str, Any], list[str]]:
    ranking_searches = args.search or [args.query]
    searches = [args.session] if args.session else ranking_searches
    requests: list[dict[str, Any]] = []
    all_hits: list[dict[str, Any]] = []
    warnings: list[str] = []
    errors: list[RecallError] = []
    discarded_internal = 0
    request_lanes: list[tuple[str, bool]] = []
    if args.recent_sync_runs and not args.session:
        request_lanes.append(("incremental", True))
    request_lanes.append(("historical", False))
    for lane, incremental in request_lanes:
        for query in searches:
            for record_kind in record_kinds(args):
                try:
                    payload = call_cli(search_arguments(args, query, record_kind, incremental=incremental))
                except RecallError as error:
                    errors.append(error)
                    warnings.append(f"{lane} question search failed for {record_kind}:{query}: {error}")
                    continue
                data = payload.get("data", {})
                hits = data.get("hits") or []
                requests.append(
                    {
                        "lane": lane,
                        "query": query,
                        "record_kind": record_kind,
                        "backend": data.get("backend") or "unknown",
                        "total_question_hits": int(data.get("total_count") or 0),
                        "returned_question_hits": len(hits),
                    }
                )
                if int(data.get("total_count") or 0) > len(hits):
                    if args.session:
                        warnings.append(
                            f"{lane} Session question lookup was truncated at 100 hits for "
                            f"{record_kind}:{query}"
                        )
                    else:
                        warnings.append(
                            f"{lane} question search page was truncated for {record_kind}:{query}; "
                            "narrow the query or raise --question-limit-per-query"
                        )
                if data.get("backend") == "legacy_scan":
                    warnings.append(f"{record_kind}:{query} used legacy_scan because the search index was unavailable or stale")
                for raw_hit in hits:
                    if not isinstance(raw_hit, dict):
                        continue
                    hit = dict(raw_hit)
                    hit["_query"] = query
                    hit["_lookup_query"] = query
                    hit["_matched_queries"] = [query]
                    hit["_record_kind"] = record_kind
                    hit["_lane"] = lane
                    if args.session and not matches_session(hit, args.session):
                        continue
                    if args.session:
                        hit["_matched_queries"] = session_matched_queries(hit, ranking_searches)
                        hit["_session_relevance"] = len(hit["_matched_queries"])
                    if is_internal_hit(hit):
                        discarded_internal += 1
                        continue
                    if not hit.get("question_id") or not hit.get("block_id"):
                        warnings.append(f"ignored question search hit without stable locators: {query}")
                        continue
                    all_hits.append(hit)
    if not requests and errors:
        raise errors[0]

    unique: dict[tuple[str, str], dict[str, Any]] = {}
    for hit in all_hits:
        key = (str(hit.get("_record_kind")), str(hit.get("block_id")))
        current = unique.get(key)
        if current is None:
            unique[key] = hit
            continue
        matched_queries = list(
            dict.fromkeys([*(current.get("_matched_queries") or []), *(hit.get("_matched_queries") or [])])
        )
        preferred = hit if question_rank(hit) < question_rank(current) else current
        merged = dict(preferred)
        merged["_matched_queries"] = matched_queries
        if current.get("_lane") == "incremental" or hit.get("_lane") == "incremental":
            merged["_lane"] = "incremental"
            incremental_hit = current if current.get("_lane") == "incremental" else hit
            merged["incremental"] = incremental_hit.get("incremental")
        merged["score"] = max(int(current.get("score") or 0), int(hit.get("score") or 0))
        unique[key] = merged
    ranked = sorted(unique.values(), key=question_rank)
    resolved_session_ids = sorted(
        {
            str(session_item(hit).get("id") or hit.get("session_id"))
            for hit in ranked
            if session_item(hit).get("id") or hit.get("session_id")
        }
    )
    if args.session and len(resolved_session_ids) > 1:
        raise RecallError(
            f"session locator {args.session!r} is ambiguous: {len(resolved_session_ids)} sessions matched"
        )
    if args.session and not resolved_session_ids:
        raise RecallError(f"no Session matched locator {args.session!r}")
    coverage = {
        "search_requests": requests,
        "search_request_count": len(requests),
        "total_question_hits": sum(item["total_question_hits"] for item in requests),
        "returned_question_hits": sum(item["returned_question_hits"] for item in requests),
        "unique_question_blocks": len(ranked),
        "unique_questions": len({(hit.get("_record_kind"), hit.get("question_id")) for hit in ranked}),
        "discarded_internal_hits": discarded_internal,
        "backends": sorted({item["backend"] for item in requests}),
        "incremental_search_requests": sum(item["lane"] == "incremental" for item in requests),
        "historical_search_requests": sum(item["lane"] == "historical" for item in requests),
        "search_hits_truncated": any(
            item["total_question_hits"] > item["returned_question_hits"] for item in requests
        ),
        "resolved_session_ids": resolved_session_ids,
    }
    return ranked, coverage, warnings


def normalized_text(value: Any) -> str:
    if value is None:
        return ""
    if not isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    text = value.strip()
    if not text.startswith("["):
        return text
    try:
        decoded = json.loads(text)
    except json.JSONDecodeError:
        return text
    if not isinstance(decoded, list):
        return text
    pieces = []
    for item in decoded:
        if isinstance(item, dict) and isinstance(item.get("text"), str):
            pieces.append(item["text"])
    return "\n".join(pieces).strip() or text


def block_semantic_role(block: dict[str, Any]) -> str:
    semantic_role = str(block.get("semantic_role") or "").strip().lower()
    if semantic_role:
        return semantic_role
    renderer = str(block.get("renderer") or "").strip().lower()
    if renderer == "diff":
        return "file-change"
    kind = str(block.get("kind") or "").strip().lower()
    return kind.rsplit(".", 1)[-1]


def result_outcome(block: dict[str, Any]) -> str | None:
    if block_semantic_role(block) != "result":
        return None
    exit_code = block.get("exit_code")
    status = str(block.get("status") or "").strip().lower()
    if isinstance(exit_code, int) and exit_code != 0:
        return "failure"
    if status in {"failed", "failure", "error", "cancelled", "canceled", "timed_out", "timeout"}:
        return "failure"
    if exit_code == 0 or status in {"completed", "success", "succeeded"}:
        return "success"
    return "unknown"


def evidence_tier(block: dict[str, Any]) -> str:
    role = block_semantic_role(block)
    if role == "question":
        return "question-context"
    if role in {"answer", "reasoning"}:
        return "answer"
    if role in {"file-change", "file_change", "diff", "code"}:
        return "change"
    if role == "command":
        return "command"
    if role == "result" and result_outcome(block) == "failure":
        return "failure"
    return "context"


def related_block_rank(block: dict[str, Any], index: int) -> tuple[int, int]:
    tier_order = {
        "question-context": 0,
        "answer": 1,
        "change": 2,
        "command": 3,
        "failure": 4,
        "context": 5,
    }
    return tier_order[evidence_tier(block)], index


def compact_block_locator(block: dict[str, Any]) -> dict[str, Any]:
    compact = {
        key: block.get(key)
        for key in (
            "block_id",
            "turn_id",
            "part_id",
            "kind",
            "semantic_role",
            "renderer",
            "role",
            "content_length",
            "status",
            "exit_code",
        )
        if block.get(key) is not None
    }
    compact["evidence_tier"] = evidence_tier(block)
    outcome = result_outcome(block)
    if outcome is not None:
        compact["result_outcome"] = outcome
    return compact


def prioritize_related_blocks(
    raw_blocks: list[Any],
    selected_block_id: str,
    limit: int,
) -> tuple[list[dict[str, Any]], dict[str, int], dict[str, int], bool]:
    eligible: list[tuple[int, dict[str, Any]]] = []
    suppressed: dict[str, int] = {}
    tier_counts: dict[str, int] = {}
    for index, raw_block in enumerate(raw_blocks):
        if not isinstance(raw_block, dict) or raw_block.get("block_id") == selected_block_id:
            continue
        if block_semantic_role(raw_block) == "result" and result_outcome(raw_block) == "success":
            suppressed["successful_result"] = suppressed.get("successful_result", 0) + 1
            continue
        tier = evidence_tier(raw_block)
        tier_counts[tier] = tier_counts.get(tier, 0) + 1
        eligible.append((index, raw_block))
    eligible.sort(key=lambda item: related_block_rank(item[1], item[0]))
    selected = [compact_block_locator(block) for _, block in eligible[:limit]]
    return selected, tier_counts, suppressed, len(eligible) > len(selected)


def fit_locator_budget(
    blocks: list[dict[str, Any]],
    remaining_chars: int,
) -> tuple[list[dict[str, Any]], int, bool]:
    selected: list[dict[str, Any]] = []
    used_chars = 0
    for block in blocks:
        encoded_length = len(json.dumps(block, ensure_ascii=False, separators=(",", ":")))
        if encoded_length > remaining_chars - used_chars:
            return selected, used_chars, True
        selected.append(block)
        used_chars += encoded_length
    return selected, used_chars, False


def selected_question_evidence(
    args: argparse.Namespace,
    hits: list[dict[str, Any]],
) -> tuple[list[dict[str, Any]], dict[str, Any], list[str]]:
    evidence: list[dict[str, Any]] = []
    warnings: list[str] = []
    used_chars = 0
    question_reads = 0
    block_locator_lists = 0
    related_block_locators = 0
    returned_related_block_locators = 0
    related_block_output_chars = 0
    questions_with_truncated_locators = 0
    suppressed_successful_results = 0
    truncated = False
    for hit in hits:
        if len(evidence) >= args.max_evidence or used_chars >= args.max_chars:
            truncated = True
            break
        block_id = str(hit["block_id"])
        question_id = str(hit["question_id"])
        try:
            detail = call_cli(["conversation", "block", "get", block_id]).get("data", {})
            question_reads += 1
            content = normalized_text(detail.get("content"))
        except RecallError as error:
            warnings.append(f"failed to read question block {block_id}: {error}")
            detail = {}
            content = normalized_text(hit.get("snippet"))
        if not content:
            continue
        related_blocks: list[dict[str, Any]] = []
        related_block_counts: dict[str, int] = {}
        related_blocks_suppressed: dict[str, int] = {}
        related_blocks_truncated = False
        try:
            raw_blocks = call_cli(["conversation", "block", "list", question_id]).get("data", [])
            block_locator_lists += 1
            if isinstance(raw_blocks, list):
                related_blocks, related_block_counts, related_blocks_suppressed, related_blocks_truncated = (
                    prioritize_related_blocks(raw_blocks, block_id, args.max_related_blocks_per_question)
                )
                related_block_locators += sum(related_block_counts.values())
                related_blocks, locator_chars, locator_budget_truncated = fit_locator_budget(
                    related_blocks,
                    max(0, args.max_locator_chars - related_block_output_chars),
                )
                related_block_output_chars += locator_chars
                related_blocks_truncated = related_blocks_truncated or locator_budget_truncated
                returned_related_block_locators += len(related_blocks)
                suppressed_successful_results += related_blocks_suppressed.get("successful_result", 0)
                if related_blocks_truncated:
                    questions_with_truncated_locators += 1
        except RecallError as error:
            warnings.append(f"failed to list related blocks for {question_id}: {error}")
        remaining = args.max_chars - used_chars
        cap = min(args.max_card_chars, remaining)
        clipped = content[:cap]
        content_truncated = len(content) > len(clipped)
        truncated = truncated or content_truncated
        used_chars += len(clipped)
        session = session_item(hit)
        evidence.append(
            {
                "id": f"question-{len(evidence)}",
                "record_kind": hit.get("_record_kind") or "session",
                "source_id": session.get("source_id"),
                "adapter_id": session.get("adapter_id"),
                "session_id": session.get("id"),
                "session_title": session.get("title"),
                "project_path": session.get("project_path"),
                "event_time": session.get("started_at") or session.get("updated_at") or session.get("imported_at"),
                "question_id": question_id,
                "question_index": hit.get("question_index"),
                "question_title": hit.get("question_title"),
                "turn_id": detail.get("turn_id") or hit.get("turn_id"),
                "block_id": block_id,
                "kind": detail.get("kind") or "question",
                "lookup_query": hit.get("_lookup_query") or hit.get("_query"),
                "matched_query": (hit.get("_matched_queries") or [hit.get("_query")])[0],
                "matched_queries": hit.get("_matched_queries") or [],
                "recall_lane": hit.get("_lane") or "historical",
                "incremental": hit.get("incremental"),
                "score": hit.get("score"),
                "snippet": normalized_text(hit.get("snippet"))[:1_000],
                "content": clipped,
                "content_truncated": content_truncated,
                "related_blocks": related_blocks,
                "related_block_counts": related_block_counts,
                "related_blocks_suppressed": related_blocks_suppressed,
                "related_blocks_truncated": related_blocks_truncated,
            }
        )
    return evidence, {
        "question_block_reads": question_reads,
        "block_locator_lists": block_locator_lists,
        "related_block_locators": related_block_locators,
        "returned_related_block_locators": returned_related_block_locators,
        "related_block_output_char_count": related_block_output_chars,
        "questions_with_truncated_locators": questions_with_truncated_locators,
        "suppressed_successful_results": suppressed_successful_results,
        "candidate_question_count": len(evidence),
        "output_char_count": used_chars,
        "truncated": truncated or len(evidence) < len(hits),
    }, warnings


def recall(args: argparse.Namespace) -> dict[str, Any]:
    hits, coverage, warnings = search_question_hits(args)
    evidence, hydration, hydration_warnings = selected_question_evidence(args, hits)
    coverage.update(hydration)
    coverage["truncated"] = bool(coverage.get("truncated") or coverage.get("search_hits_truncated"))
    warnings.extend(hydration_warnings)
    return {
        "schema_version": 3,
        "mode": "host_synthesized_recall",
        "phase": "question_discovery",
        "query": args.query,
        "query_variants": args.search or [args.query],
        "scope": {
            "project": str(Path.cwd()) if args.current_project else args.project,
            "adapter": args.adapter,
            "source": args.source,
            "record_kind": args.record_kind,
            "since": args.since,
            "until": args.until,
            "recent_sync_runs": 0 if args.session else args.recent_sync_runs,
            "session": args.session,
        },
        "persistable": False,
        "insufficient_evidence": not evidence,
        "coverage": coverage,
        "warnings": list(dict.fromkeys(warnings)),
        "evidence": evidence,
    }


def read_blocks(args: argparse.Namespace) -> dict[str, Any]:
    evidence: list[dict[str, Any]] = []
    warnings: list[str] = []
    used_chars = 0
    truncated = False
    for block_id in dict.fromkeys(args.block):
        if len(evidence) >= args.max_evidence or used_chars >= args.max_chars:
            truncated = True
            break
        try:
            detail = call_cli(["conversation", "block", "get", block_id]).get("data", {})
        except RecallError as error:
            warnings.append(f"failed to read block {block_id}: {error}")
            continue
        content = normalized_text(detail.get("content"))
        tier = evidence_tier(detail)
        if not content and tier != "failure":
            continue
        remaining = args.max_chars - used_chars
        cap = min(args.max_card_chars, remaining)
        clipped = content[:cap]
        content_truncated = len(content) > len(clipped)
        truncated = truncated or content_truncated
        used_chars += len(clipped)
        evidence.append(
            {
                "id": f"block-{len(evidence)}",
                "record_kind": detail.get("record_kind"),
                "session_id": detail.get("session_id"),
                "question_id": detail.get("question_id"),
                "turn_id": detail.get("turn_id"),
                "part_id": detail.get("part_id"),
                "block_id": detail.get("block_id") or block_id,
                "kind": detail.get("kind"),
                "semantic_role": detail.get("semantic_role"),
                "renderer": detail.get("renderer"),
                "status": detail.get("status"),
                "exit_code": detail.get("exit_code"),
                "evidence_tier": tier,
                "result_outcome": result_outcome(detail),
                "content": clipped,
                "content_truncated": content_truncated,
            }
        )
    return {
        "schema_version": 3,
        "mode": "host_synthesized_recall",
        "phase": "selected_block_read",
        "persistable": False,
        "insufficient_evidence": not evidence,
        "coverage": {
            "requested_block_count": len(dict.fromkeys(args.block)),
            "evidence_count": len(evidence),
            "output_char_count": used_chars,
            "truncated": truncated,
        },
        "warnings": warnings,
        "evidence": evidence,
    }


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    commands = root.add_subparsers(dest="command", required=True)
    commands.add_parser("doctor", help="check the installed CLI, Engine, and read contracts")
    recall_parser = commands.add_parser("recall", help="discover and read bounded question cards for host selection")
    recall_parser.add_argument("--query", required=True, help="the original user question")
    recall_parser.add_argument("--search", action="append", default=[], help="short lexical query; repeat 2-4 times")
    scope = recall_parser.add_mutually_exclusive_group()
    scope.add_argument("--current-project", action="store_true")
    scope.add_argument("--project")
    scope.add_argument(
        "--session",
        help="target one Session via its 8-character display ID or full stable ID; skips incremental search",
    )
    recall_parser.add_argument("--adapter")
    recall_parser.add_argument("--source")
    recall_parser.add_argument("--record-kind", choices=["session", "web", "both"], default="session")
    recall_parser.add_argument("--since")
    recall_parser.add_argument("--until")
    recall_parser.add_argument(
        "--recent-sync-runs",
        type=int,
        default=3,
        help="search this many recent delta-bearing sync runs before historical recall; use 0 to disable",
    )
    recall_parser.add_argument(
        "--question-limit-per-query",
        type=int,
        default=24,
        help="expanded question-card search limit for each query and lane",
    )
    recall_parser.add_argument("--max-evidence", type=int, default=8, help="maximum question cards to read")
    recall_parser.add_argument("--max-chars", type=int, default=20_000)
    recall_parser.add_argument("--max-card-chars", type=int, default=6_000)
    recall_parser.add_argument(
        "--max-related-blocks-per-question",
        type=int,
        default=16,
        help="maximum prioritized related Block locators returned for each question",
    )
    recall_parser.add_argument(
        "--max-locator-chars",
        type=int,
        default=12_000,
        help="maximum JSON characters used by related Block locator metadata",
    )

    read_parser = commands.add_parser("read", help="read exact content only for host-selected block IDs")
    read_parser.add_argument("--block", action="append", required=True, help="Block ID from recall related_blocks; repeat as needed")
    read_parser.add_argument("--max-evidence", type=int, default=8, help="maximum selected blocks to read")
    read_parser.add_argument("--max-chars", type=int, default=20_000)
    read_parser.add_argument("--max-card-chars", type=int, default=6_000)
    return root


def validate_args(args: argparse.Namespace) -> None:
    for name, upper in [
        ("max_evidence", 24),
        ("max_chars", 60_000),
        ("max_card_chars", 12_000),
    ]:
        value = getattr(args, name, 1)
        if value < 1 or value > upper:
            raise RecallError(f"--{name.replace('_', '-')} must be between 1 and {upper}")
    if args.command == "read":
        if len(args.block) > 24:
            raise RecallError("--block may be repeated at most 24 times")
        if any(not value.strip() or len(value) > 512 for value in args.block):
            raise RecallError("each --block value must contain between 1 and 512 characters")
        return
    if args.command != "recall":
        return
    if not args.query.strip() or len(args.query) > 2_000:
        raise RecallError("--query must contain between 1 and 2000 characters")
    if len(args.search) > 6:
        raise RecallError("--search may be repeated at most 6 times")
    for value in args.search:
        if not value.strip() or len(value) > 512:
            raise RecallError("each --search value must contain between 1 and 512 characters")
    if args.recent_sync_runs < 0 or args.recent_sync_runs > 20:
        raise RecallError("--recent-sync-runs must be between 0 and 20")
    if args.question_limit_per_query < 1 or args.question_limit_per_query > 100:
        raise RecallError("--question-limit-per-query must be between 1 and 100")
    if args.max_related_blocks_per_question < 1 or args.max_related_blocks_per_question > 100:
        raise RecallError("--max-related-blocks-per-question must be between 1 and 100")
    if args.max_locator_chars < 100 or args.max_locator_chars > 60_000:
        raise RecallError("--max-locator-chars must be between 100 and 60000")
    if args.session:
        value = args.session.strip().lower()
        is_short_id = len(value) == 8 and all(char in "0123456789abcdef" for char in value)
        prefix = next(
            (prefix for prefix in ("conversation-session-", "web-record-session-") if value.startswith(prefix)),
            None,
        )
        stable_hash = value[len(prefix):] if prefix else ""
        is_full_id = bool(prefix) and len(stable_hash) in {32, 64} and all(
            char in "0123456789abcdef" for char in stable_hash
        )
        if not is_short_id and not is_full_id:
            raise RecallError("--session must be an 8-character hexadecimal display ID or a full stable Session ID")


def write(payload: dict[str, Any]) -> None:
    print(json.dumps(payload, ensure_ascii=False, indent=2))


def main() -> int:
    args = parser().parse_args()
    try:
        validate_args(args)
        if args.command == "doctor":
            data = doctor()
        elif args.command == "read":
            data = read_blocks(args)
        else:
            data = recall(args)
        write({"ok": True, "data": data})
        return 0
    except RecallError as error:
        write(
            {
                "ok": False,
                "error": {
                    "type": "assetiweave_recall_error",
                    "message": str(error),
                    "command": error.command,
                    "detail": error.detail,
                    "hint": "Install the current AssetIWeave CLI and Engine together, then retry the Skill doctor.",
                },
            }
        )
        return 3


if __name__ == "__main__":
    raise SystemExit(main())
