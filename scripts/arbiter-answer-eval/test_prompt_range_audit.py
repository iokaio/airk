# SPDX-License-Identifier: Apache-2.0
import copy
import json
from pathlib import Path
import unittest

import prompt_range_audit as a
import prompt_joint_audit as joint


class RangeAuditTests(unittest.TestCase):
    def setUp(self):
        file = Path(__file__).resolve()
        self.root = file.parent
        line = file.read_text().splitlines()[0]
        path = file.name
        at = f"{path}:1-1"
        sha = a.digest(file.read_bytes())
        args = {"task": "task", "needs": ["need"]}
        self.base = {"arguments": args, "pool": [{"file": path, "source_sha256": sha, "locations": [at]}]}
        prompt = a.PREFIX + a.pair_text(a.INSTRUCTION, "Task: task\nNeed: need\nExact strings: ",
                                        f"Path: {path}\nLines: 1-1\nSource:\n{line}") + a.SUFFIX
        self.row = {"pairs": 1, "clipped": 0, "scores": [{"need": 1, "file": path, "pool_position": 1,
                    "scored_at": at, "returned_at": at, "source_sha256": sha, "score": 0.7,
                    "tokens_before_cap": 60, "rendered_sha256": a.digest(prompt.encode())}],
                    "passages": [{"at": at, "needs": [1]}], "omissions": [],
                    "reply_needs": [{"need": 1, "text": "need", "where": [{"at": at}], "status": "candidate_evidence"}],
                    "scored_support": [{"need": 1, "scored_at": at, "returned_at": at}]}

    def test_new_pool_vs_scored_ceiling_uses_exact_need(self):
        note = {"N1": {"path": "a.py", "lines": [9, 11]}}
        original = {"a.py:1-3"}
        new = {(1, "a.py:8-12")}
        scored = [{"need": 1, "scored_at": "a.py:8-9", "returned_at": "a.py:8-12"}]
        rows = a.coverage_ceiling(note, [["N1"], ["N1"]], original, new, scored, [])
        self.assertEqual(rows[1]["pooled"]["whole"], ["N1"])
        self.assertEqual(rows[1]["scored_own"]["whole"], [])
        self.assertEqual(rows[0]["scored_own"]["whole"], ["N1"])
        self.assertEqual(rows[0]["short_scored_own"]["whole"], [])
        self.assertEqual(rows[0]["new_returned_any"]["whole"], [])

    def test_generated_windows_require_a_new_matching_seed(self):
        source = Path(__file__).resolve()
        at = f"{source.name}:1-1"
        original = {"arguments": {"needs": ["SPDX"]}, "pool": [{"file": source.name, "locations": []}]}
        row = {"generated": [{"need": 1, "file": source.name, "at": at, "seed": 1}]}
        self.assertEqual(a.generated_sources(row, original, source.parent), {(1, at)})
        original["pool"][0]["locations"] = [at]
        with self.assertRaisesRegex(ValueError, "already in original"):
            a.generated_sources(row, original, source.parent)

    def test_joint_wire_rejects_synthetic_link_without_original_yes_score(self):
        args = self.base["arguments"]
        body = {"task": args["task"], "needs": [{"need": 1, "text": "need", "where": [],
                                                  "status": "not_individually_scored"}],
                "eligibility": {}, "passages": []}
        blob = json.dumps({"jsonrpc": "2.0", "id": 1, "result": {"isError": False,
                          "content": [{"type": "text", "text": json.dumps(body)}]}}) + "\n"
        sample = {"wire_utf8": blob, "wire_sha256": a.digest(blob.encode()), "wire_bytes": len(blob), "accepted": []}
        joint.wire_variant(sample, 8192, self.root, args, {}, [])
        sample["accepted"] = [dict(self.row["scores"][0], kind="existing")]
        with self.assertRaisesRegex(ValueError, "positive model judgment"):
            joint.wire_variant(sample, 8192, self.root, args, {}, [])
        with self.assertRaisesRegex(ValueError, "frame length"):
            joint.wire_variant(sample, 4, self.root, args, {}, [])

    def test_source_only_input_and_consistent_need_status(self):
        a.verify_range_variant(self.row, self.base, self.root, 40)
        self.row["reply_needs"][0]["status"] = "no_candidate_evidence"
        with self.assertRaisesRegex(ValueError, "Need status"):
            a.verify_range_variant(self.row, self.base, self.root, 40)

    def test_rejects_changed_clipping_and_grader_contaminated_input(self):
        for field, value in [("tokens_before_cap", 1025), ("rendered_sha256", "changed")]:
            row = copy.deepcopy(self.row)
            row["scores"][0][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                a.verify_range_variant(row, self.base, self.root, 40)

    def test_rejects_double_omission_and_invalid_provenance(self):
        row = self.row
        row["omissions"] = [{"need": 1, "at": row["scores"][0]["returned_at"], "reason": "byte_limit"}]
        with self.assertRaisesRegex(ValueError, "accounted"):
            a.verify_range_variant(row, self.base, self.root, 40)
        row["omissions"] = []
        row["scored_support"] = []
        with self.assertRaisesRegex(ValueError, "attribution"):
            a.verify_range_variant(row, self.base, self.root, 40)


if __name__ == "__main__":
    unittest.main()
