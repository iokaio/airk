# SPDX-License-Identifier: Apache-2.0
"""Read-only migration identity/closure tests; never import exporters or private data."""
import ast
import hashlib
import json
from pathlib import Path
import unittest

BASE = Path(__file__).resolve().parent
ROOT = next(p for p in BASE.parents if (p / "Cargo.toml").is_file() and (p / "crates/airk-core").is_dir())
MAP = json.loads((BASE / "source-map.json").read_bytes())
RUNNERS = {
    "exports-r1": "4d02b53cb4a103f0d52be6172019247ce953292d8922f8284679826ce3630fc7",
    "exports-r2": "01188a287fdf2725539c3504f5b7b8dad475e734f121392f66f759ae91ea8499",
    "mixed-r1": "091f2a9a97f78f430797fcee8df9d7f15507364e94cfdf62c4a60328d15b3344",
    "mixed-r2": "d32737006f4954919f7b1c8262bf0c2072a5dec68534c9bc8b7f6df23ee363d0",
    "mixed-r3": "a94bc1ceb4551edafaa3bd06aa1edf1810c2f7769ef474467da797e2a1194e3f",
    "mixed-r4": "f18d1a4c3e1d8d785886b1289780b738d1176c0069bbc6a28c357ac61e4be149",
    "mixed-r5": "20edf80e0ed35cd20e03c89dd7f765471c1f388c3b416500c6f685c715105d58",
    "mixed-r6": "772ede3f652101411432756faea45639d130f2f549d5099a4e7647013355bbea",
    "native-parent-bag-r1": "090f1b8e1c8059c7d6f47ef02b823bd991e88de7aa6c7cc66c71bbc9924dd078",
    "visible-prefix-r1": "fa500d3d2ad96dbd4e4b52d898d46064ae4115fedd813c33854d2a47bebaac72",
}

def digest(raw):
    return hashlib.sha256(raw).hexdigest()

