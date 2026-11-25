# SPDX-License-Identifier: Apache-2.0
"""In-memory checks for the original-weight Focus screen (no Torch or input files)."""

from pathlib import Path
import sys
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import prompt_screen as screen


class Tokenizer:
    def encode(self, value):
        return [ord(character) for character in value]


def row(repo, split, label, *, pair=None, instruction="publisher", doc=None):
    pair = pair or f"{repo}-{split}"
    query, document = "the test question", doc or f"test document {label}"
    ids = screen.text.compile_live_ids(Tokenizer(), screen.text.INSTRUCTIONS[instruction],
                                       query, document, screen.WIDTH)
    return {"id": f"{pair}-{label}", "repo": repo, "set": split, "label": label, "kind": "test",
            "pair_id": pair, "instruction": instruction, "live_ids": ids, "query": query,
            "document": document, "query_sha256": screen.text.sha256_text(query),
            "document_sha256": screen.text.sha256_text(document)}


def roster():
    return [row(repo, split, label)
            for repo in screen.REPOS for split in screen.LOCAL_SETS for label in (0, 1)] + [
        row("replay", "replay_dev", 0, instruction="software_evidence_v1"),
        row("replay", "replay_general_heldout", 1),
    ]


class PromptScreenTests(unittest.TestCase):
    def test_fixed_groups_and_general_replay_have_identical_inputs(self):
        rows = roster()
        groups, replay = screen.paired_cohort(rows, 1)
        arms, unchanged, counts = screen.compile_matched(Tokenizer(), groups, replay)
        self.assertEqual(len(groups), 4)
        fixed = ("id", "query", "document", "label", "pair_id")
        control, candidate = (arms[arm] for arm in screen.ARMS)
        for a, b in zip(control, candidate, strict=True):
            self.assertEqual([a[k] for k in fixed], [b[k] for k in fixed])
            self.assertNotEqual(a["live_ids"], b["live_ids"])
        self.assertEqual(unchanged, replay)
        self.assertTrue(all(entry["paired_groups"] == 1 for entry in counts.values()))

    def test_over_width_refusal_excludes_whole_rank_group_from_both_arms(self):
        rows = roster()
        first = rows[0]
        publisher_space = screen.WIDTH - len(first["live_ids"])
        long_document = first["document"] + "z" * publisher_space
        rows[0] = row(first["repo"], first["set"], first["label"], doc=long_document)
        self.assertEqual(len(rows[0]["live_ids"]), screen.WIDTH)
        groups, replay = screen.paired_cohort(rows, 1)
        arms, _, counts = screen.compile_matched(Tokenizer(), groups, replay)
        self.assertEqual(counts["bookbuilder/heldout_id"]["software_evidence_v1_refusals"], 1)
        self.assertEqual(counts["bookbuilder/heldout_id"].get("paired_rows", 0), 0)
        self.assertEqual(len(arms["publisher"]), 6)
        self.assertEqual(len(arms["software_evidence_v1"]), 6)

    def test_paired_ranking_and_general_control_metrics(self):
        groups, replay = screen.paired_cohort(roster(), 1)
        arms, unchanged, counts = screen.compile_matched(Tokenizer(), groups, replay)
        invocations = []

        def fake_score(rows):
            invocations.append(rows)
            good_arm = rows is not arms["publisher"]
            return [[row["id"], 0.0, float(row["label"] if good_arm else 1 - row["label"])] for row in rows]

        result = screen.comparison(arms, unchanged, fake_score, counts)
        self.assertEqual(invocations, [arms["publisher"], arms["software_evidence_v1"], unchanged])
        self.assertEqual(result["software_minus_publisher"]["heldout_id/bookbuilder"]["hit_at_1"], 1)
        self.assertEqual(result["paired_rank_changes"]["bookbuilder/heldout_id"], {"improved": 1})
        self.assertEqual(result["unchanged_replay"]["replay_general_heldout/all"]["rows"], 1)
        self.assertEqual(result["replay_runs"], 1)
        arms["software_evidence_v1"][0]["label"] = 1
        with self.assertRaisesRegex(ValueError, "not matched"):
            screen.comparison(arms, unchanged, fake_score, counts)

    def test_pins_and_replay_admission_refuse_corrupt_inputs(self):
        rows = roster()
        with self.assertRaisesRegex(ValueError, "both independently retained"):
            screen.paired_cohort(rows[:-1], 1)
        rows[0]["document"] = "unpaired content"
        with self.assertRaisesRegex(ValueError, "retained hash"):
            screen.paired_cohort(rows, 1)
        item = roster()[0]
        item["live_ids"] = []
        with self.assertRaisesRegex(ValueError, "retained input tokens"):
            screen.compile_one(Tokenizer(), item, "publisher")

    def test_direct_worker_refuses_without_owned_start_gate(self):
        with patch.object(screen.worker, "digest", return_value="plan"), patch.dict(screen.os.environ, {}, clear=True):
            with self.assertRaisesRegex(ValueError, "exact owned"):
                screen.run(Path("unused"), "plan")
