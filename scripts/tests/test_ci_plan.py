# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("ci_plan", Path(__file__).parents[1] / "ci-plan.py")
plan = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plan)


class PlanTests(unittest.TestCase):
    def test_only_verified_pinned_repair_can_skip_rebuilding_on_main(self):
        with patch.object(plan, "repair_scope", return_value={}) as verified:
            for event in ("pull_request", "push"):
                self.assertEqual(plan.pinned_repair_plan(event),
                                 dict(native=False, container=False, extension=False, contracts=True))
            self.assertEqual(verified.call_count, 2)
            for event in ("workflow_dispatch", "workflow_call"):
                self.assertIsNone(plan.pinned_repair_plan(event))
            self.assertEqual(verified.call_count, 2)
        for failure in (ValueError("changed inputs"), FileNotFoundError("removed repair"),
                        plan.subprocess.CalledProcessError(1, ["git", "merge-base"])):
            with self.subTest(failure=failure), patch.object(plan, "repair_scope", side_effect=failure):
                self.assertIsNone(plan.pinned_repair_plan("push"))

    def test_docs_and_extension_content(self):
        self.assertFalse(any(plan.classify(["docs/a.md", "README.md"], "pull_request").values()))
        self.assertEqual(plan.classify(["packages/vscode/README.md"], "pull_request"),
                         dict(native=False, container=False, extension=True, contracts=False))

    def test_unknown_deleted_and_renamed_code_requires_full_validation(self):
        for paths in (["Cargo.lock"], ["new-file"], ["crates/old.rs", "docs/new.md"],
                      [".github/workflows/ci.yml"], ["Dockerfile"], [], ["scripts/package-native.py"]):
            with self.subTest(paths=paths):
                self.assertTrue(all(plan.classify(paths, "pull_request").values()))

    def test_main_and_manual_build_full_candidates(self):
        for event in ("push", "workflow_dispatch", "workflow_call"):
            self.assertTrue(all(plan.classify(["docs/a.md"], event).values()))

    def test_typescript_requires_real_adapter(self):
        self.assertTrue(plan.classify(["packages/vscode/src/native.ts"], "pull_request")["native"])

    def test_gate_accepts_only_the_exact_planned_results(self):
        selected = plan.classify(["docs/a.md"], "pull_request")
        results = {"plan": "success", "quick": "success", **dict.fromkeys(selected, "skipped")}
        plan.require_results(selected, results)
        for name in results:
            for wrong in ("failure", "cancelled", "unexpected"):
                with self.subTest(name=name, wrong=wrong), self.assertRaises(ValueError):
                    plan.require_results(selected, {**results, name: wrong})
        full = dict.fromkeys(selected, True)
        with self.assertRaises(ValueError):
            plan.require_results(full, results)

    def test_native_matrix_gates_three_hosts_on_prs_and_intel_only_after_promotion(self):
        targets = lambda *args: [row["target"] for row in plan.native_matrix(plan.native_targets(*args))["include"]]
        every = [row["target"] for row in plan.NATIVE_ROWS]
        self.assertEqual(targets("pull_request", False), ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc", "aarch64-apple-darwin"])
        self.assertEqual(targets("push", True), ["x86_64-apple-darwin"])
        self.assertEqual(targets("push", False), every)
        for event in ("workflow_dispatch", "workflow_call"):
            self.assertEqual(targets(event, False), every)
        self.assertEqual(targets("pull_request", False, True), every, "qualification always covers all hosts")
        self.assertEqual(plan.NATIVE_ROWS[3]["timeout_minutes"], 120)
        with self.assertRaises(ValueError):
            plan.native_matrix(("unknown-target",))

    def test_dependency_toolchain_and_ci_changes_rebuild_every_target_on_main(self):
        for paths in ([], ["Cargo.lock"], ["crates/airk-core/Cargo.toml"], ["rust-toolchain.toml"], [".cargo/config.toml"],
                      ["Dockerfile"], ["scripts/ci-prune-target.py"], [".github/workflows/ci.yml"], [".config/nextest.toml"],
                      ["crates/airk-core/src/lib.rs", "Cargo.lock"]):
            with self.subTest(paths=paths):
                self.assertTrue(plan.requires_full_main(paths))
        for paths in (["crates/airk-core/src/lib.rs"], ["docs/a.md"], ["packages/vscode/src/native.ts", "scripts/ci-plan.py"]):
            with self.subTest(paths=paths):
                self.assertFalse(plan.requires_full_main(paths))

    def test_promoted_main_requires_only_the_intel_native_job(self):
        promoted = plan.promoted_plan()
        plan.require_results(promoted, {"plan": "success", "quick": "success", "native": "success",
                                        "container": "skipped", "extension": "skipped", "contracts": "skipped"})
        with self.assertRaises(ValueError):
            plan.require_results(promoted, {"plan": "success", "quick": "success", "native": "skipped",
                                            "container": "skipped", "extension": "skipped", "contracts": "skipped"})

    def run_push_plan(self, diff, promote=True):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "event.json").write_text(json.dumps({"before": "b" * 40, "after": "a" * 40}), encoding="utf-8")
            environment = {"GITHUB_EVENT_NAME": "push", "GITHUB_REF": "refs/heads/main", "GITHUB_SHA": "a" * 40,
                           "GITHUB_EVENT_PATH": str(root / "event.json"), "GITHUB_OUTPUT": str(root / "output")}
            def check_output(command, *args, **kwargs):
                if command[:2] == ["git", "diff"]:
                    if isinstance(diff, Exception):
                        raise diff
                    return "\0".join(diff).encode()
                raise AssertionError(command)
            with patch.dict(plan.os.environ, environment), patch("sys.argv", ["ci-plan.py", "plan", "--promotion", str(root / "promotion")]), \
                    patch.object(plan.subprocess, "check_output", side_effect=check_output), \
                    patch.object(plan, "pinned_repair_plan", return_value=None), \
                    patch.object(plan, "try_promotion", return_value=promote) as attempted:
                plan.main()
            outputs = dict(line.split("=", 1) for line in (root / "output").read_text(encoding="utf-8").splitlines())
            return outputs, attempted

    def test_push_plans_promote_code_changes_and_rebuild_everything_otherwise(self):
        outputs, attempted = self.run_push_plan(["crates/airk-core/src/lib.rs"])
        attempted.assert_called_once()
        self.assertEqual((outputs["promoted"], json.loads(outputs["plan"])), ("true", plan.promoted_plan()))
        self.assertEqual([row["target"] for row in json.loads(outputs["native_matrix"])["include"]], list(plan.MAIN_TARGETS))
        for diff in (["Cargo.lock"], plan.subprocess.CalledProcessError(128, ["git", "diff"])):
            with self.subTest(diff=diff):
                outputs, attempted = self.run_push_plan(diff)
                attempted.assert_not_called()
                self.assertEqual(outputs["promoted"], "false")
                self.assertTrue(all(json.loads(outputs["plan"]).values()))
                self.assertEqual(len(json.loads(outputs["native_matrix"])["include"]), 4)
        outputs, _ = self.run_push_plan(["crates/airk-core/src/lib.rs"], promote=False)
        self.assertEqual((outputs["promoted"], len(json.loads(outputs["native_matrix"])["include"])), ("false", 4))

    def test_any_promotion_doubt_falls_back_to_the_complete_build(self):
        commit = "a" * 40
        def checkout(command, text):
            return commit if command[-1] == "HEAD" else "c" * 40
        for failure in (ValueError("tree differs"), RuntimeError("API unavailable"), KeyError("artifacts")):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary, \
                    patch.object(plan.subprocess, "check_output", side_effect=checkout), \
                    patch.object(plan, "candidates") as module:
                module.return_value.promote.side_effect = failure
                self.assertFalse(plan.try_promotion(commit, Path(temporary) / "promotion"))
                self.assertFalse((Path(temporary) / "promotion").exists())
        with tempfile.TemporaryDirectory() as temporary, \
                patch.object(plan.subprocess, "check_output", side_effect=lambda command, text: "b" * 40), \
                patch.object(plan, "candidates") as module:
            self.assertFalse(plan.try_promotion(commit, Path(temporary) / "promotion"), "another checkout never promotes")
            module.return_value.promote.assert_not_called()
        record = dict(schema="airk-ci-promotion/1", pull_request=5, producer=dict(run_id="777"))
        with tempfile.TemporaryDirectory() as temporary, \
                patch.object(plan.subprocess, "check_output", side_effect=checkout), \
                patch.object(plan, "candidates") as module:
            module.return_value.promote.return_value = record
            self.assertTrue(plan.try_promotion(commit, Path(temporary) / "promotion"))
            self.assertEqual(json.loads((Path(temporary) / "promotion/promotion.json").read_text(encoding="utf-8")), record)


if __name__ == "__main__":
    unittest.main()
