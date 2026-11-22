# SPDX-License-Identifier: Apache-2.0
"""Synthetic-repository tests for mine_pairs.py (no real repository text).

Requires AIRK_PAIRS_TEST_ROOT: an existing private directory under the repository's
git-ignored artifacts/ folder. Each test creates and removes its own subfolder there.
"""
import importlib.util
import json
import os
import shutil
import stat
import sys
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("mine_pairs", HERE / "mine_pairs.py")
mp = importlib.util.module_from_spec(spec)
sys.modules["mine_pairs"] = mp
spec.loader.exec_module(mp)

SENTINEL = "SYNTHETIC-CREDENTIAL-SENTINEL-7f3a"
CUTOFF = "2026-01-10T00:00:00Z"

LIB_V1 = """pub fn compute_total(values: &[i64]) -> i64 {
    values.iter().sum()
}
"""
LIB_V2 = """pub fn compute_total(values: &[i64]) -> i64 {
    values.iter().sum()
}

pub fn scale_values(values: &mut [i64], factor: i64) {
    for v in values.iter_mut() {
        *v *= factor;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Totals of an empty slice are zero.
    #[test]
    fn compute_total_handles_empty_input() {
        assert_eq!(compute_total(&[]), 0);
    }

    fn helper_only_in_tests() {}
}
"""
README = """# Widget library

## Totals

The `compute_total` function sums every value; scaling lives in `src/lib.rs`
and callers must pass values that cannot overflow when multiplied together.

## Short

Too short.
"""
PY_SRC = """def scale_values(data, factor):
    for i, value in enumerate(data):
        data[i] = value * factor
"""
PY_TEST = '''def test_scale_values_multiplies_each_entry():
    """Scaling multiplies each entry by the factor."""
    data = [1, 2]
    scale_values(data, 3)
'''


def git(repo, *args, date=None):
    env = dict(os.environ, GIT_AUTHOR_NAME="Synthetic", GIT_AUTHOR_EMAIL="synthetic@example.invalid",
               GIT_COMMITTER_NAME="Synthetic", GIT_COMMITTER_EMAIL="synthetic@example.invalid",
               GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)
    if date:
        env["GIT_AUTHOR_DATE"] = env["GIT_COMMITTER_DATE"] = date
    subprocess.run(["git", "-c", "init.defaultBranch=main", "-c", "core.autocrlf=false",
                    "-c", "commit.gpgsign=false", "-C", str(repo), *args], check=True, capture_output=True, env=env)


def write(repo, rel, text):
    path = repo / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def commit(repo, message, date):
    git(repo, "add", "-A")
    git(repo, "commit", "-q", "-m", message, date=date)


class MinePairsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        root = os.environ.get("AIRK_PAIRS_TEST_ROOT")
        if not root or not Path(root).is_dir():
            raise unittest.SkipTest("AIRK_PAIRS_TEST_ROOT must name an existing private directory")
        cls.tmp = Path(tempfile.mkdtemp(prefix="mine-pairs-", dir=root))
        repo = cls.repo = cls.tmp / "repo"
        repo.mkdir()
        git(repo, "init", "-q")
        write(repo, "src/lib.rs", LIB_V1)
        commit(repo, "Add the compute_total function that sums a slice of values", "2026-01-01T10:00:00Z")
        write(repo, "src/lib.rs", LIB_V2)
        commit(repo, "Add scale_values so callers can multiply every value by a factor\n\nSigned-off-by: Synthetic <synthetic@example.invalid>", "2026-01-02T10:00:00Z")
        write(repo, "README.md", README)
        write(repo, "tests/test_scale.py", PY_TEST)
        write(repo, "src/scale.py", PY_SRC)
        commit(repo, "Document the totals rule and add a Python scaling test", "2026-01-03T10:00:00Z")
        write(repo, "aikeys.json", '{"key": "%s"}\n' % SENTINEL)
        write(repo, "Cargo.lock", "# lock\n")
        commit(repo, "Store provider keys and refresh the dependency lock state", "2026-01-04T10:00:00Z")
        write(repo, "src/lib.rs", LIB_V2 + "// tweak\n")
        commit(repo, "more fixes", "2026-01-05T10:00:00Z")
        write(repo, "docs/agent-current-work.md", "# Work\n\nBenchmark text that must never be mined here.\n")
        commit(repo, "Record the current assignment for the evaluation benchmark", "2026-01-06T10:00:00Z")
        write(repo, "src/late.rs", "pub fn late_function_name() {}\n")
        commit(repo, "Add a late function after the scenario freeze cutoff date", "2026-02-01T10:00:00Z")
        cls.out = cls.tmp / "out"
        cls.requested = []
        real_blob, real_run = mp.Git.blob, mp.Git.run

        def blob(self, c, p):
            cls.requested.append(p)
            return real_blob(self, c, p)

        def run(self, *a):
            cls.requested.extend(a)
            return real_run(self, *a)

        with mock.patch.object(mp.Git, "blob", blob), mock.patch.object(mp.Git, "run", run):
            mp.main(["mine", "--repo", f"airk={repo}", "--out", str(cls.out), "--before", CUTOFF])
        cls.pairs = mp.read_jsonl(cls.out / "airk.jsonl")
        cls.stats = json.loads((cls.out / "stats.json").read_text(encoding="utf-8"))

    @classmethod
    def tearDownClass(cls):
        def writable(func, path, _exc):
            os.chmod(path, stat.S_IWRITE)
            func(path)

        shutil.rmtree(cls.tmp, onexc=writable)

    def kinds(self, kind):
        return [p for p in self.pairs if p["kind"] == kind]

    def test_commit_pairs_use_message_and_new_side_hunks(self):
        commits = self.kinds("commit")
        messages = sorted(p["instruction"].split("\n")[0] for p in commits)
        self.assertEqual(messages, ["Add scale_values so callers can multiply every value by a factor",
                                    "Add the compute_total function that sums a slice of values",
                                    "Document the totals rule and add a Python scaling test"])
        scale = next(p for p in commits if p["instruction"].startswith("Add scale_values"))
        self.assertNotIn("Signed-off-by", scale["instruction"])
        self.assertEqual(scale["evidence"], [{"path": "src/lib.rs", "lines": [4, 22]}])

    def test_short_generic_late_and_benchmark_commits_are_skipped(self):
        text = json.dumps(self.pairs)
        self.assertNotIn("more fixes", text)
        self.assertNotIn("late function", text)
        self.assertNotIn("late_function_name", text)
        self.assertNotIn("agent-current-work", text)
        self.assertNotIn("Benchmark text", text)
        ex = self.stats["exclusions"]
        self.assertEqual(ex.get("airk:commit:message_too_short"), 1)
        self.assertEqual(ex.get("airk:commit:no_evidence_after_path_filter"), 1)
        self.assertEqual(ex.get("airk:commit:credential_only"), 1)
        self.assertEqual(self.stats["mine"]["before"], CUTOFF)
        self.assertEqual(self.stats["mine"]["repos"]["airk"]["commits_after_cutoff_skipped"], 1)

    def test_credential_file_is_never_read_or_emitted(self):
        for item in self.requested:
            self.assertNotIn("aikeys", str(item))
        for path in self.out.rglob("*"):
            if path.is_file():
                self.assertNotIn(SENTINEL, path.read_text(encoding="utf-8"))
                self.assertNotIn("aikeys", path.read_text(encoding="utf-8"))

    def test_doc_section_resolves_symbol_and_path(self):
        docs = self.kinds("doc")
        self.assertEqual(len(docs), 1)
        doc = docs[0]
        self.assertTrue(doc["instruction"].startswith("## Totals"))
        self.assertEqual(doc["source"], {"path": "README.md", "lines": [3, 7]})
        self.assertIn({"path": "src/lib.rs", "lines": [1, 3]}, doc["evidence"])
        self.assertIn({"path": "src/lib.rs", "lines": [1, 23]}, doc["evidence"])  # snapshot includes the later tweak line
        self.assertEqual(self.stats["exclusions"].get("airk:doc:section_too_short"), 2)

    def test_tests_resolve_code_under_test(self):
        tests = {p["instruction"].split("`")[1]: p for p in self.kinds("test")}
        self.assertEqual(sorted(tests), ["compute_total_handles_empty_input", "test_scale_values_multiplies_each_entry"])
        rust = tests["compute_total_handles_empty_input"]
        self.assertEqual(rust["evidence"], [{"path": "src/lib.rs", "lines": [1, 3]}])
        self.assertIn("Totals of an empty slice are zero.", rust["instruction"])
        py = tests["test_scale_values_multiplies_each_entry"]
        self.assertEqual(py["evidence"], [{"path": "src/scale.py", "lines": [1, 3]}])  # same language only

    def test_test_region_helpers_are_not_definitions(self):
        snap_text = json.dumps(self.pairs)
        self.assertNotIn("helper_only_in_tests\"", snap_text)

    def test_split_is_deterministic_hash_of_id(self):
        for p in self.pairs:
            self.assertEqual(p["split"], mp.split_of(p["id"], 15))
            self.assertEqual(set(p), {"id", "repo", "kind", "commit", "instruction", "evidence", "split"} | ({"source"} if p["kind"] != "commit" else set()))
        buckets = [mp.split_of(f"x-{i}", 15) for i in range(20000)]
        share = buckets.count("heldout") / len(buckets)
        self.assertTrue(0.14 < share < 0.16, share)

    def test_note_filter_is_rerunnable_and_drops_overlaps(self):
        notes_dir = self.tmp / "notes" / "airk"
        notes_dir.mkdir(parents=True, exist_ok=True)
        (notes_dir / "C1.json").write_text(json.dumps({
            "id": "airk-C1", "repository": "airk",
            "notes": [{"id": "N1", "path": "src/lib.rs", "lines": [6, 6], "alternatives": []},
                      {"id": "N2", "path": "docs/elsewhere.md", "lines": [1, 1],
                       "alternatives": [{"path": "src/scale.py", "lines": [2, 2], "symbol": None}]}]}), encoding="utf-8")
        before = len(self.pairs)
        mp.main(["filter", "--out", str(self.out), "--exclude-notes", str(self.tmp / "notes" / "*" / "*.json")])
        kept = mp.read_jsonl(self.out / "airk.jsonl")
        stats = json.loads((self.out / "stats.json").read_text(encoding="utf-8"))
        for p in kept:
            for e in p["evidence"]:
                self.assertFalse(e["path"] == "src/lib.rs" and e["lines"][0] <= 6 <= e["lines"][1])
                self.assertFalse(e["path"] == "src/scale.py" and e["lines"][0] <= 2 <= e["lines"][1])
        dropped = before - len(kept)
        self.assertEqual(dropped, 4)  # scale commit, doc section, python test, python commit
        self.assertEqual(sum(v for k, v in stats["exclusions"].items() if k.endswith("overlaps_scenario_note")), dropped)
        self.assertEqual(len(stats["note_files"]), 1)
        mp.main(["filter", "--out", str(self.out)])
        self.assertEqual(len(mp.read_jsonl(self.out / "airk.jsonl")), before)

    def test_path_policy(self):
        self.assertEqual(mp.path_exclusion("bookbuilder", "aikeys.json"), "credential_path")
        self.assertEqual(mp.path_exclusion("bookbuilder", "src/App/appsettings.Development.json"), "credential_path")
        self.assertEqual(mp.path_exclusion("airk", "docs/arbiter/answer-evaluation/x.md"), "benchmark_path")
        self.assertEqual(mp.path_exclusion("airk", "scripts/arbiter-answer-eval/run.py"), "benchmark_path")
        self.assertIsNone(mp.path_exclusion("bookbuilder", "docs/agent-current-work.md"))
        self.assertEqual(mp.path_exclusion("airk", "web/node_modules/x/index.js"), "generated_path")
        self.assertEqual(mp.path_exclusion("airk", "Cargo.lock"), "generated_path")
        self.assertEqual(mp.path_exclusion("airk", "img/logo.png"), "binary_path")
        self.assertIsNone(mp.path_exclusion("airk", "crates/core/src/lib.rs"))
        self.assertEqual(mp.path_exclusion("airk", "x/sources/" + "ab" * 32 + "--tool.py"), "generated_path")
        self.assertTrue(mp.is_test_path("crates/x/src/a/tests.rs"))
        self.assertTrue(mp.is_test_path("src/App.Tests/WidgetTests.cs"))
        self.assertFalse(mp.is_test_path("crates/x/src/a/latest.rs"))

    def test_output_outside_private_root_is_refused(self):
        with self.assertRaises(SystemExit):
            mp.main(["filter", "--out", str(HERE)])


if __name__ == "__main__":
    unittest.main(verbosity=2)
