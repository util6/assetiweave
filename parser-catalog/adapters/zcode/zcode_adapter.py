#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
import sqlite3
import sys
from pathlib import Path
from typing import Any


IGNORED_PART_TYPES = {
    "compaction",
    "reasoning",
    "retry",
    "snapshot",
    "step-finish",
    "step-start",
}
CONTENT_CARD_SCHEMA_VERSION = "zcode-content-cards-v2"


def emit(payload: dict[str, Any]) -> None:
    print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")), flush=True)


def compact_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


def compact_object(value: dict[str, Any]) -> dict[str, Any]:
    return {key: entry for key, entry in value.items() if entry not in (None, "")}


def content_card_metadata(content_card: dict[str, Any], extra: dict[str, Any] | None = None) -> str:
    metadata = dict(extra or {})
    metadata["content_card"] = compact_object(content_card)
    return compact_json(metadata)


def small_metadata(data: dict[str, Any]) -> dict[str, Any]:
    return compact_object(
        {
            "source_type": data.get("type"),
            "tool": data.get("tool") or data.get("tool_name") or data.get("toolName"),
            "title": data.get("title"),
        }
    )


def source_database(location: str) -> Path:
    path = Path(location).expanduser().resolve()
    if path.is_dir():
        for candidate in (
            path / "db" / "db.sqlite",
            path / "cli" / "db" / "db.sqlite",
            path / "db.sqlite",
        ):
            if candidate.is_file():
                return candidate
    return path


def connect_read_only(path: Path) -> sqlite3.Connection:
    if not path.is_file():
        raise ValueError(f"ZCode SQLite database not found: {path}")
    conn = sqlite3.connect(f"{path.as_uri()}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA query_only = ON")
    return conn


def table_columns(conn: sqlite3.Connection, table: str) -> set[str]:
    return {str(row["name"]) for row in conn.execute(f'PRAGMA table_info("{table}")')}


def validate_schema(conn: sqlite3.Connection) -> None:
    required = {
        "session": {"id", "title", "time_updated"},
        "message": {"id", "session_id", "time_created", "data"},
        "part": {"id", "message_id", "session_id", "time_created", "data"},
    }
    for table, columns in required.items():
        missing = columns - table_columns(conn, table)
        if missing:
            raise ValueError(
                f"ZCode table {table} is missing required columns: {', '.join(sorted(missing))}"
            )


def session_version_token(row: sqlite3.Row) -> str:
    digest = hashlib.sha256()
    digest.update(CONTENT_CARD_SCHEMA_VERSION.encode("utf-8"))
    digest.update(b"\0")
    for key in ("id", "time_updated", "message_marker", "part_marker"):
        digest.update(str(row[key] or "").encode("utf-8"))
        digest.update(b"\0")
    return digest.hexdigest()


def parse_json(text: Any) -> dict[str, Any]:
    if not isinstance(text, str):
        return {}
    try:
        value = json.loads(text)
    except json.JSONDecodeError:
        return {}
    return value if isinstance(value, dict) else {}


def timestamp(value: Any) -> str | None:
    if value is None or isinstance(value, bool):
        return None
    if isinstance(value, (int, float)):
        return str(int(value))
    text = str(value).strip()
    return text or None


def message_timestamp(row: sqlite3.Row, data: dict[str, Any]) -> str | None:
    direct = timestamp(row["time_created"])
    if direct:
        return direct
    time_value = data.get("time")
    if isinstance(time_value, dict):
        return timestamp(time_value.get("created"))
    return timestamp(time_value)


def collect_strings(value: Any, output: list[str]) -> None:
    if isinstance(value, str):
        if value.strip():
            output.append(value)
        return
    if isinstance(value, list):
        for item in value:
            collect_strings(item, output)
        return
    if not isinstance(value, dict):
        return
    if any(value.get(key) is True for key in ("ignored", "synthetic", "isSynthetic", "isMeta")):
        return
    if value.get("type") in IGNORED_PART_TYPES:
        return
    for key in (
        "text",
        "content",
        "output",
        "result",
        "summary",
        "stdout",
        "stderr",
        "preview",
        "message",
        "title",
        "patch",
        "diff",
        "error",
    ):
        if key in value:
            collect_strings(value[key], output)


def tool_text(data: dict[str, Any]) -> str | None:
    values: list[str] = []
    for key in ("output", "result", "content", "summary", "message", "error"):
        if key in data:
            collect_strings(data[key], values)
    state = data.get("state")
    if isinstance(state, dict):
        for key in ("title", "output", "error", "message"):
            if key in state:
                collect_strings(state[key], values)
    text = "\n".join(values).strip()
    return text or None


