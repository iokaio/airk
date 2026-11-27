# SPDX-License-Identifier: Apache-2.0
"""The topical map and the live gather step on a small made-up repository; no service and no model."""

from __future__ import annotations

import json
import pathlib
import tempfile
import unittest

import live_gather
import topical_index

SERVICE = """namespace App.Services;

public class LogFileService
{
    private const int DefaultRetentionDays = 10;

    public int RetentionDays()
    {
        return DefaultRetentionDays;
    }
}
"""
VIEW = """<script setup lang="ts">
const logRetentionDays = ref(10)
</script>
"""
OTHER = """public class Unrelated
{
    public void Run() { }
}
"""


class LiveGatherTest(unittest.TestCase):
    def setUp(self) -> None:
        self.folder = tempfile.TemporaryDirectory()
        root = pathlib.Path(self.folder.name)
        for path, text in {"src/Services/LogFileService.cs": SERVICE, "ui/views/SettingsView.vue": VIEW,
                           "src/Other.cs": OTHER, "src/Tests/LogFileServiceTests.cs": OTHER.replace("Unrelated", "LogFileServiceTests"),
                           "docs/guide.md": "# Logs\n\nLogs are kept for a while.\n", "node_modules/x/retention.js": "DefaultRetentionDays"}.items():
            (root / path).parent.mkdir(parents=True, exist_ok=True)
            (root / path).write_text(text, encoding="utf-8", newline="\n")
        (root / "image.png").write_bytes(b"\x89PNG\0\0")
        self.root = root
        self.index = topical_index.build(root)

    def tearDown(self) -> None:
        self.folder.cleanup()

    def test_the_map_records_roles_and_defined_names_and_skips_what_cannot_be_read_live(self) -> None:
        files = self.index["files"]
        self.assertEqual(sorted(files), ["docs/guide.md", "src/Other.cs", "src/Services/LogFileService.cs",
                                         "src/Tests/LogFileServiceTests.cs", "ui/views/SettingsView.vue"])
        self.assertEqual(files["src/Tests/LogFileServiceTests.cs"]["role"], "test")
        self.assertEqual(files["docs/guide.md"]["role"], "document")
        self.assertEqual(files["src/Services/LogFileService.cs"]["defines"]["DefaultRetentionDays"], [5])
        self.assertEqual(files["docs/guide.md"]["defines"], {"Logs": [1]})
        self.assertEqual(self.index["word_names"]["retention"], ["DefaultRetentionDays", "RetentionDays", "logRetentionDays"])

    def test_a_feature_word_reaches_the_names_that_hold_it_and_every_occurrence_is_counted(self) -> None:
        repository = live_gather.Repository(self.root, self.index)
        brief = live_gather.gather(repository, {"task": "Change the default log retention from 10 days to 30 days."})
        census = {row["term"]: row for row in brief["census"]}
        self.assertEqual((census["DefaultRetentionDays"]["occurrences"], census["DefaultRetentionDays"]["files"]), (2, 1))
        self.assertEqual(census["DefaultRetentionDays"]["in"], ["src/Services/LogFileService.cs:5,9"])
        self.assertEqual(census["logRetentionDays"]["in"], ["ui/views/SettingsView.vue:2"])
        where = [row["at"] for row in brief["needs"][0]["where"]]
        self.assertEqual(where[0].split(":")[0], "src/Services/LogFileService.cs")
        self.assertTrue(any(row.startswith("ui/views/SettingsView.vue:") for row in where))
        self.assertNotIn("src/Other.cs", {row.split(":")[0] for row in where})
        self.assertIn("5 text files read live", brief["searched"])
        self.assertLessEqual(len(json.dumps(brief).encode("utf-8")), live_gather.MAX_BRIEF_BYTES)

    def test_an_anchor_that_is_nowhere_is_reported_with_no_occurrences(self) -> None:
        repository = live_gather.Repository(self.root, self.index)
        brief = live_gather.gather(repository, {"task": "t", "anchors": ["retention_days"]})
        self.assertEqual(brief["census"][0], {"term": "retention_days", "occurrences": 0, "files": 0})


if __name__ == "__main__":
    unittest.main()
