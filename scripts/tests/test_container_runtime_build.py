# SPDX-License-Identifier: Apache-2.0
"""Container provenance seals real COPY inputs without inventing Git provenance."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("container_runtime_build", Path(__file__).parents[1] / "build-service-container.py")
BUILD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILD)


class ContainerSourceSealTests(unittest.TestCase):
    def fixture(self, root):
        dockerfile = ("FROM toolchain AS source\n"
                      "COPY Dockerfile .dockerignore Cargo.toml Cargo.lock rust-toolchain.toml ./\n"
                      "COPY crates ./crates\n"
                      "COPY scripts/build-service-container.py ./scripts/\n"
                      "FROM source AS test\n")
        for name in (".dockerignore", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "crates/core/src/lib.rs", "scripts/build-service-container.py"):
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fictional public build input", encoding="utf-8")
        (root / "Dockerfile").write_text(dockerfile, encoding="utf-8")

    def test_actual_copied_bytes_and_recipe_change_the_seal_while_uncopied_state_does_not(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            original = BUILD.container_source_identity(root)
            (root / "private-state.txt").write_bytes(b"not a COPY input")
            self.assertEqual(BUILD.container_source_identity(root), original)
            path = root / "crates/core/src/lib.rs"
            previous = path.read_bytes()
            path.write_bytes(b"changed actual compiler input")
            changed = BUILD.container_source_identity(root)
            self.assertNotEqual(changed["sha256"], original["sha256"])
            self.assertEqual(changed["files"], original["files"])
            path.write_bytes(previous)
            self.assertEqual(BUILD.container_source_identity(root), original)
            with (root / "Dockerfile").open("a") as output:
                output.write("# changed actual recipe\n")
            self.assertNotEqual(BUILD.container_source_identity(root), original)

    def test_private_state_and_external_hardlink_aliases_are_rejected_before_hashing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            for name in ("crates/.env", "crates/model.key", "crates/temp/private.txt", "crates/artifacts/private.txt"):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"must not enter a public source seal")
                with self.subTest(name=name), self.assertRaisesRegex(ValueError, "Private state"):
                    BUILD.container_source_identity(root)
                path.unlink()
            alias = root / "crates/alias.rs"
            os.link(root / "crates/core/src/lib.rs", alias)
            with self.assertRaisesRegex(ValueError, "Aliased"):
                BUILD.container_source_identity(root)

    def test_unknown_copy_roots_and_missing_required_recipe_inputs_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            original = (root / "Dockerfile").read_text()
            (root / "private").mkdir()
            (root / "Dockerfile").write_text(original.replace("FROM source AS test", "COPY private ./private\nFROM source AS test"))
            with self.assertRaisesRegex(ValueError, "public build inputs"):
                BUILD.container_source_identity(root)
            (root / "Dockerfile").write_text(original.replace("Dockerfile .dockerignore ", ""))
            with self.assertRaisesRegex(ValueError, "incomplete"):
                BUILD.container_source_identity(root)


if __name__ == "__main__":
    unittest.main()