def nested_string(value: Any, names: tuple[str, ...], depth: int = 0) -> str | None:
    if depth > 8 or not isinstance(value, dict):
        return None
    for name in names:
        candidate = value.get(name)
        if isinstance(candidate, str) and candidate.strip():
            return candidate
    for key in (
        "state",
        "input",
        "tool_input",
        "toolInput",
        "action",
        "request",
        "params",
        "parameters",
    ):
        candidate = nested_string(value.get(key), names, depth + 1)
        if candidate:
            return candidate
    for key in ("arguments", "args"):
        child = value.get(key)
        if isinstance(child, str) and child.lstrip().startswith(("{", "[")):
            try:
                child = json.loads(child)
            except json.JSONDecodeError:
                continue
        candidate = nested_string(child, names, depth + 1)
        if candidate:
            return candidate
    return None


def nested_int(value: Any, names: tuple[str, ...], depth: int = 0) -> int | None:
    if depth > 8 or not isinstance(value, dict):
        return None
    for name in names:
        candidate = value.get(name)
        if isinstance(candidate, int) and not isinstance(candidate, bool):
            return candidate
    for key in ("state", "input", "result", "metadata"):
        candidate = nested_int(value.get(key), names, depth + 1)
        if candidate is not None:
            return candidate
    return None


def normalized_part(
    *,
    role: str,
    kind: str,
    text: str | None = None,
    language: str | None = None,
    command: str | None = None,
    cwd: str | None = None,
    status: str | None = None,
    exit_code: int | None = None,
    metadata_json: str | None = None,
) -> dict[str, Any]:
    return {
        "role": role,
        "kind": kind,
        "text": text,
        "language": language,
        "command": command,
        "cwd": cwd,
        "status": status,
        "exit_code": exit_code,
        "metadata_json": metadata_json,
    }


def split_markdown(role: str, text: str) -> list[dict[str, Any]]:
    parts: list[dict[str, Any]] = []
    remaining = text
    while "```" in remaining:
        before, fenced = remaining.split("```", 1)
        if before.strip():
            parts.append(
                normalized_part(
                    role=role,
                    kind="text",
                    text=before.strip(),
                    metadata_json=(
                        content_card_metadata({"type": "answer", "format": "markdown"})
                        if role == "assistant"
                        else None
                    ),
                )
            )
        if "```" not in fenced:
            tail = f"```{fenced}".strip()
            if tail:
                parts.append(
                    normalized_part(
                        role=role,
                        kind="text",
                        text=tail,
                        metadata_json=(
                            content_card_metadata({"type": "answer", "format": "markdown"})
                            if role == "assistant"
                            else None
                        ),
                    )
                )
            return parts
        body, remaining = fenced.split("```", 1)
        language = None
        code = body
        if "\n" in body:
            first_line, code = body.split("\n", 1)
            language = first_line.strip() or None
        code = code.strip("\n")
        if code.strip():
            parts.append(
                normalized_part(
                    role=role,
                    kind="code_block",
                    text=code,
                    language=language,
                    metadata_json=content_card_metadata(
                        {"type": "code", "language": language}
                    ),
                )
            )
    if remaining.strip():
        parts.append(
            normalized_part(
                role=role,
                kind="text",
                text=remaining.strip(),
                metadata_json=(
                    content_card_metadata({"type": "answer", "format": "markdown"})
                    if role == "assistant"
                    else None
                ),
            )
        )
    return parts


