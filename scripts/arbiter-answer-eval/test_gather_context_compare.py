# SPDX-License-Identifier: Apache-2.0
"""Independent controls for Gather Context coverage and reporting."""

import copy
import unittest

from gather_context_compare import compare, report


SHA = "a" * 64
CASE = "bookbuilder-C1"


def fixtures():
    witness = {"path": "docs/owner.md", "start_line": 3, "end_line": 4,
               "source_sha256": "b" * 64, "excerpt_sha256": "c" * 64}
    identity = {"id": CASE, "repo": "bookbuilder", "publication": "publication", "index_id": "index"}
    trace = {"steps": [{"number": 1}], "handoff": {"source_wording": "An observed lead"},
             "stop": {"source_wording": "No live run"},
             "proof_limits": {"closing_qualification": "No live run"}}
    reference = {
        "schema": "airk.private.gather-context-reference/1", "spec_sha256": SHA,
        "cases": [{**identity, "slot": "C1", "source_commit": "d" * 40,
                   "author_paths": {"openai": trace, "anthropic": trace},
                   "obligations": [{"id": "need-1", "fact": "A fact", "witness": witness,
                                    "alternate_locations_for_review": [{"path": "docs/alternate.md"}]}]}],
    }
    control = {"schema": "airk.private.gather-context-capture/1", "spec_sha256": SHA, "arm": "control",
               "cases": [{**identity, "disposition": "partial", "delivered_passages": [{
                   **witness, "excerpt_sha256": "d" * 64}],
                          "actions": [{"round": 1, "selector": "find", "elapsed_ms": 10}],
                          "costs": {"elapsed_ms": 10, "scanned_files": None}}]}
    candidate = {"schema": "airk.private.gather-context-capture/1", "spec_sha256": SHA, "arm": "candidate",
                 "cases": [{**identity, "disposition": "completed", "delivered_passages": [dict(witness)],
                            "actions": [{"round": 1, "selector": "find", "elapsed_ms": 20},
                                        {"round": 2, "selector": "docs/owner.md"}],
                            "costs": {"elapsed_ms": 20, "scanned_files": 2}}]}
    return reference, control, candidate


class CompareTests(unittest.TestCase):
    def test_counts_exact_witnesses_and_excludes_unknown_cost_from_delta(self):
        result = compare(*fixtures())
        row = result["cases"][0]
        self.assertEqual(row["arms"]["control"]["verified_exact_witnesses"], [])
        self.assertEqual(row["arms"]["candidate"]["verified_exact_witnesses"], ["need-1"])
        self.assertEqual(row["alternate_locations_for_review"], {"need-1": [{"path": "docs/alternate.md"}]})
        self.assertEqual(row["cost_comparison"], {
            "elapsed_ms": {"control": 10, "candidate": 20, "delta": 10},
            "scanned_files": {"control": None, "candidate": 2, "delta": None},
        })
        self.assertEqual(row["first_observed_divergence"]["kind"], "delivered_witness_difference")
        self.assertIsNone(row["first_observed_divergence"]["position"])

    def test_broader_range_and_alternate_path_stay_unverified(self):
        reference, control, candidate = fixtures()
        passages = candidate["cases"][0]["delivered_passages"]
        passages[0]["end_line"] = 5
        passages.append({**passages[0], "path": "docs/alternate.md", "end_line": 4})
        row = compare(reference, control, candidate)["cases"][0]
        self.assertEqual(row["arms"]["candidate"]["verified_exact_witnesses"], [])
        self.assertEqual(row["arms"]["candidate"]["unverified_obligations"], ["need-1"])

    def test_refuses_changed_identity_missing_cases_and_duplicate_cases(self):
        for update, message in [
            (lambda c: c.update(spec_sha256="f" * 64), "capture spec differs"),
            (lambda c: c.update(cases=[]), "nonempty case roster"),
            (lambda c: c["cases"].append(copy.deepcopy(c["cases"][0])), "duplicate case id"),
            (lambda c: c["cases"][0].update(id="munarium-C1"), "case roster differs"),
            (lambda c: c["cases"][0].update(publication="stale"), "scope or publication differs"),
            (lambda c: c["cases"][0].update(source_commit="f" * 40), "source commit differs"),
            (lambda c: c["cases"][0].update(disposition="unknown"), "invalid disposition"),
        ]:
            with self.subTest(message=message), self.assertRaisesRegex(ValueError, message):
                reference, control, candidate = fixtures()
                update(candidate)
                compare(reference, control, candidate)

    def test_reports_first_comparable_action_before_delivery(self):
        reference, control, candidate = fixtures()
        candidate["cases"][0]["actions"][0]["selector"] = "literal"
        row = compare(reference, control, candidate)["cases"][0]
        self.assertEqual(row["first_observed_divergence"], {
            "kind": "recorded_action", "position": 1, "changed_fields": ["selector"],
            "control": {"selector": "find"}, "candidate": {"selector": "literal"},
        })

    def test_requires_both_judgments_and_labels_unjudged(self):
        comparison = compare(*fixtures())
        unjudged = report(comparison)
        self.assertIn("Unjudged diagnostic", unjudged)
        self.assertIn("0/1 (partial) | 1/1 (completed)", unjudged)
        self.assertIn("No live run", unjudged)
        control = {"id": CASE, "arm": "control", "status": "scored",
                   "correct_finish": False, "external_turns": None}
        candidate = {"id": CASE, "arm": "candidate", "status": "scored",
                     "correct_finish": True, "external_turns": 2, "citations": ["docs/owner.md:3"]}
        with self.assertRaisesRegex(ValueError, "spec differs"):
            report(comparison, judged={"spec_sha256": "f" * 64, "cases": [control, candidate]})
        with self.assertRaisesRegex(ValueError, "roster differs"):
            report(comparison, judged={"spec_sha256": SHA, "cases": [candidate]})
        with self.assertRaisesRegex(ValueError, "duplicate"):
            report(comparison, judged={"spec_sha256": SHA, "cases": [control, control]})
        judged = report(comparison, judged={"spec_sha256": SHA, "cases": [control, candidate]})
        self.assertIn("Candidate judgment. scored, correct finish, 2 external turns.", judged)
        self.assertIn("docs/owner.md:3", judged)
