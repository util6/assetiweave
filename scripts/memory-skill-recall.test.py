#!/usr/bin/env python3
import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "builtin-assets/skills/assetiweave-memory/scripts/recall.py"


class MemorySkillRecallTest(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.cli = Path(self.temp_dir.name) / "assetiweave-cli"
        self.cli.write_text(
            textwrap.dedent(
                r'''#!/usr/bin/env python3
import json
import sys

args = sys.argv[1:]

def success(data):
    print(json.dumps({"ok": True, "data": data}))

if args == ["version"]:
    success({"cli_version": "0.6.1", "engine_version": "0.6.1", "compatible": True})
elif args[:1] == ["schema"]:
    success({"method": args[1]})
elif args[:3] == ["memory", "recall", "search"]:
    query = args[args.index("--query") + 1]
    limit = args[args.index("--limit") + 1]
    project = args[args.index("--project") + 1] if "--project" in args else None
    success({
        "query": query,
        "limit": int(limit),
        "scope": {"project_path": project},
        "hits": [{"title": "Memory source record", "snippet": "A bounded source result."}],
    })
elif args[:4] == ["memory", "recall", "session", "create"]:
    project = args[args.index("--project") + 1] if "--project" in args else None
    success({"id": "recall-session-1", "scope": {"project_path": project}, "turns": []})
elif args[:4] == ["memory", "recall", "turn", "send"]:
    session_id = args[4]
    query = args[args.index("--query") + 1]
    success({
        "id": session_id,
        "scope": {"project_path": "/repo"},
        "turns": [{"id": "turn-1", "status": "queued", "query": query}],
    })
elif args[:4] == ["memory", "recall", "session", "get"]:
    success({
        "id": args[4],
        "scope": {"project_path": "/repo"},
        "turns": [{
            "id": "turn-1",
            "status": "completed",
            "query": "What changed?",
            "output": {
                "answer": "The source record describes a completed change.",
                "sessionReferences": [{"sessionId": "session-1"}],
                "contentReferences": [{"contentId": "content-1"}],
                "followUpSuggestions": ["Ask about the implementation details."],
            },
        }],
    })
else:
    print(json.dumps({"ok": False, "error": {"message": "unexpected args: " + repr(args)}}))
    sys.exit(3)
'''
            ),
            encoding="utf-8",
        )
        self.cli.chmod(0o755)

    def tearDown(self):
        self.temp_dir.cleanup()

    def run_script(self, *args):
        environment = os.environ.copy()
        environment["ASSETIWEAVE_CLI"] = str(self.cli)
        completed = subprocess.run(
            ["python3", str(SCRIPT), *args],
            check=False,
            capture_output=True,
            text=True,
            env=environment,
        )
        payload = json.loads(completed.stdout)
        return completed, payload

    def test_doctor_checks_the_unified_memory_contracts(self):
        completed, payload = self.run_script("doctor")

        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertTrue(payload["ok"])
        self.assertTrue(payload["data"]["ready"])
        self.assertEqual(
            payload["data"]["contracts"],
            [
                "memory.recent.list",
                "memory.context.resolve",
                "memory.project.get",
                "memory.recall.search",
                "memory.recall.session.create",
                "memory.recall.session.get",
                "memory.recall.turn.send",
                "memory.recall.turn.cancel",
            ],
        )

    def test_search_uses_the_new_recall_contract_and_scope(self):
        completed, payload = self.run_script(
            "search",
            "--query",
            "recent decision",
            "--project",
            "/repo",
            "--limit",
            "3",
        )

        self.assertEqual(completed.returncode, 0, completed.stderr)
        data = payload["data"]
        self.assertEqual(data["query"], "recent decision")
        self.assertEqual(data["limit"], 3)
        self.assertEqual(data["scope"]["project_path"], "/repo")
        self.assertEqual(len(data["hits"]), 1)

    def test_recall_creates_sends_and_polls_one_persistent_turn(self):
        completed, payload = self.run_script(
            "recall",
            "--query",
            "What changed?",
            "--project",
            "/repo",
            "--poll",
            "0.01",
        )

        self.assertEqual(completed.returncode, 0, completed.stderr)
        data = payload["data"]
        self.assertEqual(data["id"], "recall-session-1")
        self.assertEqual(data["turns"][-1]["status"], "completed")
        self.assertEqual(
            data["turns"][-1]["output"]["contentReferences"],
            [{"contentId": "content-1"}],
        )

    def test_invalid_recall_polling_is_reported_as_structured_error(self):
        completed, payload = self.run_script(
            "recall",
            "--query",
            "What changed?",
            "--poll",
            "0",
        )

        self.assertEqual(completed.returncode, 3)
        self.assertFalse(payload["ok"])
        self.assertIn("polling values", payload["error"]["message"])


if __name__ == "__main__":
    unittest.main()
