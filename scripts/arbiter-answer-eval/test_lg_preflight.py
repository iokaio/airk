# SPDX-License-Identifier: Apache-2.0
"""Synthetic tests for lg_preflight.py; no service, GPU, git tree or private input is used."""

import copy
import json
import tempfile
import unittest
from pathlib import Path

import common
import lg_lifetimes as lg
import lg_preflight as pf
from test_lg_lifetimes import write, write_base

SERIES = "06-gate-lg"
MANIFESTS = {"context-compact-fp16-cuda": "c" * 64, "context-large-fp16-cuda": "d" * 64, "focus-1024-cuda": "f" * 64}
# The checks that read only the inputs' own files (the others read the git tree, the device or run the suite).
FILE_CHECKS = tuple(row for row in pf.CHECKS if row[0] in ("image", "default budget", "scenario freeze", "pairs",
                                                           "roots", "lifetimes file", "disk"))


def gate_inputs(root: Path) -> dict:
    image = root / "image"
    for name in ("service.exe", "cli.exe"):
        write(image / name, {"file": name})
    write(image / "images.json", {"git_head": "a" * 40, **{
        part: {"image": {"path": str(image / name), "sha256": common.sha256_file(image / name)}}
        for part, name in (("service", "service.exe"), ("cli", "cli.exe"))}})
    pairs = {}
    for arm, context in (("C", "context-compact-fp16-cuda"), ("L", "context-large-fp16-cuda")):
        folder = root / f"pair-{arm}"
        roster = write(folder / "roster.json", {"entries": [context]})
        sha = write(folder / "pair.json", {
            "roster": {"path": str(folder / "roster.json"), "sha256": roster},
            "profiles": {context: {"stage": "context", "model": context, "manifest": MANIFESTS[context]},
                         "focus-1024-cuda": {"stage": "focus", "model": "focus",
                                             "manifest": MANIFESTS["focus-1024-cuda"]}}})
        pairs[arm] = {"path": str(folder / "pair.json"), "sha256": sha}
    pairs["F"] = dict(pairs["C"])
    bases = {arm: f"{SERIES}-{arm}-base" for arm in ("C", "L", "F")}
    for arm in ("C", "L", "F", "C32", "L32"):
        write_base(root, SERIES, arm, f"{SERIES}-{arm}-base")
    freeze = write(root / "scenario-freeze.json", {"files": {}})
    rows = lg.build(SERIES, root, image, {arm: (row["path"], row["sha256"]) for arm, row in pairs.items()}, bases)
    write(root / "lifetimes.json", rows)
    return {"series": SERIES, "r_root": str(root), "commit": "a" * 40, "image": str(image),
            "scenario_freeze_sha256": freeze, "prompts_freeze": str(root / "prompts-freeze.json"),
            "lifetimes": str(root / "lifetimes.json"), "pairs": pairs, "manifests": dict(MANIFESTS), "bases": bases,
            "roots": {}, "spares": {"C32": f"{SERIES}-C32-base", "L32": f"{SERIES}-L32-base"},
            "disk_floor_bytes": 0, "disk_need_bytes": 1, "device_need_gib": 35, "host_need_gib": 64}


