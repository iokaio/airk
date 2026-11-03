# SPDX-License-Identifier: Apache-2.0
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("airk_source_guard", ROOT / "supply-chain/guard.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class SourcePolicyTests(unittest.TestCase):
    def fixture(self, root):
        manifest = root / "crates/airk-fictional/Cargo.toml"
        manifest.parent.mkdir(parents=True)
        manifest.write_text('[package]\nname="airk-fictional"\nversion.workspace=true\n', encoding="utf-8")
        (root / "Cargo.toml").write_text('[workspace]\nmembers=["crates/*"]\n[workspace.package]\nversion="1.8.8"\n', encoding="utf-8")
        rows = [{"name": "registry-fixture", "version": "1.0.0", "source": "registry+https://github.com/rust-lang/crates.io-index", "checksum": "a" * 64},
                {"name": "git-fixture", "version": "1.0.0", "source": "git+https://github.com/fictional/fixture?rev=" + "b" * 40 + "#" + "b" * 40, "checksum": None}]
        policy = {"schema": "airk.dependency-source-policy/1", "packages": rows,
                  "first_party": {"airk-fictional": "crates/airk-fictional/Cargo.toml"}}
        (root / "supply-chain").mkdir()
        (root / "supply-chain/source-policy.json").write_text(json.dumps(policy), encoding="utf-8")
        packages = [{"name": "airk-fictional", "version": "1.8.8"}, *copy.deepcopy(rows)]
        self.lock(root, packages)
        return packages, policy

    def lock(self, root, rows, version=4):
        text = f"version = {version}\n"
        for row in rows:
            text += "\n[[package]]\n"
            for key, value in row.items():
                if value is not None:
                    text += f"{key} = {json.dumps(value)}\n"
        (root / "Cargo.lock").write_text(text, encoding="utf-8")

    def test_existing_graph_reports_full_source_population_without_audit_claim(self):
        result = MODULE.validate_lock_sources()
        self.assertEqual(result["packages"], result["registry_packages"] + result["git_packages"])
        self.assertEqual(result["git_packages"], 2)
        self.assertEqual(result["first_party_packages"], 8)
        self.assertFalse(result["baseline_is_security_audit"])

    def test_changed_git_revision_url_and_registry_checksum_require_review(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original, _ = self.fixture(root)
            before = (root / "supply-chain/source-policy.json").read_bytes()
            for row, field, value in [(2, "source", original[2]["source"].replace("b" * 40, "c" * 40)),
                                       (2, "source", original[2]["source"].replace("github.com", "untrusted.invalid")),
                                       (1, "checksum", "d" * 64)]:
                modified = copy.deepcopy(original)
                modified[row][field] = value
                self.lock(root, modified)
                with self.assertRaisesRegex(ValueError, "source review required"):
                    MODULE.validate_lock_sources(root / "Cargo.lock")
                self.assertEqual((root / "supply-chain/source-policy.json").read_bytes(), before)

    def test_new_registry_and_path_dependencies_fail_even_without_a_version_collision(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original, _ = self.fixture(root)
            for row in [{"name": "new-registry-fixture", "version": "1.0.0", "source": original[1]["source"], "checksum": "e" * 64},
                        {"name": "unreviewed-path", "version": "1.8.8"}]:
                self.lock(root, original + [row])
                with self.assertRaisesRegex(ValueError, "review required|Unapproved local/path"):
                    MODULE.validate_lock_sources(root / "Cargo.lock")

    def test_source_schema_corruption_does_not_become_a_pass(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original, policy = self.fixture(root)
            self.lock(root, original, version=3)
            with self.assertRaisesRegex(ValueError, "format"):
                MODULE.validate_lock_sources(root / "Cargo.lock")
            self.lock(root, original)
            policy["packages"][0]["unexpected"] = True
            (root / "supply-chain/source-policy.json").write_text(json.dumps(policy), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "unknown or missing fields"):
                MODULE.validate_lock_sources(root / "Cargo.lock")

    def test_missing_workspace_nodes_and_malformed_workspace_metadata_refuse(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original, _ = self.fixture(root)
            self.lock(root, original[1:])
            with self.assertRaisesRegex(ValueError, "workspace package set"):
                MODULE.validate_lock_sources(root / "Cargo.lock")
            self.lock(root, original)
            (root / "Cargo.toml").write_text('workspace="malformed"\n', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "Workspace metadata is malformed"):
                MODULE.validate_lock_sources(root / "Cargo.lock")

    def test_first_party_paths_cannot_escape_or_disguise_another_package(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original, policy = self.fixture(root)
            policy["first_party"]["airk-fictional"] = "../foreign/Cargo.toml"
            (root / "supply-chain/source-policy.json").write_text(json.dumps(policy), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "escapes"):
                MODULE.validate_lock_sources(root / "Cargo.lock")

    def test_removal_is_reported_without_regenerating_approvals(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original, _ = self.fixture(root)
            self.lock(root, original[:-1])
            result = MODULE.validate_lock_sources(root / "Cargo.lock")
            self.assertEqual(result["unused_approvals"], 1)
            self.assertEqual(result["git_packages"], 0)

    def test_same_name_version_replacement_cannot_reuse_retained_approved_manifest(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, policy = self.fixture(root)
            replacement = root / "elsewhere/Cargo.toml"
            replacement.parent.mkdir()
            replacement.write_text('[package]\nname="airk-fictional"\nversion="1.8.8"\n', encoding="utf-8")
            (root / "Cargo.toml").write_text('[workspace]\nmembers=["elsewhere"]\n[workspace.package]\nversion="1.8.8"\n', encoding="utf-8")
            self.assertTrue((root / policy["first_party"]["airk-fictional"]).is_file())
            before = (root / "Cargo.lock").read_bytes()
            with self.assertRaisesRegex(ValueError, "Effective workspace members"):
                MODULE.validate_lock_sources(root / "Cargo.lock")
            self.assertEqual(before, (root / "Cargo.lock").read_bytes())
            # An intentional relocation needs an explicit reviewed inventory change.
            policy["first_party"]["airk-fictional"] = "elsewhere/Cargo.toml"
            (root / "supply-chain/source-policy.json").write_text(json.dumps(policy), encoding="utf-8")
            self.assertEqual(MODULE.validate_lock_sources(root / "Cargo.lock")["conclusion"], "success")

    def test_extra_glob_member_and_exclusions_refuse_before_cargo_resolution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            original = (root / "Cargo.toml").read_text(encoding="utf-8")
            for setting in ('exclude=["crates/airk-fictional"]', 'default-members=["crates/airk-fictional"]'):
                (root / "Cargo.toml").write_text(original.replace('[workspace]', '[workspace]\n' + setting), encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "closed workspace"):
                    MODULE.validate_lock_sources(root / "Cargo.lock")
            (root / "Cargo.toml").write_text(original, encoding="utf-8")
            (root / "crates/unapproved").mkdir()
            with self.assertRaisesRegex(ValueError, "Effective workspace members"):
                MODULE.validate_lock_sources(root / "Cargo.lock")

    def test_local_dependency_paths_are_bound_in_every_supported_table(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            manifest = root / "Cargo.toml"
            original = manifest.read_text(encoding="utf-8")
            for table in ('workspace.dependencies', 'dependencies', 'dev-dependencies', 'build-dependencies',
                          'target."cfg(windows)".dependencies', 'patch.crates-io', 'replace'):
                manifest.write_text(original + f'\n[{table}]\nairk-fictional={{path="elsewhere"}}\n', encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "Unapproved local/path"):
                    MODULE.validate_lock_sources(root / "Cargo.lock")
            manifest.write_text(original + '\n[workspace.dependencies]\nalias={package="airk-fictional",path="crates/airk-fictional"}\n', encoding="utf-8")
            self.assertEqual(MODULE.validate_lock_sources(root / "Cargo.lock")["conclusion"], "success")
            manifest.write_text(original + '\n[workspace.dependencies]\nalias={package="wrong-name",path="crates/airk-fictional"}\n', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "package identity"):
                MODULE.validate_lock_sources(root / "Cargo.lock")

    def test_linked_member_directory_is_not_an_approved_location(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, policy = self.fixture(root)
            alias = root / "alias"
            try:
                alias.symlink_to(root / "crates/airk-fictional", target_is_directory=True)
            except OSError:
                self.skipTest("Creating directory symlinks requires OS permission")
            (root / "Cargo.toml").write_text('[workspace]\nmembers=["alias"]\n[workspace.package]\nversion="1.8.8"\n', encoding="utf-8")
            policy["first_party"]["airk-fictional"] = "alias/Cargo.toml"
            (root / "supply-chain/source-policy.json").write_text(json.dumps(policy), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "alias"):
                MODULE.validate_lock_sources(root / "Cargo.lock")


if __name__ == "__main__":
    unittest.main()
