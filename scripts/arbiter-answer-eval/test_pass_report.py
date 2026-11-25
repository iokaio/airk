# SPDX-License-Identifier: Apache-2.0
"""Pass report joins grading and timing receipts by their exact sample key."""

from contextlib import redirect_stdout
import io
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import pass_report  # noqa: E402


class SampleJoin(unittest.TestCase):
    def test_repeated_samples_keep_their_own_grades_turns_and_times(self):
        pass_dir = Path("synthetic-pass")
        index = {"series": "synthetic", "results": []}
        documents = {pass_dir / "pass-index.json": index}
        turns = {}
        for sample, count, wall, tool_ms in ((1, 1, 1200, 150), (2, 3, 3400, 350)):
            name = "focus-gpu" if sample == 1 else "focus-gpu.s2"
            session = pass_dir / name
            coverage = session / "coverage.json"
            judges = [session / judge / "verdict.json" for judge in ("j1", "j2")]
            index["results"].append({
                "key": f"bookbuilder-A1/{name}", "configuration": "focus", "device": "gpu",
                "repo": "bookbuilder", "slot": "A1", "kind": "architecture", "sample": sample,
                "session_dir": str(session), "path": "focus",
                "coverage": str(coverage), "judges": [str(path) for path in judges],
            })
            documents[session / "session.json"] = {
                "status": "complete", "session_id": f"session-{sample}", "elapsed_ms": wall,
            }
            documents[coverage] = {
                "essential": ["fact"], "essential_uncovered": [], "all_essential_turn": count,
                "counted_turns": count, "essential_covered": 1, "essential_total": 1,
                "first_call_yield": int(sample == 1), "total_bytes": 100 * count,
                "total_elapsed_ms": tool_ms * count,
            }
            for path in judges:
                documents[path] = {"status": "valid", "verdict": {
                    "notes": [{"id": "fact", "verdict": "correct"}],
                }}
            turns[session / "wrapper" / "turns.jsonl"] = [
                {"turn": turn, "tool": "airk_answer", "ok": True, "elapsed_ms": tool_ms, "bytes": 100}
                for turn in range(1, count + 1)
            ]

        csv_path = pass_dir / "results.csv"
        archived_grades = io.StringIO(
            "repo,slot,configuration,device,judge1_grade,judge2_grade,adjudicated_grade,turns_to_finish\n"
            "bookbuilder,A1,focus,gpu,4,4,4,1\n"
            "bookbuilder,A1,focus,gpu,2,2,2,3\n"
        )
        with (patch.object(Path, "is_file", lambda path: path in documents or path == csv_path),
              patch.object(Path, "is_dir", return_value=True),
              patch.object(Path, "open", return_value=archived_grades),
              patch.object(pass_report.common, "read_json", side_effect=lambda path: documents[Path(path)]),
              patch.object(pass_report.common, "read_jsonl", side_effect=turns.__getitem__),
              patch.object(pass_report.common, "write_json") as output,
              redirect_stdout(io.StringIO())):
            self.assertEqual(pass_report.main(["--pass-dir", str(pass_dir)]), 0)

        destination, report = output.call_args.args
        self.assertEqual(destination, pass_dir / "pass-report.json")
        expected = [
            ("bookbuilder-A1/focus-gpu", "4", "4", "4", "1", 1, 1200, 150),
            ("bookbuilder-A1/focus-gpu.s2", "2", "2", "2", "3", 3, 3400, 1050),
        ]
        fields = ("key", "j1", "j2", "adjudicated", "turns_to_finish", "turns", "wall_ms", "tool_ms")
        self.assertEqual([tuple(row[field] for field in fields) for row in report["results"]], expected)
        self.assertEqual({name: row["sessions"] for name, row in report["per_result_name"].items()},
                         {"focus-gpu": 1, "focus-gpu.s2": 1})


if __name__ == "__main__":
    unittest.main()