class PreflightTests(unittest.TestCase):
    def refused(self, inputs: dict) -> dict:
        receipt = pf.run(inputs, FILE_CHECKS)
        return {row["check"]: row["problem"] for row in receipt["checks"] if not row["ok"]}

    def test_matching_inputs_pass_and_each_mismatch_is_refused_by_name(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            inputs = gate_inputs(root)
            receipt = pf.run(inputs, FILE_CHECKS)
            self.assertTrue(receipt["passed"], receipt)
            found = {row["check"]: row["found"] for row in receipt["checks"]}
            self.assertEqual(found["roots"]["spares"], {"C32": "free", "L32": "free"})
            self.assertEqual(found["default budget"]["default_budget"], common.ANSWER_BUDGET_DEFAULT)
            # One wrong value each: the check that owns it refuses, and the others still report.
            cases = (
                ("image", dict(inputs, commit="b" * 40), "was built from"),
                ("scenario freeze", dict(inputs, scenario_freeze_sha256="0" * 64), "not the frozen scenarios"),
                ("pairs", dict(inputs, manifests=dict(MANIFESTS, **{"focus-1024-cuda": "0" * 64})), "has manifest"),
                ("pairs", dict(inputs, pairs=dict(inputs["pairs"], L=dict(inputs["pairs"]["L"], sha256="0" * 64))),
                 "differs from its pin"),
                ("default budget", dict(inputs, default_budget=dict(common.ANSWER_BUDGET_DEFAULT, input_tokens=1)),
                 "default budget is"),
                ("roots", dict(inputs, bases=dict(inputs["bases"], C="missing")), "has no handle"),
                ("lifetimes file", dict(inputs, image=str(root / "other-image")), "names another image"),
                ("disk", dict(inputs, disk_floor_bytes=1 << 62), "bytes free"),
            )
            for check, changed, text in cases:
                problems = self.refused(changed)
                self.assertIn(check, problems, check)
                self.assertIn(text, problems[check], check)
            # A root that has had a CUDA lifetime is refused; pointing the arm at a spare root passes again.
            write(root / f"fixture-{SERIES}-L" / f"{SERIES}-L" / "gpu.json", {"model": "pair"})
            self.assertIn("already spent its CUDA lifetime", self.refused(inputs)["roots"])
            moved = copy.deepcopy(inputs)
            moved["roots"] = {"L": "C32"}
            moved["bases"]["L"] = f"{SERIES}-C32-base"
            problems = self.refused(moved)
            self.assertNotIn("roots", problems)
            self.assertIn("lifetimes file", problems)  # the old lifetimes file still names the spent root
            rows = lg.build(SERIES, root, root / "image",
                            {arm: (row["path"], row["sha256"]) for arm, row in inputs["pairs"].items()},
                            moved["bases"], moved["roots"])
            write(root / "lifetimes-2.json", rows)
            moved["lifetimes"] = str(root / "lifetimes-2.json")
            self.assertEqual(self.refused(moved), {})
            self.assertEqual(pf.run(moved, FILE_CHECKS)["checks"][4]["found"]["spares"], {"L32": "free"})

    def test_every_check_runs_to_its_end(self):
        with tempfile.TemporaryDirectory() as folder:
            inputs = gate_inputs(Path(folder))
            broken = dict(inputs, scenario_freeze_sha256="0" * 64, disk_floor_bytes=1 << 62)
            receipt = pf.run(broken, FILE_CHECKS)
            self.assertFalse(receipt["passed"])
            self.assertEqual(sorted(row["check"] for row in receipt["checks"] if not row["ok"]),
                             ["disk", "scenario freeze"])
            self.assertEqual(len(receipt["checks"]), len(FILE_CHECKS))
            self.assertEqual({name for name, _ in pf.CHECKS} - {name for name, _ in FILE_CHECKS},
                             {"harness tree", "prompts freeze", "memory", "CUDA math environment",
                              "plan smoke test", "harness suite"})

    def test_the_plan_must_smoke_every_arm_before_its_replays_and_sessions(self):
        import make_loop_spec
        with tempfile.TemporaryDirectory() as folder:
            r = Path(folder)
            spec = make_loop_spec.gate_spec("06-gate-lg", r, make_loop_spec.PROFILES, ["munarium"], {})
            plan = make_loop_spec.gate_plan(r / "spec.json", spec, r)
            path = r / "passes" / "06-gate-lg" / "plan.json"
            path.parent.mkdir(parents=True)
            path.write_text(json.dumps(plan))
            inputs = {"r_root": str(r), "series": "06-gate-lg"}
            self.assertEqual(pf.check_plan_smoke(inputs)["smoked"], ["C", "F", "L"])
            path.write_text(json.dumps([s for s in plan if not ("parallel" in s and s["parallel"][0]["gate"]["command"] == "smoke")]))
            with self.assertRaises(common.HarnessError):
                pf.check_plan_smoke(inputs)


if __name__ == "__main__":
    unittest.main()
