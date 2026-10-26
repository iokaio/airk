# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("storage_engine", Path(__file__).parents[1] / "check-storage-engine.py")
storage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(storage)


class StorageEngineGate(unittest.TestCase):
    def root(self, directory, packages):
        root = Path(directory)
        (root / "crates").mkdir()
        (root / "Cargo.toml").write_text('[workspace]\nmembers = []\n', encoding="utf-8")
        (root / "Cargo.lock").write_text('version = 4\n' + ''.join(
            f'[[package]]\nname = "{name}"\nversion = "1.0.0"\n' for name in packages), encoding="utf-8")
        return root

    def test_locked_sqlite_including_wasm_blocks_before_invoking_cargo(self):
        for name in ("rusqlite", "libsqlite3-sys", "sqlite-wasm-rs", "rsqlite-vfs"):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                root = self.root(directory, ["redb", name])
                with patch.object(storage.subprocess, "check_output") as cargo:
                    with self.assertRaisesRegex(ValueError, "Full redb cutover required"):
                        storage.check(root)
                    cargo.assert_not_called()

    def test_all_feature_graph_rejects_transitive_sqlite(self):
        with tempfile.TemporaryDirectory() as directory:
            root = self.root(directory, ["redb"])
            metadata = {"packages": [{"name": "redb"}, {"name": "rusqlite"}]}
            with patch.object(storage.subprocess, "check_output", return_value=json.dumps(metadata)):
                with self.assertRaisesRegex(ValueError, "build/test graph retains SQLite"):
                    storage.check(root)

    def test_manifest_aliases_and_target_development_dependencies_are_rejected(self):
        manifest = {"target": {"cfg(windows)": {"dev-dependencies": {"old_engine": {"package": "rusqlite", "version": "1"}}}}}
        with self.assertRaisesRegex(ValueError, "old_engine"):
            storage.inspect_manifest(manifest, "fixture/Cargo.toml")

    def test_redb_graph_requires_no_network_or_feature_filtering(self):
        with tempfile.TemporaryDirectory() as directory:
            root = self.root(directory, ["redb"])
            with patch.object(storage.subprocess, "check_output", return_value=json.dumps({"packages": [{"name": "redb"}]})) as cargo:
                result = storage.check(root)
                self.assertTrue(result["sqlite_free"])
                command = cargo.call_args.args[0]
                for argument in ("--locked", "--offline", "--all-features"):
                    self.assertIn(argument, command)


if __name__ == "__main__":
    unittest.main()