class Migration(unittest.TestCase):
    def test_all_ten_literal_runner_identities_and_source_count(self):
        self.assertEqual(len(MAP["files"]), 60)
        rows = {Path(row["destination"]).relative_to(BASE.relative_to(ROOT)).as_posix(): row for row in MAP["files"]}
        for recipe, expected in RUNNERS.items():
            self.assertEqual(rows[recipe + "/runner.py"]["original_sha256"], expected)
        self.assertEqual(sum(name.endswith(".py") for name in rows), 56)
        self.assertEqual(sum(name.endswith(".rs.in") for name in rows), 2)

    def test_each_maintained_source_reverses_to_exact_original_bytes(self):
        for row in MAP["files"]:
            with self.subTest(path=row["destination"]):
                path = ROOT / row["destination"]
                self.assertTrue(path.resolve().is_relative_to(BASE))
                raw = path.read_bytes()
                self.assertEqual((len(raw), digest(raw)), (row["maintained_bytes"], row["maintained_sha256"]))
                for change in reversed(row["changes"]):
                    if change["kind"] in {"repository-root", "maintained-dependency", "explicit-data-root", "private-preparation-root"}:
                        old, new = change["old"].encode(), change["new"].encode()
                        self.assertEqual(raw.count(new), 1)
                        if change["kind"] == "repository-root":
                            self.assertRegex(old.decode(), r"^ROOT=Path\(__file__\)\.resolve\(\)\.parents\[\d+\]$")
                        raw = raw.replace(new, old, 1)
                    else:
                        self.assertEqual(change["kind"], "spdx-prefix")
                        prefix = change["prefix"].encode()
                        self.assertIn(prefix, (b"# SPDX-License-Identifier: Apache-2.0\n", b"# SPDX-License-Identifier: Apache-2.0\r\n"))
                        self.assertTrue(raw.startswith(prefix))
                        raw = raw[len(prefix):]
                self.assertEqual((len(raw), digest(raw)), (row["original_bytes"], row["original_sha256"]))
                original = ROOT / row["source"]
                if original.is_file():
                    self.assertEqual(raw, original.read_bytes())

    def test_all_python_parses_without_importing_and_carries_license(self):
        for path in BASE.rglob("*.py"):
            raw = path.read_bytes()
            self.assertTrue(raw.startswith(b"# SPDX-License-Identifier: Apache-2.0"))
            ast.parse(raw, filename=str(path))
            compile(raw, str(path), "exec")

    def test_no_unlisted_source_or_payload_and_every_recipe_has_readme(self):
        expected = {ROOT / row["destination"] for row in MAP["files"]}
        expected |= {BASE / "test_migration.py", BASE / "source-map.json", BASE / "support/recipe_paths.py"}
        for path in BASE.rglob("*"):
            if path.is_dir():
                self.assertTrue((path / "README.md").is_file())
            elif path.name != "README.md":
                self.assertIn(path, expected)
                self.assertNotIn(path.suffix, {".onnx", ".safetensors", ".bin", ".pt", ".pkl"})

    def test_graders_pin_maintained_source_closure(self):
        audit = BASE / "support/blind-metadata-r5/audit.py"
        grader = BASE / "visible-prefix-r1/grade_candidates.py"
        self.assertEqual(digest(audit.read_bytes()), "a1687757532d766baac52f9c78547f30c1d3b340f9370cfe961d7817b237ad77")
        self.assertEqual(digest(grader.read_bytes()), "9253fdb047c16104379e60a02129a95ca7afa14ca89056132cbf595bb37aed6b")
        for path in [grader, BASE / "native-parent-bag-r1/grade_dev.py", BASE / "native-parent-bag-r1/superseded-six-role-preparation-r1/grade_dev.py"]:
            source = path.read_text()
            self.assertIn("scripts/arbiter/export/minilm/support/blind-metadata-r5/audit.py", source)
            self.assertIn(digest(audit.read_bytes()), source)
            self.assertNotIn("a1-next-experiment-blind-metadata-r5/audit.py", source)
            if path != grader:
                self.assertIn("scripts/arbiter/export/minilm/visible-prefix-r1/grade_candidates.py", source)
                self.assertIn(digest(grader.read_bytes()), source)
                self.assertNotIn("visible-prefix-export-source-r1/grade_candidates.py", source)

    def test_audit_missing_private_root_refuses_before_budget_or_outputs(self):
        source = (BASE / "support/blind-metadata-r5/audit.py").read_bytes()
        tree = ast.parse(source)
        main = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == "main")
        self.assertIsInstance(main.body[0], ast.Global)
        self.assertEqual(ast.dump(main.body[1].test), ast.dump(ast.parse("ROOT is None", mode="eval").body))
        self.assertIsInstance(main.body[1].body[0], ast.Raise)
        self.assertEqual(ast.dump(main.body[2].value), ast.dump(ast.parse("enforce_budget()", mode="eval").body))
        root = next(n for n in tree.body if isinstance(n, ast.Assign) and any(isinstance(t, ast.Name) and t.id == "ROOT" for t in n.targets))
        self.assertIsInstance(root.value, ast.IfExp)
        self.assertIsNone(root.value.orelse.value)
        self.assertIn(b'AIRK_BLIND_METADATA_ROOT', source)

    def test_preparation_refuses_missing_or_tracked_run_root_before_writes(self):
        import os
        import runpy
        import sys
        from unittest.mock import patch
        adapters = ["visible-prefix-r1/prepare.py", "visible-prefix-r1/prepare_native.py", "visible-prefix-r1/prepare_final_native.py"]
        for folder in ["native-parent-bag-r1", "native-parent-bag-r1/superseded-six-role-preparation-r1"]:
            adapters += [folder + "/prepare_seal.py", folder + "/prepare_native_packet.py"]
        for adapter in adapters:
            for value in [None, str(BASE)]:
                with self.subTest(adapter=adapter, run_root=value), patch.dict(os.environ), patch.object(sys, "path", list(sys.path)), patch.object(Path, "write_bytes", side_effect=AssertionError("write before admission")), patch.object(Path, "write_text", side_effect=AssertionError("write before admission")), patch.object(Path, "mkdir", side_effect=AssertionError("mkdir before admission")):
                    if value is None:
                        os.environ.pop("AIRK_RECIPE_RUN_ROOT", None)
                    else:
                        os.environ["AIRK_RECIPE_RUN_ROOT"] = value
                    with self.assertRaisesRegex(ValueError, "AIRK_RECIPE_RUN_ROOT"):
                        runpy.run_path(str(BASE / adapter), run_name="_preparation_admission_probe")

    def test_private_path_helper_identity_and_fresh_output_boundary(self):
        import os
        import runpy
        from unittest.mock import patch
        helper = BASE / "support/recipe_paths.py"
        self.assertEqual(MAP["maintained_utilities"], [{"path": helper.relative_to(ROOT).as_posix(), "sha256": digest(helper.read_bytes()), "purpose": "New explicit private preparation path admission; no historical original"}])
        namespace = runpy.run_path(str(helper))
        with patch.dict(os.environ, {"AIRK_RECIPE_RUN_ROOT": str(BASE)}):
            with self.assertRaisesRegex(ValueError, "ignored directory"):
                namespace["private_run_root"](ROOT)
        for path in [BASE, BASE.parent / "escape", BASE / "README.md"]:
            with self.assertRaisesRegex(ValueError, "fresh output"):
                namespace["fresh_output"](BASE, path)

    def test_private_run_root_checks_ignore_and_tracked_descendants(self):
        import os
        import runpy
        import subprocess
        from types import SimpleNamespace
        from unittest.mock import patch
        namespace = runpy.run_path(str(BASE / "support/recipe_paths.py"))
        private = ROOT / "artifacts/v190/final-plan"
        for ignored, tracked, accepted in [(0, b"", True), (1, b"", False), (0, b"tracked-file\0", False)]:
            calls = [SimpleNamespace(returncode=ignored), SimpleNamespace(stdout=tracked)]
            with patch.dict(os.environ, {"AIRK_RECIPE_RUN_ROOT": str(private)}), patch.object(Path, "resolve", lambda self, strict=False: self), patch.object(Path, "is_dir", return_value=True), patch.object(subprocess, "run", side_effect=calls) as run:
                if accepted:
                    self.assertEqual(namespace["private_run_root"](ROOT), private.resolve())
                else:
                    with self.assertRaisesRegex(ValueError, "ignored and contain no tracked"):
                        namespace["private_run_root"](ROOT)
                self.assertEqual(run.call_args_list[0].args[0][-4:], ["check-ignore", "-q", "--", "artifacts/v190/final-plan"])
                self.assertEqual(run.call_args_list[1].args[0][-4:], ["ls-files", "-z", "--", "artifacts/v190/final-plan"])

    def test_missing_historical_helper_seals_stay_explicit(self):
        gaps = [row for row in MAP["files"] if row["dependency_status"] == "current-retained-helper-not-sealed-in-historical-run"]
        self.assertEqual({Path(row["destination"]).parent.name for row in gaps}, {"mixed-r3", "mixed-r4", "mixed-r5"})
        self.assertEqual({row["original_sha256"] for row in gaps}, {"ecad6215b984f2a82f07e01f3c5e0b7f86557838705a1461b3c1800d10d2202a", "fc231b9da78778583fbd3506d0a02e8f7d76c4ae93f82e09b6714b548a09c4b0"})
        self.assertEqual(len(MAP["identical_original_groups"]), 11)

if __name__ == "__main__":
    unittest.main(verbosity=2)
