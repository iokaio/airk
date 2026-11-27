# SPDX-License-Identifier: Apache-2.0
"""The third version's map and reply sections on a small made-up repository."""

from __future__ import annotations

import pathlib
import tempfile
import unittest

import live_gather
import live_gather_v3 as v3

FILES = {
    "src/Services/LogFileService.cs": "public class LogFileService\n{\n    private const int DefaultRetentionDays = 10;\n}\n",
    "src/Tests/LogFileServiceTests.cs": "public class LogFileServiceTests\n{\n    public void Keeps() { }\n}\n",
    "docs/logs.md": "# Logs\n\nLog retention is 10 days by default; see `DefaultRetentionDays`.\n",
    "installer/App.wixproj": "<Project>\n  <PropertyGroup>\n    <ProductVersion>4.0.7</ProductVersion>\n  </PropertyGroup>\n</Project>\n",
    "CLAUDE.md": "# Rules\n\n## Logging retention defaults\nA change to a log retention default needs a test that pins the days value.\n",
}


class ThirdVersionTest(unittest.TestCase):
    def setUp(self) -> None:
        self.folder = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.folder.name)
        filler = {f"src/Other/Unrelated{number}.cs": f"public class Unrelated{number} {{ }}" for number in range(12)}
        for path, text in {**FILES, **filler}.items():
            (self.root / path).parent.mkdir(parents=True, exist_ok=True)
            (self.root / path).write_text(text, encoding="utf-8", newline="\n")
        self.index = v3.build(self.root)

    def tearDown(self) -> None:
        self.folder.cleanup()

    def test_the_map_gains_keys_values_sections_test_links_and_document_links(self) -> None:
        files = self.index["files"]
        self.assertEqual(files["installer/App.wixproj"]["defines"]["ProductVersion"], [3])
        self.assertIn("ProductVersion", self.index["word_names"]["version"])
        self.assertEqual(files["src/Services/LogFileService.cs"]["values"], [["DefaultRetentionDays", 3, "time", 864000.0]])
        self.assertEqual(files["docs/logs.md"]["quantities"], [[3, "time", 864000.0]])
        self.assertEqual(self.index["tests_of"], {"src/Services/LogFileService.cs": ["src/Tests/LogFileServiceTests.cs"]})
        self.assertEqual(self.index["documented_in"], {"DefaultRetentionDays": ["docs/logs.md:1-3"]})
        self.assertEqual([row["heading"] for row in files["CLAUDE.md"]["sections"]], ["Rules", "Logging retention defaults"])

    def test_the_reply_names_the_same_quantity_the_tests_and_where_a_name_is_documented(self) -> None:
        repository = live_gather.Repository(self.root, self.index)
        brief = v3.gather(repository, {"task": "Change the default log retention from 10 days to 30 days and pin it with a test."})
        self.assertEqual({row["at"] for row in brief["same_quantity"]}, {"src/Services/LogFileService.cs:3", "docs/logs.md:3"})
        self.assertEqual(brief["tests"], [{"file": "src/Services/LogFileService.cs", "tests": ["src/Tests/LogFileServiceTests.cs"]}])
        self.assertEqual(brief["documented"], ["DefaultRetentionDays: docs/logs.md:1-3"])
        self.assertEqual(brief["rules"], [{"at": "CLAUDE.md:3-4", "why": "rule section: Logging retention defaults"}])
        located = v3.scored(brief)
        self.assertIn("docs/logs.md:3-3", located["omitted"])
        self.assertIn("src/Tests/LogFileServiceTests.cs", located["next"]["paths"])


if __name__ == "__main__":
    unittest.main()
