# SPDX-License-Identifier: Apache-2.0
import importlib.util
from pathlib import Path
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


if __name__ == "__main__":
    unittest.main()
