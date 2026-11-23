# SPDX-License-Identifier: Apache-2.0
"""Synthetic tests for lg_lifetimes.py; no service, GPU or private input is used."""

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

import lg_lifetimes as lg


def write(path, value):
    data = (json.dumps(value) + "\n").encode()
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_bytes(data)
    return hashlib.sha256(data).hexdigest()


def write_base(root, series, arm, name, indexes=("bookbuilder-v2", "munarium"), aliases=True, **handle):
    """A fixture root with one base lifetime handle, as fixture.py leaves it."""
    fixtures = Path(root) / f"fixture-{series}-{arm}"
    record = {"state": "closed", "restartable": True, "indexes": {index: {"index_id": "idx"} for index in indexes}}
    record.update(handle)
    write(fixtures / name / "handle.json", record)
    if aliases:
        write(fixtures / "index-aliases.json", {"aliases": {"bookbuilder": "bookbuilder-v2"}})
    return fixtures


class LifetimesTests(unittest.TestCase):
    def pairs(self, root):
        pairs = {}
        for arm, rows in (("C", {"c": "context", "f": "focus"}), ("L", {"l": "context", "f": "focus"}),
                          ("F", {"c": "context", "f": "focus"})):
            path = root / f"{arm}.json"
            sha = write(path, {"profiles": {n: {"stage": s, "model": n} for n, s in rows.items()}})
            pairs[arm] = (str(path), sha)
        return pairs

    def test_each_arm_gets_its_root_pinned_pair_and_measured_memory(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            pairs = self.pairs(root)
            write_base(root, "06-gate-lg", "L", "06-gate-lg-L-base")
            out = lg.build("06-gate-lg", root, root / "image", pairs, {"L": "06-gate-lg-L-base"})
            self.assertEqual((out["C"]["device_gib"], out["L"]["device_gib"], out["F"]["device_gib"]), (10, 18, 3))
            self.assertIn("--seed-cpu", out["C"]["start_args"])
            self.assertEqual(out["L"]["start_args"][-2:], ["--restart-from", "06-gate-lg-L-base"])
            self.assertEqual(len({row["fixtures"] for row in out.values()}), 3)
            # A changed pair or a pair without a CUDA Focus entry is refused.
            with self.assertRaises(ValueError):
                lg.build("s", root, root, dict(pairs, C=(pairs["C"][0], "0" * 64)), {})
            sha = write(root / "nofocus.json", {"profiles": {"c": {"stage": "context", "model": "compact"}}})
            with self.assertRaises(ValueError):
                lg.build("s", root, root, dict(pairs, C=(str(root / "nofocus.json"), sha)), {})

    def test_the_lifetimes_index_the_loop_pair_and_no_airk(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            out = lg.build("06-gate-lg", root, root / "image", self.pairs(root), {})
            for row in out.values():
                at = row["start_args"].index("--repositories")
                # Index names for the launcher, with the repository that has denies first.
                self.assertEqual(row["start_args"][at + 1], "bookbuilder-v2,munarium")
                self.assertNotIn("airk", row["start_args"][at + 1].split(","))
                # Scenario repositories for the settled guard (bookbuilder resolves through the alias file).
                self.assertEqual(row["repositories"], "munarium,bookbuilder")

    def test_precision_arms_restart_their_own_base_with_the_fp32_single_entry_model(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            pairs = self.pairs(root)
            for arm in ("C32", "L32"):
                write_base(root, "06-gate-lg", arm, f"06-gate-lg-{arm}-base")
            out = lg.build("06-gate-lg", root, root / "image", pairs,
                           {"C32": "06-gate-lg-C32-base", "L32": "06-gate-lg-L32-base"})
            self.assertEqual(sorted(out), ["C", "C32", "F", "L", "L32"])
            for arm, model, device in (("C32", "compact", 16), ("L32", "large", 44)):
                row = out[arm]
                args = row["start_args"]
                self.assertEqual(args[args.index("--model") + 1], model)
                self.assertNotIn("--pair", args)
                self.assertEqual(args[-2:], ["--restart-from", f"06-gate-lg-{arm}-base"])
                self.assertEqual(args[args.index("--repositories") + 1], "bookbuilder-v2,munarium")
                self.assertEqual((row["launcher"], row["device_gib"]), ("gpu_fixture.py", device))
                self.assertTrue(row["fixtures"].endswith(f"fixture-06-gate-lg-{arm}"))
                self.assertEqual(row["lifetime"], f"06-gate-lg-{arm}")
            self.assertEqual(len({row["fixtures"] for row in out.values()}), 5)
            # Without a base a precision arm is left out; with an unknown arm or a base that is not clean, refused.
            self.assertEqual(sorted(lg.build("06-gate-lg", root, root / "image", pairs, {})), ["C", "F", "L"])
            with self.assertRaisesRegex(ValueError, "unknown arms"):
                lg.build("06-gate-lg", root, root / "image", pairs, {"CF16": "x"})
            write_base(root, "bad", "L32", "base", state="ready")
            with self.assertRaisesRegex(ValueError, "not a clean restart origin"):
                lg.build("bad", root, root / "image", pairs, {"L32": "base"})
            with self.assertRaisesRegex(ValueError, "needs its base"):
                lg.precision_row("C32", "06-gate-lg", root, root / "image", None)

    def test_a_root_gives_one_cuda_lifetime_and_a_spare_root_takes_the_arm(self):
        """A restart needs the saved configuration unchanged, so a root whose CUDA lifetime ran (or failed
        after it started) cannot serve the arm again; the arm then runs in a spare root under its own name."""
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            pairs = self.pairs(root)
            for arm in ("C", "L", "F", "C32", "L32"):
                write_base(root, "06-gate-lg", arm, f"06-gate-lg-{arm}-base")
            bases = {arm: f"06-gate-lg-{arm}-base" for arm in ("C", "L", "F")}
            self.assertEqual(lg.cuda_lifetimes(root / "fixture-06-gate-lg-L"), [])
            lg.build("06-gate-lg", root, root / "image", pairs, bases)
            write(root / "fixture-06-gate-lg-L" / "06-gate-lg-L" / "gpu.json", {"model": "pair"})
            self.assertEqual(lg.cuda_lifetimes(root / "fixture-06-gate-lg-L"), ["06-gate-lg-L"])
            with self.assertRaisesRegex(ValueError, "already spent its one CUDA lifetime"):
                lg.build("06-gate-lg", root, root / "image", pairs, bases)
            out = lg.build("06-gate-lg", root, root / "image", pairs, dict(bases, L="06-gate-lg-C32-base"),
                           {"L": "C32"})
            row = out["L"]
            self.assertTrue(row["fixtures"].endswith("fixture-06-gate-lg-C32"))
            self.assertEqual(row["lifetime"], "06-gate-lg-L")  # the arm keeps its name in the spare root
            self.assertEqual(row["start_args"][-2:], ["--restart-from", "06-gate-lg-C32-base"])
            self.assertEqual(row["device_gib"], 18)
            # The spare's base is required, a spare cannot serve two arms, and only judged arms move.
            with self.assertRaisesRegex(ValueError, "needs that root's base"):
                lg.arm_row("L", "06-gate-lg", root, root / "image", *pairs["L"], root_arm="C32")
            with self.assertRaisesRegex(ValueError, "every arm needs its own fixture root"):
                lg.build("06-gate-lg", root, root / "image", pairs,
                         dict(bases, L="06-gate-lg-C32-base", C="06-gate-lg-C32-base"), {"L": "C32", "C": "C32"})
            with self.assertRaisesRegex(ValueError, "--root maps a judged arm"):
                lg.build("06-gate-lg", root, root / "image", pairs, bases, {"C32": "L32"})

    def test_a_base_must_be_a_clean_restart_origin_of_the_pair(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            pairs = self.pairs(root)
            for number, (problem, options) in enumerate((
                    ("not a clean restart origin", {"state": "indexing"}),
                    ("not a clean restart origin", {"restartable": False}),
                    ("not a clean restart origin", {"error": "error: ValueError: x"}),
                    ("holds an airk index", {"indexes": ("airk", "bookbuilder-v2")}),
                    ("not bookbuilder-v2,munarium", {"indexes": ("bookbuilder-v2",)}),
                    ("no index-aliases.json", {"aliases": False}))):
                series = f"s{number}"
                indexes = options.pop("indexes", ("bookbuilder-v2", "munarium"))
                aliases = options.pop("aliases", True)
                write_base(root, series, "C", "base", indexes=indexes, aliases=aliases, **options)
                with self.assertRaisesRegex(ValueError, problem):
                    lg.build(series, root, root / "image", pairs, {"C": "base"})
            with self.assertRaisesRegex(ValueError, "has no handle"):
                lg.build("missing", root, root / "image", pairs, {"C": "base"})
            fixtures = write_base(root, "wrong-alias", "C", "base")
            write(fixtures / "index-aliases.json", {"aliases": {"bookbuilder": "bookbuilder"}})
            with self.assertRaisesRegex(ValueError, "does not serve bookbuilder by bookbuilder-v2"):
                lg.build("wrong-alias", root, root / "image", pairs, {"C": "base"})


if __name__ == "__main__":
    unittest.main()