def normalize_assistant_part(data: dict[str, Any]) -> list[dict[str, Any]]:
    kind = str(data.get("type") or "text")
    if kind in IGNORED_PART_TYPES:
        return []
    if kind == "text":
        text = data.get("text")
        return split_markdown("assistant", text) if isinstance(text, str) else []
    if kind in {"tool", "tool-call", "tool-result"}:
        command = nested_string(data, ("command", "cmd", "shell_command"))
        text = tool_text(data)
        cwd = nested_string(
            data,
            ("cwd", "workdir", "working_directory", "workingDirectory"),
        )
        status = nested_string(data, ("status",))
        exit_code = nested_int(data, ("exit_code", "exitCode", "code"))
        parts: list[dict[str, Any]] = []
        if command:
            parts.append(
                normalized_part(
                    role="tool",
                    kind="command",
                    command=command,
                    cwd=cwd,
                    metadata_json=content_card_metadata(
                        {"type": "command", "cwd": cwd},
                        small_metadata(data),
                    ),
                )
            )
            if text:
                parts.append(
                    normalized_part(
                        role="tool",
                        kind="tool",
                        text=text,
                        status=status,
                        exit_code=exit_code,
                        metadata_json=content_card_metadata(
                            {
                                "type": "result",
                                "format": "plain",
                                "status": status,
                                "exit_code": exit_code,
                            },
                            small_metadata(data),
                        ),
                    )
                )
            return parts
        if text:
            return [
                normalized_part(
                    role="tool",
                    kind="tool",
                    text=text,
                    status=status,
                    exit_code=exit_code,
                    metadata_json=content_card_metadata(
                        {
                            "type": "result",
                            "format": "plain",
                            "status": status,
                            "exit_code": exit_code,
                        },
                        small_metadata(data),
                    ),
                )
            ]
        return [
            normalized_part(
                role="tool",
                kind="tool",
                text=(
                    str(data.get("tool") or data.get("tool_name") or data.get("type") or "tool")
                ),
                metadata_json=content_card_metadata(
                    {"type": "tool", "format": "plain"},
                    small_metadata(data),
                ),
            )
        ]
    if kind in {"file", "patch"}:
        text = nested_string(data, ("path", "filename", "name", "text", "summary", "url"))
        return [
            normalized_part(
                role="assistant",
                kind="file_change",
                text=text,
                metadata_json=content_card_metadata(
                    {"type": "result", "format": "plain"},
                    small_metadata(data),
                ),
            )
        ]
    text_values: list[str] = []
    collect_strings(data, text_values)
    text = "\n".join(text_values).strip()
    return [
        normalized_part(
            role="assistant",
            kind="text",
            text=text,
            metadata_json=content_card_metadata({"type": "answer", "format": "markdown"}),
        )
    ] if text else []


def load_parts_by_message(
    conn: sqlite3.Connection, session_id: str
) -> dict[str, list[dict[str, Any]]]:
    grouped: dict[str, list[dict[str, Any]]] = {}
    rows = conn.execute(
        """
        SELECT message_id, data
        FROM part
        WHERE session_id = ?
        ORDER BY time_created ASC, id ASC
        """,
        (session_id,),
    )
    for row in rows:
        grouped.setdefault(str(row["message_id"]), []).append(parse_json(row["data"]))
    return grouped


def user_text(parts: list[dict[str, Any]]) -> str:
    texts = [
        str(part["text"]).strip()
        for part in parts
        if part.get("type") == "text"
        and isinstance(part.get("text"), str)
        and str(part["text"]).strip()
    ]
    return "\n\n".join(texts)


def load_turns(conn: sqlite3.Connection, session_id: str) -> list[dict[str, Any]]:
    parts_by_message = load_parts_by_message(conn, session_id)
    turns: list[dict[str, Any]] = []
    current: dict[str, Any] | None = None
    rows = conn.execute(
        """
        SELECT id, time_created, data
        FROM message
        WHERE session_id = ?
        ORDER BY time_created ASC, id ASC
        """,
        (session_id,),
    )
    for row in rows:
        message_id = str(row["id"])
        data = parse_json(row["data"])
        role = str(data.get("role") or "")
        created_at = message_timestamp(row, data)
        message_parts = parts_by_message.get(message_id, [])
        if role == "user":
            prompt = user_text(message_parts)
            if not prompt:
                continue
            if current is not None:
                turns.append(current)
            current = {
                "external_id": message_id,
                "turn_index": len(turns),
                "user_text": prompt,
                "title": None,
                "started_at": created_at,
                "ended_at": None,
                "parts": [],
            }
        elif current is not None:
            for part in message_parts:
                current["parts"].extend(normalize_assistant_part(part))
            current["ended_at"] = created_at
    if current is not None:
        turns.append(current)
    return display_turns(turns)


def display_turns(turns: list[dict[str, Any]]) -> list[dict[str, Any]]:
    visible = [turn for turn in turns if turn.get("parts")]
    for index, turn in enumerate(visible):
        turn["turn_index"] = index
    return visible


