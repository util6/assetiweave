#!/usr/bin/env python3
"""Run the unified AssetIWeave Memory read and Recall workflows."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
from typing import Any

CONTRACTS = [
    "memory.recent.list",
    "memory.context.resolve",
    "memory.project.get",
    "memory.recall.search",
    "memory.recall.session.create",
    "memory.recall.session.get",
    "memory.recall.turn.send",
    "memory.recall.turn.cancel",
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
    return shutil.which("assetiweave-cli") or shutil.which("aiwc") or "assetiweave-cli"


def call_cli(arguments: list[str]) -> dict[str, Any]:
    command = [cli_path(), *arguments]
    try:
        completed = subprocess.run(command, capture_output=True, text=True, check=False)
    except OSError as error:
        raise RecallError(f"failed to start AssetIWeave CLI: {error}", command=command) from error
    stdout = completed.stdout.strip()
    try:
        payload = json.loads(stdout) if stdout else None
    except json.JSONDecodeError as error:
        raise RecallError(
            "AssetIWeave CLI returned non-JSON output",
            command=command,
            detail={"stdout": stdout[:2000], "stderr": completed.stderr[:2000]},
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
    return {
        "ready": True,
        "cli": cli_path(),
        "cli_version": version.get("cli_version"),
        "engine_version": version.get("engine_version"),
        "compatible": version.get("compatible", True),
        "contracts": contracts,
    }


def scope_arguments(args: argparse.Namespace) -> dict[str, Any]:
    project = args.project
    if args.current_project:
        project = str(Path.cwd())
    return {
        "app_id": args.app,
        "source_id": args.source,
        "project_path": project,
        "session_id": args.session,
    }


def scope_flags(args: argparse.Namespace) -> list[str]:
    flags: list[str] = []
    if args.current_project:
        flags.append("--current-project")
    elif args.project:
        flags.extend(["--project", args.project])
    if args.app:
        flags.extend(["--app", args.app])
    if args.source:
        flags.extend(["--source", args.source])
    if args.session:
        flags.extend(["--session", args.session])
    return flags


def search(args: argparse.Namespace) -> dict[str, Any]:
    command = [
        "memory", "recall", "search", "--query", args.query,
        "--limit", str(args.limit), "--offset", str(args.offset),
    ]
    return call_cli(command + scope_flags(args)).get("data", {})


def recall(args: argparse.Namespace) -> dict[str, Any]:
    created = call_cli(["memory", "recall", "session", "create"] + scope_flags(args)).get("data", {})
    session_id = created.get("id")
    if not session_id:
        raise RecallError("Recall session response did not include an id")
    state = call_cli([
        "memory", "recall", "turn", "send", session_id, "--query", args.query,
    ]).get("data", created)
    deadline = time.monotonic() + args.timeout
    while time.monotonic() < deadline:
        turns = state.get("turns") or []
        if turns:
            status = turns[-1].get("status")
            if status in {"completed", "failed", "cancelled", "resume_unavailable"}:
                return state
        time.sleep(args.poll)
        state = call_cli([
            "memory", "recall", "session", "get", session_id,
        ]).get("data", state)
    raise RecallError("Recall turn polling timed out", detail={"session_id": session_id})


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    commands = root.add_subparsers(dest="command", required=True)
    commands.add_parser("doctor", help="check the CLI, Engine, and unified Memory contracts")
    for name, help_text in [
        ("search", "run a deterministic Memory search"),
        ("recall", "run one persistent structured Recall turn"),
    ]:
        command = commands.add_parser(name, help=help_text)
        command.add_argument("--query", required=True)
        command.add_argument("--current-project", action="store_true")
        command.add_argument("--project")
        command.add_argument("--app")
        command.add_argument("--source")
        command.add_argument("--session")
        command.add_argument("--limit", type=int, default=24)
        command.add_argument("--offset", type=int, default=0)
        if name == "recall":
            command.add_argument("--poll", type=float, default=0.5)
            command.add_argument("--timeout", type=float, default=120.0)
    return root


def validate(args: argparse.Namespace) -> None:
    if args.command not in {"search", "recall"}:
        return
    if not args.query.strip() or len(args.query) > 4000:
        raise RecallError("--query must contain between 1 and 4000 characters")
    if args.limit < 1 or args.limit > 128 or args.offset < 0:
        raise RecallError("search pagination is outside the supported range")
    if args.command == "recall" and (
        args.poll <= 0 or args.poll > 30 or args.timeout <= 0 or args.timeout > 1800
    ):
        raise RecallError("recall polling values are outside the supported range")


def main() -> int:
    args = parser().parse_args()
    try:
        validate(args)
        if args.command == "doctor":
            result = doctor()
        elif args.command == "search":
            result = search(args)
        else:
            result = recall(args)
        print(json.dumps({"ok": True, "data": result}, ensure_ascii=False, indent=2))
        return 0
    except RecallError as error:
        print(json.dumps({
            "ok": False,
            "error": {
                "type": "assetiweave_memory_error",
                "message": str(error),
                "command": error.command,
                "detail": error.detail,
            },
        }, ensure_ascii=False, indent=2))
        return 3


if __name__ == "__main__":
    raise SystemExit(main())
