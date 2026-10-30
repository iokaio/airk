# SPDX-License-Identifier: Apache-2.0
"""Native test reads must use the shared typed-retry helpers or state why they are single-shot."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
TESTS = ROOT / "packages/vscode/src/test"
# The shared helpers own the retry contract; installedEditor uses the same parser.
HELPERS = {"nativeReadRetry.ts"}
NATIVE_MARKERS = ("AIRK_TEST_BINARY", "NativeServiceFixture", "AIRK_EDITOR_RUN")
RAW_CALL = re.compile(r"""\.request\(\s*['"]tools/call['"]""")
REASON = re.compile(r"//\s*native read:\s*\S")
SILENT_TS = re.compile(r"assert\.(?:notEqual|notStrictEqual)\([^,()]*isError\s*,\s*true\s*\)|assert\.ok\(\s*!\s*[\w.!?]*isError\s*\)")
SILENT_PY = re.compile(r"assert\s+not\s+[\w.\[\]\"'()]*isError[\"')\]]*\s*$")


def native_sources():
    for path in sorted(TESTS.glob("*.ts")):
        text = path.read_text(encoding="utf-8")
        if path.name not in HELPERS and any(marker in text for marker in NATIVE_MARKERS):
            yield path, text.splitlines()


class NativeReadContract(unittest.TestCase):
    def test_raw_native_tool_calls_use_the_shared_retry_or_state_a_single_shot_reason(self):
        sources = list(native_sources())
        self.assertTrue(any(path.name == "nativeSmoke.test.ts" for path, _ in sources))
        unexplained = []
        for path, lines in sources:
            for number, line in enumerate(lines):
                if not RAW_CALL.search(line) or path.name == "installedEditor.ts":
                    continue
                if not (REASON.search(line) or (number and REASON.search(lines[number - 1]))):
                    unexplained.append(f"{path.relative_to(ROOT)}:{number + 1}")
        self.assertEqual(unexplained, [], "Use requestWithReadRetry/observeQuickStatus, or add `// native read: <why exactly once>`")

    def test_installed_editor_retries_only_through_the_shared_hint_parser(self):
        text = (TESTS / "installedEditor.ts").read_text(encoding="utf-8")
        self.assertIn("readRetryDelay(result.result)", text)
        self.assertNotIn("direct_query_admission", text, "Hint fields belong to the shared parser")

    def test_error_assertions_report_the_native_reply(self):
        silent = [f"{path.relative_to(ROOT)}:{number}" for path in sorted(TESTS.glob("*.ts"))
                  for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1) if SILENT_TS.search(line)]
        silent += [f"{path.relative_to(ROOT)}:{number}" for path in sorted((ROOT / "scripts").glob("check-*.py"))
                   for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1) if SILENT_PY.search(line)]
        self.assertEqual(silent, [], "A hosted failure must include the reply that caused it")

    def test_patterns_detect_the_original_silent_failure(self):
        self.assertTrue(SILENT_TS.search("assert.notEqual(result.isError,true);return JSON.parse(x);"))
        self.assertTrue(SILENT_TS.search("assert.ok(!result.isError)"))
        self.assertFalse(SILENT_TS.search("assert.notEqual(result.isError,true,boundedReply(result))"))
        self.assertTrue(SILENT_PY.search('    assert not reply.get("isError")'))
        self.assertFalse(SILENT_PY.search('    assert not reply.get("isError"), reply'))
        self.assertTrue(RAW_CALL.search("await session.request('tools/call',{name:'status'})"))


if __name__ == "__main__":
    unittest.main()
