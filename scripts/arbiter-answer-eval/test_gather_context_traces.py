# SPDX-License-Identifier: Apache-2.0
"""Controls for evaluator-side parsing of the development walkthroughs."""

from pathlib import Path
import unittest

from gather_context_traces import parse_traces


DOCS = Path(__file__).resolve().parents[2] / "docs" / "arbiter"
OPENAI = "\n".join([
    "## Walkthroughs",
    "#### Bookbuilder C1 · Example",
    "",
    "> Read this",
    ">   thing.",
    "",
    "**My reading.** Need a result.",
    "",
    "**1.** Find the owner.",
    "",
    "~~~powershell",
    "Find-Paths 'needle'",
    "~~~",
    "",
    "**Found.** A path without a range.",
    "",
    "**2. Check the public contract.** The source was external.",
    "",
    "**Handoff.** The path is a lead.",
    "",
    "## Next section",
])


class TraceParserTests(unittest.TestCase):
    def test_keeps_source_markdown_and_coordinates_without_range_credit(self):
        trace = parse_traces("openai", OPENAI)[("bookbuilder", "C1")]
        self.assertEqual(trace["instruction"], "Read this thing.")
        self.assertEqual(trace["prerequisite"], {
            "document_lines": [7, 7],
            "source_wording": "**My reading.** Need a result.",
        })
        first, second = trace["steps"]
        self.assertEqual(first["document_lines"], [9, 15])
        self.assertEqual(first["commands"], [{
            "language": "powershell",
            "document_lines": [12, 12],
            "source_wording": "Find-Paths 'needle'",
        }])
        self.assertEqual(first["observations"], [{
            "kind": "observed",
            "document_line": 15,
            "source_wording": "A path without a range.",
        }])
        self.assertIn("source was external", second["source_wording"])
        self.assertEqual(second["commands"], [])
        self.assertEqual(trace["proof_limits"]["source_range_coverage"],
                         "requires_independent_verification")

    def test_refuses_duplicate_case_step_gap_and_missing_handoff(self):
        for text, error in [
            (OPENAI + "\n" + OPENAI, "Duplicate walkthrough case"),
            (OPENAI.replace("**2.", "**3."), "Nonconsecutive walkthrough step"),
            (OPENAI.replace("**Handoff.**", "Handoff."), "Missing or misplaced"),
        ]:
            with self.subTest(error=error), self.assertRaisesRegex(ValueError, error):
                parse_traces("openai", text)

    def test_retains_turn_expectation_and_result_separately_from_stop(self):
        text = "\n".join([
            "#### Munarium T1 (test)",
            "> Find it.",
            "",
            "**Anchors.** A word.",
            "",
            "1. **Turn 1, one call.** Grep. *Expect:* a file. *Got:* a directory.",
            "2. **Turn 2, one call.** Inspect it.",
            "",
            "**Context in hand.** The governing file.",
            "",
            "**Stop.** Nothing was executed.",
        ])
        trace = parse_traces("anthropic", text)[("munarium", "T1")]
        self.assertEqual(trace["steps"][0]["observations"], [
            {"kind": "expected", "document_line": 6, "source_wording": "a file."},
            {"kind": "observed", "document_line": 6, "source_wording": "a directory."},
        ])
        self.assertIn("The governing file.", trace["handoff"]["source_wording"])
        self.assertIn("Nothing was executed.", trace["stop"]["source_wording"])
        self.assertNotIn("Context in hand", trace["steps"][1]["source_wording"])

    def test_walkthrough_documents_have_the_development_roster(self):
        expected = {(repo, slot) for repo in ("bookbuilder", "munarium")
                    for slot in ("A1", "A2", "A3", "C1", "C2", "C3", "T1", "T2", "W1", "W2")}
        for author, count in [("openai", 101), ("anthropic", 109)]:
            with self.subTest(author=author):
                text = (DOCS / f"retrieval-strategy-{author}.md").read_text(encoding="utf-8")
                parsed = parse_traces(author, text)
                self.assertEqual(set(parsed), expected)
                self.assertEqual(sum(len(case["steps"]) for case in parsed.values()), count)
                lines = text.splitlines()
                for case in parsed.values():
                    for step in case["steps"]:
                        start, end = step["document_lines"]
                        self.assertEqual("\n".join(lines[start - 1:end]), step["source_wording"])
