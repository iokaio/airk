# SPDX-License-Identifier: Apache-2.0
"""Focused controls for the committed vector coverage diagnostic."""

from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))

import common
import search_vector_audit as audit


def unit(path, first, last, *, row=0, members=None, native=True):
    lines = {"start": first, "end": last}
    location = {"Lines": lines} if native else {"Page": {"number": 4}}
    return {"row": row, "citation": {"path": path, "start_line": first, "end_line": last, "location": location},
            "members": members if members is not None else []}


def record(*units, present=True):
    return {"source_present": present, "units": units}


class Coverage(unittest.TestCase):
    repo = "bookbuilder-v2"
    path = "docs/architecture.md"
    alternative = "docs/reference.md"

    def classify(self, record_for_note, record_for_alternative=None):
        note = {"path": self.path, "lines": [10, 20]}
        paths = {(self.repo, self.path): record_for_note}
        if record_for_alternative is not None:
            note["alternatives"] = [{"path": self.alternative, "lines": [10, 20]}]
            paths[(self.repo, self.alternative)] = record_for_alternative
        return audit.classify(self.repo, note, paths)

    def test_reports_absent_source_empty_index_and_non_line_citation_separately(self):
        self.assertEqual(self.classify(record(present=False)), ("source_absent", []))
        self.assertEqual(self.classify(record()), ("no_committed_units", []))
        self.assertEqual(self.classify(record(unit(self.path, 10, 20, native=False))), ("no_native_lines", []))

    def test_rule_two_counts_an_alternative_with_five_overlapping_lines(self):
        alternative = unit(self.alternative, 7, 14)
        self.assertEqual(self.classify(record(unit(self.path, 7, 13)), record(alternative)),
                         ("single_unit", [alternative]))

    def test_union_counts_distinct_lines_on_one_path_only(self):
        first = unit(self.path, 8, 12)
        overlapping = unit(self.path, 10, 13)
        tail = unit(self.path, 18, 18)
        separate = unit(self.alternative, 19, 20)
        self.assertEqual(self.classify(record(first, overlapping), record(separate)), ("no_coverage", []))
        self.assertEqual(self.classify(record(first, overlapping, tail), record(separate)), ("union_only", []))

    def test_trace_joins_members_and_path_at_exact_bounded_rank(self):
        wanted, wrong = [{"chunk_index": 0, "span": {"start": 12, "end": 18}}], [
            {"chunk_index": 0, "span": {"start": 21, "end": 30}}]
        ranked = [[{"path": self.path, "members": wrong, "occurrence": "other-passage"},
                   {"path": self.alternative, "members": wanted, "occurrence": "other-file"}],
                  [{"path": self.path, "members": wrong, "occurrence": "other-passage"},
                   {"path": self.path, "members": wanted, "occurrence": "qualified"}]]
        units = [unit(self.path, 10, 20, members=wanted)]
        self.assertEqual(audit.occurrence_ranks(ranked, units),
                         [{"query": 1, "rank": 2, "occurrence": "qualified", "source": {
                             "path": self.path, "start_line": 10, "end_line": 20}}])

    def test_rank_keeps_sparse_global_position_and_requires_single_unit(self):
        note = {"path": self.path, "lines": [10, 20]}
        ranked = [(2, audit.unit_span(unit(self.path, 8, 12)["citation"])),
                  (8, audit.unit_span(unit(self.path, 13, 15)["citation"])),
                  (21, audit.unit_span(unit(self.path, 16, 20)["citation"]))]
        self.assertEqual(audit.first_covering_rank(ranked, note), 21)
        self.assertEqual(audit.first_covering_rank(ranked, note, union_only=True), 8)

    def test_rank_union_does_not_mix_alternatives_or_recount_lines(self):
        note = {"path": self.path, "lines": [10, 20],
                "alternatives": [{"path": self.alternative, "lines": [10, 20]}]}
        ranked = [(2, audit.unit_span(unit(self.path, 9, 12)["citation"])),
                  (4, audit.unit_span(unit(self.alternative, 13, 15)["citation"])),
                  (6, audit.unit_span(unit(self.path, 11, 13)["citation"])),
                  (12, audit.unit_span(unit(self.path, 19, 19)["citation"]))]
        self.assertIsNone(audit.first_covering_rank(ranked[:-1], note, union_only=True))
        self.assertEqual(audit.first_covering_rank(ranked, note, union_only=True), 12)

    def test_rank_rejects_repeated_positions_without_a_cover(self):
        note = {"path": self.path, "lines": [10, 20]}
        with self.assertRaisesRegex(common.HarnessError, "strictly increasing"):
            audit.first_covering_rank([(2, None), (2, None)], note)


    def test_native_log_must_cover_each_requested_path(self):
        input_record = {"collections": [{"repository": self.repo, "committed_rows": 1, "paths": [self.path]}]}
        with self.assertRaisesRegex(common.HarnessError, "every requested path"):
            audit.native_paths(b"unrelated native output\n", input_record)
        raw = (audit.NATIVE_MARKER +
               '{"repository":"bookbuilder-v2","path":"docs/architecture.md","source_present":true,"units":[]}\n')
        self.assertEqual(audit.native_paths(raw.encode("utf-8"), input_record)[(self.repo, self.path)]["units"], [])


if __name__ == "__main__":
    unittest.main()
