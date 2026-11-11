# SPDX-License-Identifier: Apache-2.0
"""Golden capture must normalize machine paths and compare outputs exactly."""
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
import unittest.mock

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("cli_golden", ROOT / "scripts" / "dev" / "cli_golden.py")
GOLDEN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GOLDEN)


def python_case(case_id, code, stdin=None):
    return {"id": case_id, "argv": ["-c", code], "stdin": stdin}


class CliGolden(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def cases(self, *cases, rules=()):
        path = self.root / "cases.json"
        path.write_text(json.dumps({"schema": GOLDEN.CASES_SCHEMA, "normalize": list(rules),
                                    "cases": list(cases)}), encoding="utf-8")
        return path

    def capture(self, cases, name="out.json"):
        out = self.root / name
        GOLDEN.capture(sys.executable, cases, out, fixture_base=self.root / "work")
        return json.loads(out.read_text(encoding="utf-8")), out

    def test_each_case_runs_without_and_with_json_errors(self):
        cases = self.cases(python_case("format", "import os;print(os.environ.get('AIRK_ERROR_FORMAT'))"))
        golden, _ = self.capture(cases)
        self.assertEqual(golden["schema"], GOLDEN.GOLDEN_SCHEMA)
        self.assertEqual([(c["error_format"], c["stdout"].strip()) for c in golden["cases"]],
                         [(False, "None"), (True, "json")])

    def test_machine_paths_and_placeholders_are_normalized(self):
        code = "import os,sys;print(os.getcwd());print(sys.argv[1]);print(sys.executable)"
        cases = self.cases({"id": "paths", "argv": ["-c", code, "{install_empty}"]})
        golden, _ = self.capture(cases)
        text = golden["cases"][0]["stdout"]
        self.assertIn("<FIXTURE>", text)
        self.assertIn("<BINARY>", text)
        self.assertNotIn(str(self.root), text)
        self.assertNotIn(str(Path.home()), text)

    def test_owner_folders_and_stdin_are_prepared(self):
        code = "import os,sys;print(sorted(os.listdir('.')));print(sys.stdin.read())"
        golden, _ = self.capture(self.cases(python_case("fixture", code, stdin="hello")))
        self.assertIn("'control', 'data', 'install-empty', 'measurement.json', 'src'",
                      golden["cases"][0]["stdout"])
        self.assertIn("hello", golden["cases"][0]["stdout"])

    def test_inherited_airk_variables_are_removed(self):
        code = "import os;print(sorted(k for k in os.environ if k.startswith('AIRK_')))"
        with unittest.mock.patch.dict(os.environ, {"AIRK_LEAK_TEST": "1"}):
            golden, _ = self.capture(self.cases(python_case("env", code)))
        self.assertEqual(golden["cases"][0]["stdout"].strip(), "[]")

    def test_non_utf8_output_is_kept_as_base64(self):
        golden, _ = self.capture(self.cases(python_case("bytes", "import sys;sys.stdout.buffer.write(b'\\xff')")))
        self.assertEqual(golden["cases"][0]["stdout_encoding"], "base64")
        self.assertEqual(golden["cases"][0]["stdout"], "/w==")

    def test_reviewed_normalization_rules_apply(self):
        cases = self.cases(python_case("timing", "print('took 123 ms')"),
                           rules=[{"pattern": r"\d+ ms", "replacement": "<MS> ms"}])
        golden, _ = self.capture(cases)
        self.assertEqual(golden["cases"][0]["stdout"].strip(), "took <MS> ms")

    def test_compare_detects_changes_and_honors_replacements(self):
        cases = self.cases(python_case("hello", "print('airk says hi')"))
        _, first = self.capture(cases, "first.json")
        _, second = self.capture(cases, "second.json")
        self.assertEqual(GOLDEN.compare(first, second), [])
        changed = self.cases(python_case("hello", "print('airkctl says hi')"))
        _, third = self.capture(changed, "third.json")
        self.assertTrue(GOLDEN.compare(first, third))
        self.assertEqual(GOLDEN.compare(first, third, [("airk says", "airkctl says")]), [])

    def test_compare_reports_missing_and_unexpected_cases(self):
        _, first = self.capture(self.cases(python_case("a", "print(1)")), "a.json")
        _, second = self.capture(self.cases(python_case("b", "print(1)")), "b.json")
        problems = GOLDEN.compare(first, second)
        self.assertTrue(any(p.startswith("missing case") for p in problems))
        self.assertTrue(any(p.startswith("unexpected case") for p in problems))

    def test_cases_must_have_unique_ids(self):
        with self.assertRaises(ValueError):
            GOLDEN.load_cases(self.cases(python_case("same", "1"), python_case("same", "2")))

    def test_committed_cases_file_is_valid(self):
        document = GOLDEN.load_cases(ROOT / "fixtures" / "cli-golden" / "cases.json")
        names = set(GOLDEN.placeholders(Path("base")))
        for case in document["cases"]:
            for part in case["argv"]:
                part.format(**{name: "x" for name in names})


if __name__ == "__main__":
    unittest.main()
