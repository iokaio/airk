# SPDX-License-Identifier: Apache-2.0
"""Unit tests for the commit split on a synthetic git repository.

The repository is created under AIRK_COMMIT_SPLIT_TEST_ROOT when it is set
(the evaluation runs it under its private folder), otherwise under a fresh
temporary folder. Synthetic data only.

Run: python -I -B -m unittest discover -s scripts/arbiter-answer-eval/commit_split
"""

from __future__ import annotations

import io
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import split  # noqa: E402

LINES = "".join("line %d\n" % n for n in range(1, 7))


def git(repo: Path, *args: str) -> bytes:
    return subprocess.run(
        ["git", "-c", "core.autocrlf=false", "-c", "user.name=Synthetic", "-c",
         "user.email=synthetic@example.invalid", "-C", str(repo), *args],
        check=True, capture_output=True).stdout


class SplitTests(unittest.TestCase):
    def setUp(self):
        base = os.environ.get("AIRK_COMMIT_SPLIT_TEST_ROOT")
        if base:
            Path(base).mkdir(parents=True, exist_ok=True)
        self.work = Path(tempfile.mkdtemp(prefix="commit-split-", dir=base or None))
        self.repo = self.work / "repo"
        self.private = self.work / "private"
        self.out = self.work / "out"
        self.repo.mkdir()
        git(self.repo, "init", "-q")
        files = {
            "keep.txt": "same\n",
            "whole_b.txt": "one\n",
            "split.txt": LINES,
            "gone.txt": "bye\n",
            "crlf.txt": "a\nb\n",
            "rooted.txt": "r1\nr2\n",
        }
        for name, text in files.items():
            (self.repo / name).write_bytes(text.encode())
        git(self.repo, "add", ".")
        git(self.repo, "commit", "-q", "-m", "base")
        # Working-tree work of three series.
        (self.repo / "whole_b.txt").write_bytes(b"one\ntwo\n")
        (self.repo / "split.txt").write_bytes(
            LINES.replace("line 2\n", "line 2 (A)\n").replace("line 5\n", "line 5\nadded B\nadded C\n").encode())
        (self.repo / "gone.txt").unlink()
        (self.repo / "crlf.txt").write_bytes(b"a\r\nb\r\nc\r\n")
        (self.repo / "rooted.txt").write_bytes(b"r1\nr2 (B)\nr3 (C)\n")
        (self.repo / "new_c.txt").write_bytes(b"c only\n")
        (self.repo / "tools").mkdir()
        (self.repo / "tools" / "x.py").write_bytes(b"print('a')\n")
        (self.repo / "private.txt").write_bytes(b"not for any commit\n")
        (self.private / "snap").mkdir(parents=True)
        (self.private / "snap" / "rooted.txt").write_bytes(b"r1\nr2 (B)\n")
        self.spec = {
            "schema": split.SPEC_SCHEMA,
            "series": ["A", "B", "C"],
            "roots": {"snap": "snap"},
            "excluded": {"private.txt": "private data"},
            "globs": {"tools/*": {"series": "A", "reason": "tooling"}},
            "files": {
                "whole_b.txt": {"series": "B", "reason": "whole file"},
                "gone.txt": {"series": "B", "reason": "deletion"},
                "crlf.txt": {"series": "A", "reason": "CRLF working copy"},
                "new_c.txt": {"series": "C", "reason": "new file"},
                "split.txt": {"reason": "split", "states": {
                    "A": {"from_final": {"drop": [[6, 7]]}},
                    "B": {"from_final": {"drop": [[7, 7]]}}}, "hunks": {}},
                "rooted.txt": {"reason": "root", "states": {"B": {"root": "snap"}}, "hunks": {}},
            },
        }
        self.index_before = (self.repo / ".git" / "index").read_bytes()

    def tearDown(self):
        def writable(function, path, _error):
            os.chmod(path, stat.S_IWRITE)
            function(path)

        shutil.rmtree(self.work, onexc=writable)

    def run_split(self, spec: dict, *extra: str) -> int:
        spec_file = self.work / "spec.json"
        spec_file.write_text(json.dumps(spec), encoding="utf-8")
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            return split.main(["--repo", str(self.repo), "--spec", str(spec_file),
                               "--private-root", str(self.private), "--out", str(self.out), *extra])

    def show(self, tree: str, path: str) -> bytes | None:
        result = subprocess.run(["git", "-C", str(self.repo), "show", "%s:%s" % (tree, path)], capture_output=True)
        return result.stdout if result.returncode == 0 else None

    def test_cumulative_states_patches_and_untouched_index(self):
        self.assertEqual(self.run_split(self.spec, "--export-check", str(self.work / "export")), 0)
        manifest = json.loads((self.out / "manifest.json").read_text(encoding="utf-8"))
        trees = manifest["trees"]
        a, b, c = trees["A"], trees["B"], trees["C"]
        self.assertEqual(self.show(a, "split.txt"), LINES.replace("line 2\n", "line 2 (A)\n").encode())
        self.assertEqual(self.show(b, "split.txt"),
                         LINES.replace("line 2\n", "line 2 (A)\n").replace("line 5\n", "line 5\nadded B\n").encode())
        self.assertEqual(self.show(c, "split.txt"), (self.repo / "split.txt").read_bytes())
        self.assertEqual(self.show(a, "whole_b.txt"), b"one\n")
        self.assertEqual(self.show(b, "whole_b.txt"), b"one\ntwo\n")
        self.assertEqual(self.show(a, "gone.txt"), b"bye\n")
        self.assertIsNone(self.show(b, "gone.txt"))
        self.assertEqual(self.show(a, "crlf.txt"), b"a\nb\nc\n")
        self.assertEqual(self.show(a, "rooted.txt"), b"r1\nr2\n")
        self.assertEqual(self.show(b, "rooted.txt"), b"r1\nr2 (B)\n")
        self.assertEqual(self.show(c, "rooted.txt"), b"r1\nr2 (B)\nr3 (C)\n")
        self.assertIsNone(self.show(b, "new_c.txt"))
        self.assertEqual(self.show(c, "new_c.txt"), b"c only\n")
        self.assertEqual(self.show(a, "tools/x.py"), b"print('a')\n")
        self.assertIsNone(self.show(c, "private.txt"))
        self.assertEqual(manifest["excluded"], {"private.txt": "private data"})
        self.assertEqual(manifest["crlf_normalized"], ["crlf.txt"])
        self.assertEqual(manifest["verification"]["export"]["working_tree"], {"identical": 6, "crlf_only": 1})
        for s in "ABC":
            self.assertNotIn(b"\r", (self.out / (s + ".patch")).read_bytes())
        self.assertEqual({s: manifest["patches"][s]["files"] for s in "ABC"}, {"A": 3, "B": 4, "C": 3})
        # The real index and working tree are untouched.
        self.assertEqual((self.repo / ".git" / "index").read_bytes(), self.index_before)
        self.assertEqual(git(self.repo, "diff", "--cached", "--name-only"), b"")
        self.assertFalse((self.work / "export").exists())
        self.assertFalse((self.out / "split-index.tmp").exists())

    def test_patches_apply_in_order_to_a_fresh_index(self):
        self.assertEqual(self.run_split(self.spec), 0)
        manifest = json.loads((self.out / "manifest.json").read_text(encoding="utf-8"))
        env = dict(os.environ, GIT_INDEX_FILE=str(self.work / "check-index"))
        subprocess.run(["git", "-C", str(self.repo), "read-tree", "HEAD"], check=True, env=env)
        for s in "ABC":
            subprocess.run(["git", "-C", str(self.repo), "apply", "--cached", str(self.out / (s + ".patch"))],
                           check=True, env=env)
            tree = subprocess.run(["git", "-C", str(self.repo), "write-tree"], check=True, env=env,
                                  capture_output=True).stdout.decode().strip()
            self.assertEqual(tree, manifest["trees"][s])

    def test_a_later_series_is_added_by_rules_only(self):
        spec = json.loads(json.dumps(self.spec))
        spec["series"] = ["A", "B", "C", "D"]
        spec["files"]["new_c.txt"]["series"] = "D"
        self.assertEqual(self.run_split(spec), 0)
        manifest = json.loads((self.out / "manifest.json").read_text(encoding="utf-8"))
        self.assertIsNone(self.show(manifest["trees"]["C"], "new_c.txt"))
        self.assertEqual(self.show(manifest["trees"]["D"], "new_c.txt"), b"c only\n")
        self.assertEqual(manifest["patches"]["D"]["files"], 3)

    def test_unassigned_paths_and_bad_rules_are_refused(self):
        spec = json.loads(json.dumps(self.spec))
        del spec["files"]["new_c.txt"]
        self.assertEqual(self.run_split(spec), 1)
        spec = json.loads(json.dumps(self.spec))
        spec["files"]["split.txt"]["states"]["A"] = {"from_final": {"drop": [[1, 1]]}}
        self.assertEqual(self.run_split(spec), 1)
        spec = json.loads(json.dumps(self.spec))
        spec["files"]["split.txt"]["states"]["C"] = "HEAD"
        self.assertEqual(self.run_split(spec), 1)
        self.assertEqual((self.repo / ".git" / "index").read_bytes(), self.index_before)

    def test_from_final_revert_and_replace(self):
        base = b"a\nb\nc\n"
        final = b"a\nB\nc\nd\n"
        self.assertEqual(split.from_final(base, final, {"revert": [0]}), b"a\nb\nc\nd\n")
        self.assertEqual(split.from_final(base, final, {"revert": [1]}), b"a\nB\nc\n")
        self.assertEqual(split.from_final(base, final, {"replace": [["B", "x", 1]]}), b"a\nx\nc\nd\n")
        with self.assertRaises(split.SplitError):
            split.from_final(base, final, {"replace": [["B", "x", 2]]})
        with self.assertRaises(split.SplitError):
            split.from_final(base, final, {"replace": [], "absent": ["d"]})
        with self.assertRaises(split.SplitError):
            split.from_final(base, final, {"revert": [5]})


if __name__ == "__main__":
    unittest.main()
