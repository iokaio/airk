# SPDX-License-Identifier: Apache-2.0
"""Static checks of installers/windows/airk-service.iss that ISCC would otherwise
report only during a full local build."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "installers/windows/airk-service.iss"


def code_comment_problems(text):
    """Brace comments in [Code] end at the first `}`; an inner `{` (such as a
    `{tmp}` constant) ends the comment early and the rest parses as code."""
    start = text.index("[Code]")
    line = text.count("\n", 0, start) + 1
    problems = []
    index, quoted = start, False
    while index < len(text):
        char = text[index]
        if char == "\n":
            line += 1
            quoted = False
        elif char == "'":
            quoted = not quoted
        elif not quoted and text.startswith("//", index):
            index = text.index("\n", index) - 1
        elif not quoted and char == "{":
            end = text.index("}", index)
            if "{" in text[index + 1:end]:
                problems.append(line)
            line += text.count("\n", index, end)
            index = end
        index += 1
    return problems


class ServiceInstallerSource(unittest.TestCase):
    def test_no_line_starts_with_a_preprocessor_hash_except_directives(self):
        # ISPP reads any line whose first non-blank character is `#` as a
        # directive, so a Pascal character code (#13#10) cannot start a line.
        text = SCRIPT.read_text(encoding="utf-8")
        bad = [number for number, line in enumerate(text.splitlines(), 1)
               if re.match(r"\s*#[0-9$]", line)]
        self.assertEqual(bad, [])

    def test_code_brace_comments_do_not_contain_braces(self):
        self.assertEqual(code_comment_problems(SCRIPT.read_text(encoding="utf-8")), [])

    def test_setup_operations_and_image_live_in_the_owner_private_folder(self):
        # {tmp} inherits the user's Temp ACL, which can grant other accounts
        # access (a sandbox group did); setup refuses such an operation file.
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("Folder := SetupPrivateFolder + '\\airk-setup-' + Id;", text)
        self.assertIn("Result := SetupPrivateFolder + '\\airk-setup.exe';", text)
        self.assertNotIn("ExpandConstant('{tmp}\\airk-setup-'", text)
        restrict = re.search(r"icacls\.exe'\), '\"' \+ Folder \+\s+'\" /inheritance:r /grant:r ", text)
        self.assertIsNotNone(restrict)
        self.assertIn("or (Code <> 0) then", text)
        self.assertIn("RestrictToUser(PrivateFolder);", text)

    def test_default_locations_use_a_new_private_root(self):
        # Separate from the pre-split %LOCALAPPDATA%\AIRK root and created
        # private, so inherited grants to other accounts do not apply.
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("DefaultDirName={localappdata}\\AIRK Service\\program\n", text)
        self.assertIn("DefaultRootName = 'AIRK Service';", text)
        self.assertIn("GetPreviousData('DataRoot', DefaultRoot)", text)
        self.assertIn("GetPreviousData('CacheRoot', DefaultRoot)", text)
        self.assertNotIn("{localappdata}\\AIRK')", text)
        start = text.index("procedure PrepareDefaultRoot;")
        body = text[start:text.index("\nend;", start)]
        self.assertIn("RestrictToUser(DefaultRoot);", body)

    def test_an_old_extension_blocks_the_data_page_and_is_warned_about(self):
        # Setup's plan reports a running pre-split extension as a blocker;
        # the data page must not advance past it, and the page says to update
        # or close VS Code first (a closed editor cannot be observed).
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("(Pos('\"location_problems\":[]', Json) > 0) and (Pos('\"blockers\":[]', Json) > 0)", text)
        self.assertIn("update the AIRK extension in VS Code to {#AirkVersion} or later, or close", text)

    def test_the_checks_catch_the_defects_they_name(self):
        self.assertEqual(code_comment_problems("[Code]\n{ below {tmp} and }\nbegin end;\n"), [2])
        self.assertEqual(code_comment_problems("[Code]\nx := '{'; { fine }\n// {a{b}\n"), [])


if __name__ == "__main__":
    unittest.main()
