# SPDX-License-Identifier: Apache-2.0
"""The further aspects of the topical index, each on a small made-up input."""

from __future__ import annotations

import unittest

import topical_aspects as aspects


def record(role: str) -> dict:
    return {"role": role}


class AspectsTest(unittest.TestCase):
    def test_a_test_file_is_linked_to_the_source_file_it_names(self) -> None:
        files = {"src/Services/LogFileService.cs": record("source"), "src/Tests/LogFileServiceTests.cs": record("test"),
                 "ui/src/views/SettingsView.vue": record("source"), "ui/src/__tests__/SettingsView.test.ts": record("test"),
                 "client/rest.py": record("source"), "client/tests/test_rest.py": record("test"),
                 "client/tests/test_unrelated_thing.py": record("test")}
        self.assertEqual(aspects.test_links(files), {"client/rest.py": ["client/tests/test_rest.py"],
                                                     "src/Services/LogFileService.cs": ["src/Tests/LogFileServiceTests.cs"],
                                                     "ui/src/views/SettingsView.vue": ["ui/src/__tests__/SettingsView.test.ts"]})

    def test_a_named_value_carries_its_quantity_when_the_name_or_wrapper_says_the_unit(self) -> None:
        text = "pub const MAX_TTL_SECS: u64 = 86_400;\nprivate const int DefaultRetentionDays = 10;\nvar timeout = TimeSpan.FromMinutes(5);\nlimit = 500\n"
        got = {row["name"]: row for row in aspects.values_of(text)}
        self.assertEqual((got["MAX_TTL_SECS"]["line"], got["MAX_TTL_SECS"]["kind"], got["MAX_TTL_SECS"]["base"]), (1, "time", 86400.0))
        self.assertEqual(got["DefaultRetentionDays"]["base"], 864000.0)
        self.assertEqual((got["timeout"]["kind"], got["timeout"]["base"]), ("time", 300.0))
        self.assertNotIn("kind", got["limit"])

    def test_the_same_quantity_in_words_meets_the_named_value(self) -> None:
        stated = aspects.quantities_of("Tokens have a hard cap of 24 h.\nKeep logs for 10 days, at most 500 files.\n")
        self.assertEqual([(row["line"], row["kind"], row["base"]) for row in stated],
                         [(1, "time", 86400.0), (2, "time", 864000.0), (2, "file", 500.0)])

    def test_project_settings_and_script_files_give_their_keys(self) -> None:
        self.assertEqual(aspects.config_keys("a/App.wixproj", "<Project>\n  <PropertyGroup>\n    <ProductVersion>4.0.7</ProductVersion>\n"),
                         {"ProductVersion": [3]})
        self.assertEqual(aspects.config_keys("appsettings.json", '{\n  "AI": {\n    "RequestTimeoutSeconds": 120\n'),
                         {"RequestTimeoutSeconds": [3]})
        self.assertEqual(aspects.config_keys("scripts/release.ps1", "param(\n  [string]$Version,\n  [switch]$WithBundle\n)\nfunction Get-Stamp {\n"),
                         {"Version": [2], "WithBundle": [3], "Get-Stamp": [5]})
        self.assertEqual(aspects.config_keys("src/a.cs", "<ProductVersion>1</ProductVersion>"), {})

    def test_a_guidance_file_governs_its_folder_and_below_and_is_read_by_section(self) -> None:
        files = {"CLAUDE.md": record("guidance"), "pro/AGENTS.md": record("guidance"), "other/CLAUDE.md": record("guidance"),
                 "pro/src/a.cs": record("source")}
        self.assertEqual(aspects.governing("pro/src/a.cs", files), ["pro/AGENTS.md", "CLAUDE.md"])
        sections = aspects.sections_of("Intro line\n\n# Keys\nKeys are encrypted; see `KeyEncryptionService.Decrypt()`.\n\n## Tests\nRun `test-all.ps1`.\n")
        self.assertEqual([(row["heading"], row["start"], row["end"]) for row in sections], [("", 1, 2), ("Keys", 3, 5), ("Tests", 6, 7)])
        self.assertEqual(sections[1]["names"], ["Decrypt"])
        self.assertIn("encrypted", sections[1]["words"])

    def test_entry_points_and_public_names(self) -> None:
        self.assertTrue(aspects.entry_point("src/Host/Program.cs", ""))
        self.assertTrue(aspects.entry_point("src/api.rs", ".route(a)\n.route(b)\n.route(c)\n.route(d)\n"))
        self.assertFalse(aspects.entry_point("src/util.rs", ".route(a)\n"))
        self.assertEqual(aspects.public_names("public class A\n{\n    private int _b;\n    pub fn c() {}\n", {"A": [1], "_b": [3], "c": [4]}),
                         ["A", "c"])


if __name__ == "__main__":
    unittest.main()
