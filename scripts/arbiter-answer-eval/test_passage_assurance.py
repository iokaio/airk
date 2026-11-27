# SPDX-License-Identifier: Apache-2.0
"""The prototype's replies carry the build's one passage assurance, verbatim, and only with passages."""
from __future__ import annotations

import pathlib
import tempfile
import unittest

import gather_context_dose
import gather_context_oracle

# Literal from the user's decision of 27 November 2026, not read from the code under test.
ASSURANCE = "These passages are exact text taken from the files referenced. There is no need to read these ranges again."


class PassageAssurance(unittest.TestCase):
    def setUp(self) -> None:
        self.folder = tempfile.TemporaryDirectory()
        self.snapshot = pathlib.Path(self.folder.name)
        (self.snapshot / "a.py").write_text("def alpha():\n    return 1\n", encoding="utf-8")
        self.task = {"instruction": "change alpha",
                     "notes": [{"path": "a.py", "lines": [1, 2], "mark": "essential"}]}

    def tearDown(self) -> None:
        self.folder.cleanup()

    def test_the_constant_is_the_decided_sentence(self) -> None:
        self.assertEqual(gather_context_dose.PASSAGE_ASSURANCE, ASSURANCE)

    def test_a_reply_with_passages_carries_the_assurance_before_them(self) -> None:
        brief = gather_context_oracle.brief(self.task, self.snapshot, {"task": "change alpha"})
        self.assertTrue(brief["passages"])
        self.assertEqual(brief["use"], ASSURANCE)
        keys = list(brief)
        self.assertLess(keys.index("use"), keys.index("passages"))

    def test_a_reply_without_passages_carries_no_assurance(self) -> None:
        brief = gather_context_oracle.brief(self.task, self.snapshot, {"task": "change alpha"}, layout="ranges")
        self.assertFalse(brief["passages"])
        self.assertNotIn("use", brief)
        # The ranges layout's own instruction stays an explicit experiment switch.
        trusted = gather_context_oracle.brief(self.task, self.snapshot, {"task": "change alpha"},
                                              layout="ranges", trust=True)
        self.assertNotEqual(trusted["use"], ASSURANCE)


if __name__ == "__main__":
    unittest.main()
