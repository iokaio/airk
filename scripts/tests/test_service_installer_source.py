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

    def test_the_checks_catch_the_defects_they_name(self):
        self.assertEqual(code_comment_problems("[Code]\n{ below {tmp} and }\nbegin end;\n"), [2])
        self.assertEqual(code_comment_problems("[Code]\nx := '{'; { fine }\n// {a{b}\n"), [])


if __name__ == "__main__":
    unittest.main()