def session_rows(
    conn: sqlite3.Connection, session_id: str | None, max_sessions: int
) -> list[sqlite3.Row]:
    columns = table_columns(conn, "session")
    project_expression = (
        "COALESCE(NULLIF(path, ''), NULLIF(directory, ''))"
        if {"path", "directory"} <= columns
        else "path"
        if "path" in columns
        else "directory"
        if "directory" in columns
        else "NULL"
    )
    query = f"""
        SELECT id, title, {project_expression} AS project_path, time_updated,
               COALESCE((SELECT MAX(CAST(time_created AS TEXT) || ':' || id)
                         FROM message WHERE session_id = session.id), '') AS message_marker,
               COALESCE((SELECT MAX(CAST(time_created AS TEXT) || ':' || id)
                         FROM part WHERE session_id = session.id), '') AS part_marker
        FROM session
    """
    params: list[Any] = []
    if session_id:
        query += " WHERE id = ?"
        params.append(session_id)
    query += " ORDER BY time_updated DESC, id DESC LIMIT ?"
    params.append(max_sessions)
    return list(conn.execute(query, params))


def normalized_sessions(
    conn: sqlite3.Connection,
    db_path: Path,
    session_id: str | None,
    max_sessions: int,
    include_turns: bool,
) -> list[dict[str, Any]]:
    sessions: list[dict[str, Any]] = []
    for row in session_rows(conn, session_id, max_sessions):
        turns = load_turns(conn, str(row["id"])) if include_turns else []
        if include_turns and not turns:
            continue
        sessions.append(
            {
                "external_id": str(row["id"]),
                "title": str(row["title"]) if row["title"] is not None else None,
                "project_path": (
                    str(row["project_path"]) if row["project_path"] is not None else None
                ),
                "started_at": turns[0]["started_at"] if turns else None,
                "updated_at": timestamp(row["time_updated"]),
                "source_locator": str(db_path),
                "source_fingerprint": session_version_token(row),
                "turns": turns,
            }
        )
    return sessions


def configured_limit(config: Any) -> int:
    if not isinstance(config, dict):
        return 500
    value = config.get("max_sessions", 500)
    if not isinstance(value, int) or isinstance(value, bool):
        raise ValueError("source.config.max_sessions must be an integer")
    return min(max(value, 1), 5000)


def run(request: dict[str, Any]) -> None:
    if request.get("protocol_version") != 1:
        raise ValueError("unsupported protocol_version")
    method = str(request.get("method") or "")
    if method not in {"probe", "list_sessions", "read_session"}:
        raise ValueError(f"unsupported method: {method}")
    source = request.get("source")
    if not isinstance(source, dict) or not isinstance(source.get("location"), str):
        raise ValueError("source.location is required")
    params = request.get("params")
    params = params if isinstance(params, dict) else {}
    session_id = params.get("session_id")
    if session_id is not None and not isinstance(session_id, str):
        raise ValueError("params.session_id must be a string or null")
    db_path = source_database(source["location"])
    max_sessions = configured_limit(source.get("config"))
    with connect_read_only(db_path) as conn:
        validate_schema(conn)
        if method == "probe":
            count = int(conn.execute("SELECT COUNT(*) FROM session").fetchone()[0])
            emit({"type": "complete", "item": {"session_count": count, "turn_count": 0}})
            return
        sessions = normalized_sessions(
            conn,
            db_path,
            session_id,
            max_sessions,
            include_turns=method == "read_session",
        )
        if method == "read_session" and session_id:
            before = session_rows(conn, session_id, 1)
            before_token = session_version_token(before[0]) if before else None
            returned_token = sessions[0]["source_fingerprint"] if sessions else None
            if before_token != returned_token:
                raise ValueError(f"ZCode session changed while it was being read: {session_id}")
    turn_count = 0
    for session in sessions:
        turn_count += len(session["turns"])
        if method == "list_sessions":
            emit({
                "type": "item",
                "item": {
                    "kind": "session_descriptor",
                    "external_id": session["external_id"],
                    "updated_at": session["updated_at"],
                    "source_locator": session["source_locator"],
                    "version_token": session["source_fingerprint"],
                },
            })
        else:
            emit({"type": "item", "item": {"kind": "session", "session": session}})
    emit(
        {
            "type": "complete",
            "item": {
                "session_count": len(sessions),
                "turn_count": turn_count,
                "snapshot_complete": True if method == "list_sessions" else None,
            },
        }
    )


def main() -> int:
    try:
        request = json.loads(sys.stdin.readline())
        if not isinstance(request, dict):
            raise ValueError("adapter request must be a JSON object")
        run(request)
    except Exception as error:
        emit(
            {
                "type": "error",
                "error": {
                    "message": str(error),
                    "kind": error.__class__.__name__,
                },
            }
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
