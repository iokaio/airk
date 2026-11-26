# SPDX-License-Identifier: Apache-2.0
"""Independent controls for the Gather Context loss-stage classification."""

import json
import pathlib
import tempfile
import unittest

import gather_context_loss_stage as loss


def hit(path, start, end, text="source text"):
    return {"citation": {"path": path, "start_line": start, "end_line": end}, "text": text, "truncated": False}


def record(operation, hits, arguments=None):
    return {"operation": operation, "arguments": arguments or {}, "canonical_result": {"hits": hits}}


def note(note_id, path, lines, mark="essential", alternatives=None):
    row = {"id": note_id, "mark": mark, "path": path, "lines": lines, "fact": "a fact"}
    if alternatives:
        row["alternatives"] = alternatives
    return row


class LossStageTest(unittest.TestCase):
    def stage(self, records, a_note):
        reply = {"structuredContent": {"records": records}}
        return loss.classify({"notes": [a_note]}, reply)["notes"][0]["stage"]

    def test_a_source_read_over_the_lines_is_opened(self):
        records = [record("search", [hit("src/a.rs", 1, 40)]), record("read_file", [hit("src/a.rs", 10, 30)])]
        self.assertEqual(self.stage(records, note("N1", "src/a.rs", [12, 20])), "opened")

    def test_a_search_hit_over_the_lines_is_only_listed(self):
        records = [record("search", [hit("src/a.rs", 10, 30)])]
        self.assertEqual(self.stage(records, note("N1", "src/a.rs", [12, 20])), "listed")

    def test_too_little_overlap_leaves_the_file_stage(self):
        # Four of nine lines overlap; the rule needs five.
        records = [record("context", [hit("src/a.rs", 1, 15)])]
        self.assertEqual(self.stage(records, note("N1", "src/a.rs", [12, 20])), "file")

    def test_a_short_range_needs_all_of_its_lines(self):
        records = [record("read_file", [hit("src/a.rs", 5, 6)])]
        self.assertEqual(self.stage(records, note("N1", "src/a.rs", [5, 7])), "file")
        records = [record("read_file", [hit("src/a.rs", 5, 7)])]
        self.assertEqual(self.stage(records, note("N1", "src/a.rs", [5, 7])), "opened")

    def test_another_file_is_never(self):
        records = [record("read_file", [hit("src/b.rs", 12, 20)])]
        self.assertEqual(self.stage(records, note("N1", "src/a.rs", [12, 20])), "never")

    def test_an_alternative_location_counts(self):
        alternative = [{"path": "docs/guide.md", "lines": [3, 9]}]
        records = [record("context", [hit("docs/guide.md", 1, 12)])]
        self.assertEqual(self.stage(records, note("N1", "src/a.rs", [12, 20], alternatives=alternative)), "opened")

    def test_a_longer_path_matches_its_suffix(self):
        records = [record("read_file", [hit("pro/src/a.rs", 12, 20)])]
        self.assertEqual(self.stage(records, note("N1", "src/a.rs", [12, 20])), "opened")

    def test_a_read_with_no_returned_span_still_reaches_its_file(self):
        records = [record("read_file", [], {"path": "src/a.rs", "start_line": 12, "end_line": 20})]
        self.assertEqual(self.stage(records, note("N1", "src/a.rs", [12, 20])), "file")

    def test_only_essential_notes_are_counted(self):
        reply = {"records": [record("read_file", [hit("src/a.rs", 12, 20)])]}
        scenario = {"notes": [note("N1", "src/a.rs", [12, 20]), note("N2", "src/a.rs", [1, 5], mark="useful")]}
        result = loss.classify(scenario, reply)
        self.assertEqual(result["essentials"], 1)
        self.assertEqual(result["counts"], {"opened": 1, "listed": 0, "file": 0, "never": 0})

    def test_follow_up_reads_group_nearby_missing_notes_per_file(self):
        notes = [note("N1", "src/a.rs", [10, 20]), note("N2", "src/a.rs", [200, 240]),
                 note("N3", "src/a.rs", [900, 910]), note("N4", "src/b.rs", [5, 9]),
                 note("N5", "src/c.rs", [1, 3]), note("N6", "src/d.rs", [1, 3], mark="useful")]
        stages = {"N1": "file", "N2": "never", "N3": "file", "N4": "never", "N5": "opened"}
        reads = loss.follow_up_reads(notes, stages)
        self.assertEqual([(read["path"], read["start_line"], read["end_line"], read["notes"]) for read in reads],
                         [("src/a.rs", 10, 240, ["N1", "N2"]), ("src/a.rs", 900, 910, ["N3"]),
                          ("src/b.rs", 5, 9, ["N4"])])

    def test_a_listed_note_needs_no_follow_up_read(self):
        notes = [note("N1", "src/a.rs", [10, 20])]
        self.assertEqual(loss.follow_up_reads(notes, {"N1": "listed"}), [])

    def test_a_reply_without_records_is_refused(self):
        with self.assertRaises(ValueError):
            loss.classify({"notes": []}, {"content": []})

    def test_totals_by_repository_and_a_missing_reply(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            for repository in ("alpha", "beta"):
                (root / "scenarios" / repository).mkdir(parents=True)
            (root / "replies").mkdir()
            scenario = {"notes": [note("N1", "src/a.rs", [12, 20]), note("N2", "src/c.rs", [1, 9])]}
            for repository, slot in (("alpha", "C1"), ("beta", "C1"), ("beta", "C2")):
                (root / "scenarios" / repository / f"{slot}.json").write_text(json.dumps(scenario), "utf-8")
            reply = {"structuredContent": {"records": [record("search", [hit("src/a.rs", 10, 30)])]}}
            (root / "replies" / "alpha-C1.reply.json").write_text(json.dumps(reply), "utf-8")
            (root / "replies" / "beta-C1.reply.json").write_text(json.dumps(reply), "utf-8")
            result = loss.classify_all(root / "scenarios", root / "replies")
        self.assertEqual([case["id"] for case in result["cases"]], ["alpha-C1", "beta-C1"])
        self.assertEqual(result["essentials"], 4)
        self.assertEqual(result["totals"], {"opened": 0, "listed": 2, "file": 0, "never": 2})
        self.assertEqual(result["by_repository"]["beta"], {"opened": 0, "listed": 1, "file": 0, "never": 1})


if __name__ == "__main__":
    unittest.main()
