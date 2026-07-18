from __future__ import annotations

import json
import subprocess
import sys
import unittest
from pathlib import Path
from typing import Any


ADAPTER = Path(__file__).parents[1] / "adapters" / "zcode" / "zcode_adapter.py"


def run_adapter(request: dict[str, Any]) -> list[dict[str, Any]]:
    result = subprocess.run(
        [sys.executable, str(ADAPTER)],
        input=f"{json.dumps(request)}\n",
        text=True,
        capture_output=True,
        check=True,
    )
    return [json.loads(line) for line in result.stdout.splitlines() if line]


class ZcodeAdapterProtocolTests(unittest.TestCase):
    def test_probe_does_not_require_or_open_a_source_database(self) -> None:
        messages = run_adapter({"protocol_version": 1, "method": "probe"})

        self.assertEqual(
            messages,
            [{"type": "complete", "item": {"session_count": 0, "turn_count": 0}}],
        )

    def test_list_sessions_still_requires_a_source_location(self) -> None:
        messages = run_adapter({"protocol_version": 1, "method": "list_sessions"})

        self.assertEqual(messages[0]["type"], "error")
        self.assertEqual(messages[0]["error"]["message"], "source.location is required")


if __name__ == "__main__":
    unittest.main()
