from __future__ import annotations

import json
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ADAPTER = Path(__file__).parents[1] / "adapters" / "zcode" / "adapter.mjs"


def create_fixture(path: Path) -> None:
    conn = sqlite3.connect(path)
    conn.executescript(
        """
        CREATE TABLE session (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            slug TEXT NOT NULL,
            directory TEXT NOT NULL,
            path TEXT,
            title TEXT NOT NULL,
            version TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL
        );
        CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        CREATE TABLE part (
            id TEXT PRIMARY KEY,
            message_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        """
    )
    conn.execute(
        """
        INSERT INTO session (
            id, project_id, slug, directory, path, title, version,
            time_created, time_updated
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            "session-1",
            "project-1",
            "fixture",
            "/tmp/zcode-project",
            "/tmp/zcode-project",
            "ZCode fixture",
            "1",
            1_767_225_600_000,
            1_767_225_604_000,
        ),
    )
    messages = [
        ("message-1", "user", 1_767_225_600_000),
        ("message-2", "assistant", 1_767_225_601_000),
        ("message-3", "user", 1_767_225_602_000),
        ("message-4", "assistant", 1_767_225_603_000),
    ]
    for message_id, role, timestamp in messages:
        conn.execute(
            """
            INSERT INTO message (
                id, session_id, time_created, time_updated, data
            ) VALUES (?, ?, ?, ?, ?)
            """,
            (
                message_id,
                "session-1",
                timestamp,
                timestamp,
                json.dumps(
                    {
                        "role": role,
                        "time": {
                            "created": timestamp,
                            "completed": timestamp + 500 if role == "assistant" else None,
                        },
                    }
                ),
            ),
        )
    parts = [
        ("part-1", "message-1", {"type": "text", "text": "How should ZCode import?"}),
        ("part-2", "message-2", {"type": "reasoning", "text": "hidden reasoning"}),
        (
            "part-3",
            "message-2",
            {
                "type": "text",
                "text": "Use the external adapter.\n\n```sh\npnpm test\n```",
            },
        ),
        (
            "part-4",
            "message-2",
            {
                "type": "tool",
                "tool": "Read",
                "state": {
                    "status": "completed",
                    "input": {"file_path": "/tmp/zcode-project/src/main.rs"},
                    "output": "file contents",
                },
            },
        ),
        (
            "part-5",
            "message-2",
            {
                "type": "tool",
                "tool": "Bash",
                "state": {
                    "status": "completed",
                    "input": {
                        "command": "printf '%s\\n' '--- tests ---' && pnpm test",
                        "workdir": "/tmp/zcode-project",
                    },
                    "output": "tests passed",
                },
            },
        ),
        (
            "part-6",
            "message-2",
            {
                "type": "tool",
                "tool": "Bash",
                "callID": "zcode-simple-shell",
                "command": "pwd",
                "output": "simple output",
            },
        ),
        ("part-7", "message-3", {"type": "text", "text": "Continue"}),
        ("part-8", "message-4", {"type": "text", "text": "Done"}),
    ]
    for index, (part_id, message_id, data) in enumerate(parts):
        timestamp = 1_767_225_600_000 + index
        conn.execute(
            """
            INSERT INTO part (
                id, message_id, session_id, time_created, time_updated, data
            ) VALUES (?, ?, ?, ?, ?, ?)
            """,
            (
                part_id,
                message_id,
                "session-1",
                timestamp,
                timestamp,
                json.dumps(data),
            ),
        )
    conn.commit()
    conn.close()


def run_adapter(db_path: Path, session_id: str | None = None) -> list[dict]:
    request = {
        "protocol_version": 1,
        "request_id": "test-request",
        "method": "read_session",
        "source": {"location": str(db_path), "config": None},
        "params": {"session_id": session_id},
    }
    completed = subprocess.run(
        ["node", str(ADAPTER)],
        input=json.dumps(request),
        text=True,
        capture_output=True,
        check=True,
    )
    return [json.loads(line) for line in completed.stdout.splitlines() if line.strip()]


class ZCodeConversationAdapterTests(unittest.TestCase):
    def test_read_session_normalizes_zcode_sqlite_without_mutating_it(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = Path(tmpdir) / "db.sqlite"
            create_fixture(db_path)
            before = (db_path.stat().st_size, db_path.stat().st_mtime_ns)

            lines = run_adapter(db_path)

            after = (db_path.stat().st_size, db_path.stat().st_mtime_ns)
            self.assertEqual(after, before)
            self.assertEqual(lines[-1]["type"], "complete")
            session = lines[0]["item"]["session"]
            self.assertEqual(session["external_id"], "session-1")
            self.assertEqual(session["project_path"], "/tmp/zcode-project")
            self.assertEqual(session["started_at"], "1767225600000")
            self.assertEqual(session["updated_at"], "1767225604000")
            self.assertEqual(len(session["turns"]), 2)

            first_turn = session["turns"][0]
            self.assertEqual(first_turn["user_text"], "How should ZCode import?")
            self.assertEqual(first_turn["started_at"], "1767225600000")
            self.assertEqual(first_turn["ended_at"], "1767225601000")
            self.assertNotIn("hidden reasoning", json.dumps(first_turn))

            text_part = next(
                part
                for part in first_turn["parts"]
                if part["kind"] == "text" and part["role"] == "assistant"
            )
            self.assertEqual(text_part["text"], "Use the external adapter.")
            code_part = next(part for part in first_turn["parts"] if part["kind"] == "code_block")
            self.assertEqual(code_part["language"], "sh")
            self.assertEqual(code_part["text"], "pnpm test")
            tool_part = next(part for part in first_turn["parts"] if part["kind"] == "tool")
            self.assertEqual(tool_part["role"], "tool")
            self.assertEqual(tool_part["text"], "file contents")
            command_part = next(part for part in first_turn["parts"] if part["kind"] == "command")
            self.assertEqual(command_part["command"], "printf '%s\\n' '--- tests ---' && pnpm test")
            self.assertEqual(command_part["cwd"], "/tmp/zcode-project")
            self.assertEqual(command_part["source_execution_id"], "part-5")
            self.assertNotIn("shell_execution_projection", json.loads(command_part["metadata_json"]))
            result_part = next(
                part
                for part in first_turn["parts"]
                if part["kind"] == "tool" and part["text"] == "tests passed"
            )
            self.assertEqual(result_part["status"], "completed")
            self.assertEqual(result_part["source_execution_id"], "part-5")
            simple_command = next(
                part
                for part in first_turn["parts"]
                if part["kind"] == "command" and part["source_execution_id"] == "zcode-simple-shell"
            )
            self.assertEqual(simple_command["command"], "pwd")
            self.assertNotIn("shell_execution_projection", json.loads(simple_command["metadata_json"]))

    def test_read_session_filters_by_external_session_id(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = Path(tmpdir) / "db.sqlite"
            create_fixture(db_path)

            missing_lines = run_adapter(db_path, "missing-session")

            self.assertEqual(
                missing_lines,
                [
                    {
                        "type": "complete",
                        "item": {
                            "session_count": 0,
                            "turn_count": 0,
                            "snapshot_complete": None,
                        },
                    }
                ],
            )


if __name__ == "__main__":
    unittest.main()
