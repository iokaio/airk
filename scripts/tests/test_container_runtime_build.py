# SPDX-License-Identifier: Apache-2.0
"""Container provenance seals real COPY inputs without inventing Git provenance."""
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("container_runtime_build", Path(__file__).parents[1] / "build-service-container.py")
BUILD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILD)


class ContainerSourceSealTests(unittest.TestCase):
    def fixture(self, root):
        dockerfile = ("FROM toolchain AS source\n"
                      "COPY Dockerfile .dockerignore Cargo.toml Cargo.lock rust-toolchain.toml ./\n"
                      "COPY crates ./crates\n"
                      "COPY scripts/build-service-container.py ./scripts/\n"
                      "COPY scripts/container-inputs.json ./scripts/\n"
                      "FROM source AS test\n")
        for name in (".dockerignore", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "crates/core/src/lib.rs", "scripts/build-service-container.py"):
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fictional public build input", encoding="utf-8")
        (root / "Dockerfile").write_text(dockerfile, encoding="utf-8")
        (root / "scripts/container-inputs.json").write_text(json.dumps({"schema": 1, "trees": ["crates"],
            "files": ["Dockerfile", ".dockerignore", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
                      "scripts/build-service-container.py", "scripts/container-inputs.json",
                      "packages/vscode/src/test/fixtures/httpAuthorityDigest.json"]}), encoding="utf-8")

    def test_actual_multistage_repository_recipe_has_a_bounded_complete_source_seal(self):
        root = Path(__file__).parents[2]
        result = BUILD.container_source_identity(root)
        self.assertGreater(result["files"], 100)
        self.assertEqual(len(result["sha256"]), 64)

    def test_manifest_cannot_widen_tree_ownership_or_admit_directory_as_exact_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            manifest = root / "scripts/container-inputs.json"
            original = json.loads(manifest.read_text())
            for tree in ("packages", "scripts", "tools", "docs", "third_party"):
                changed = dict(original, trees=original["trees"] + [tree])
                manifest.write_text(json.dumps(changed))
                with self.subTest(tree=tree), self.assertRaisesRegex(ValueError, "reviewed schema"):
                    BUILD.container_source_identity(root)
            original["files"].append("scripts")
            manifest.write_text(json.dumps(original))
            recipe = (root / "Dockerfile").read_text()
            (root / "Dockerfile").write_text(recipe.replace("FROM source AS test", "COPY scripts ./scripts\nFROM source AS test"))
            with self.assertRaisesRegex(ValueError, "exact files"):
                BUILD.container_source_identity(root)

    def test_manifest_is_sealed_and_unsafe_exact_paths_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            before = BUILD.container_source_identity(root)
            manifest = root / "scripts/container-inputs.json"
            original = json.loads(manifest.read_text())
            manifest.write_text(json.dumps(original, indent=2))
            self.assertNotEqual(before, BUILD.container_source_identity(root))
            for name in ("../private.txt", "/private.txt", "scripts/*.py", "scripts/../private.txt", "scripts\\other.py"):
                manifest.write_text(json.dumps(dict(original, files=original["files"] + [name])))
                with self.subTest(name=name), self.assertRaisesRegex(ValueError, "exact relative"):
                    BUILD.container_source_identity(root)

    def test_shared_http_authority_fixture_is_required_and_its_exact_bytes_are_sealed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            before = BUILD.container_source_identity(root)
            name = "packages/vscode/src/test/fixtures/httpAuthorityDigest.json"
            path = root / name
            path.parent.mkdir(parents=True)
            path.write_bytes(b'{"fixture":"original authority digest"}')
            recipe = (root / "Dockerfile").read_text()
            (root / "Dockerfile").write_text(recipe.replace(
                "FROM source AS test", f"COPY {name} ./{name}\nFROM source AS test"))
            original = BUILD.container_source_identity(root)
            self.assertEqual(original["files"], before["files"] + 1)
            path.write_bytes(b'{"fixture":"changed authority digest"}')
            self.assertNotEqual(BUILD.container_source_identity(root)["sha256"], original["sha256"])
            path.unlink()
            with self.assertRaisesRegex(ValueError, "Missing or redirected"):
                BUILD.container_source_identity(root)

    def test_shared_fixture_exception_does_not_admit_packages_or_neighboring_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            fixture = root / "packages/vscode/src/test/fixtures/httpAuthorityDigest.json"
            fixture.parent.mkdir(parents=True)
            fixture.write_text("public shared fixture")
            neighbor = fixture.with_name("neighbor.json")
            neighbor.write_text("not a public container input")
            recipe = (root / "Dockerfile").read_text()
            for selected in ("packages", "packages/vscode/src/test/fixtures", neighbor.relative_to(root).as_posix()):
                (root / "Dockerfile").write_text(recipe.replace(
                    "FROM source AS test", f"COPY {selected} ./packages/\nFROM source AS test"))
                with self.subTest(selected=selected), self.assertRaisesRegex(ValueError, "public build inputs"):
                    BUILD.container_source_identity(root)

    def test_shared_fixture_rejects_a_redirected_parent_before_reading_its_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            actual = root / "outside-public-copy"
            actual.mkdir()
            (actual / "httpAuthorityDigest.json").write_text("must not be read through an alias")
            parent = root / "packages/vscode/src/test/fixtures"
            parent.parent.mkdir(parents=True)
            try:
                parent.symlink_to(actual, target_is_directory=True)
            except OSError as error:
                if os.name == "nt" and error.winerror == 1314:
                    self.skipTest("Windows contributor identity cannot create symbolic links")
                raise
            name = "packages/vscode/src/test/fixtures/httpAuthorityDigest.json"
            recipe = (root / "Dockerfile").read_text()
            (root / "Dockerfile").write_text(recipe.replace(
                "FROM source AS test", f"COPY {name} ./{name}\nFROM source AS test"))
            with self.assertRaisesRegex(ValueError, "Redirected"):
                BUILD.container_source_identity(root)

    def test_continued_run_shell_and_copy_are_separate_logical_instructions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            original = BUILD.container_source_identity(root)
            recipe = (root / "Dockerfile").read_text()
            recipe = ("FROM toolchain AS prerequisite\n"
                      "RUN printf '%s' \\\n  'quoted shell text' \\\n# FROM unrelated AS source\n"
                      "  && printf '%s' '# COPY secret ./secret'\n" + recipe)
            recipe = recipe.replace("COPY Dockerfile .dockerignore Cargo.toml Cargo.lock rust-toolchain.toml ./",
                                    "COPY Dockerfile \\\n# A comment inside a continued COPY is not an input.\n"
                                    "  .dockerignore Cargo.toml \\\n  Cargo.lock rust-toolchain.toml ./")
            recipe = recipe.replace("FROM toolchain AS source", "from toolchain as source")
            (root / "Dockerfile").write_text(recipe)
            observed = BUILD.container_source_identity(root)
            self.assertEqual(observed["files"], original["files"])
            self.assertNotEqual(observed["sha256"], original["sha256"])
            (root / "Cargo.lock").write_text("changed continued input")
            self.assertNotEqual(BUILD.container_source_identity(root), observed)

    def test_only_exact_static_stage_copy_is_excluded_and_unknown_forms_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            original = (root / "Dockerfile").read_text()
            known = "COPY --from=static-inference /build/static-onnx-runtime /build/static-onnx-runtime\n"
            (root / "Dockerfile").write_text(original.replace("COPY crates", known + "COPY crates"))
            BUILD.container_source_identity(root)
            unsupported = [known.replace("static-inference", "other-stage", 1),
                           "COPY --chown=1000 crates ./crates\n", 'COPY ["crates", "./crates"]\n',
                           "COPY crates/$SELECTED ./crates\n", "COPY crates/*.rs ./crates\n",
                           "ADD crates ./crates\n", "COPY crates ./crates # not-a-Docker-comment\n"]
            for instruction in unsupported:
                (root / "Dockerfile").write_text(original.replace("COPY crates ./crates\n", instruction))
                with self.subTest(instruction=instruction), self.assertRaises(ValueError):
                    BUILD.container_source_identity(root)

    def test_recipe_bounds_unfinished_continuations_and_unreviewed_parsing_are_explicit(self):
        for recipe in ("# escape=`\nFROM toolchain AS source\n", "RUN <<EOF\nFROM fake AS source\nEOF\n",
                       "RUN echo unfinished \\", "RUN " + "x" * (64 * 1024),
                       "#" + "x" * (512 * 1024), "\n" * 8193):
            with self.subTest(prefix=recipe[:40]), self.assertRaises(ValueError):
                list(BUILD.docker_instructions(recipe))

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

    def test_pair_versions_are_refused_before_any_build_or_destination(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text('[workspace.package]\nversion="1.8.13"\n', encoding="utf-8")
            destination = root / "container"
            with (mock.patch.object(BUILD, "ROOT", root),
                  mock.patch.object(BUILD.platform, "system", return_value="Linux"),
                  mock.patch.object(BUILD.builder, "cargo_binary", side_effect=AssertionError("no Cargo build")),
                  mock.patch.object(sys, "argv", ["build-service-container.py", "--destination", str(destination)])):
                with self.assertRaisesRegex(SystemExit, "historical runtime pair"):
                    BUILD.main()
            self.assertFalse(destination.exists())


if __name__ == "__main__":
    unittest.main()
