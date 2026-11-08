# SPDX-License-Identifier: Apache-2.0
"""Pure pinned consumer closure regressions. No compiler, Cargo or network."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), Path(__file__).parents[1] / name)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


CORE = load("provision-static-onnx-runtime.py")
LINK = load("static-onnx-link-closure.py")


class StaticLinkClosureTests(unittest.TestCase):
    def test_failed_runner_recovery_pins_exact_artifact_and_never_claims_command_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / LINK.RECOVERED_LIBRARY["path"]
            archive.parent.mkdir(parents=True)
            archive.write_bytes(b"!<arch>\n" + b"genuine-test-archive" * 8)
            library = {"path": LINK.RECOVERED_LIBRARY["path"], "bytes": archive.stat().st_size, "sha256": CORE.digest(archive)}
            old = [{"path": f"original-{i}", "bytes": 80, "sha256": "a" * 64} for i in range(79)]
            failed = {"result": "failed", "error": "[Errno 1] Operation not permitted", "original_libraries": old, "argv": ["actual-reviewed-command"], "cmake": {"path": "/reviewed/cmake"}}
            CORE.write_json(root / "link-supplement-attempt.json", failed)
            (root / "logs").mkdir()
            (root / "logs/link-supplement.log").write_text("actual failed-owner evidence")
            CORE.write_json(root / "build-receipt.json", {"original": True})
            evidence = {path: CORE.digest(root / path) for path in LINK.NEO_RECOVERY}
            with patch.object(LINK, "NEO_RECOVERY", evidence), patch.object(LINK, "RECOVERED_LIBRARY", library):
                self.assertEqual(LINK.pinned_failure(root, "aarch64-apple-darwin", CORE), failed)
                receipt = {"schema": 1, "target": "aarch64-apple-darwin", "result": "archive_recovered_after_owned_runner_failure",
                           "evidence": evidence, "library": library, "original_libraries": old, "argv": failed["argv"], "cmake": failed["cmake"],
                           "command_exit_unknown": True, "closure": {"real": True}, "process_absence": {"kind": "native_ps_owned_root_commandline_absence", "coverage": "command_line_root_match_only", "descendants_settlement_qualified": False, "matched_processes": [], "observation_only": True},
                           "qualification": {"fresh_full_build": False, "command_pass": False, "final_static_link": False, "actual_model": False}}
                with patch.object(LINK, "closure", return_value={"real": True}), patch.object(CORE, "verify_file"):
                    LINK.validate_recovery(root, "aarch64-apple-darwin", {}, receipt, CORE)
                    for change in ({"command_exit_unknown": False}, {"command": {"exit_code": 0}},
                                   {"qualification": dict(receipt["qualification"], command_pass=True)}, {"original_libraries": old[:-1]}):
                        with self.assertRaises(ValueError):
                            LINK.validate_recovery(root, "aarch64-apple-darwin", {}, {**receipt, **change}, CORE)
                archive.write_bytes(b"changed")
                with self.assertRaises(ValueError):
                    LINK.pinned_failure(root, "aarch64-apple-darwin", CORE)
                with self.assertRaises(ValueError):
                    LINK.pinned_failure(root, "x86_64-pc-windows-msvc", CORE)

    def fixture(self, root, windows):
        target = "x86_64-pc-windows-msvc" if windows else "aarch64-apple-darwin"
        result = {"environment": {"ORT_LIB_LOCATION": str(root / "b")}, "receipt": {"libraries": []}}
        protobuf = root / "b/Release/_deps/protobuf-build" / ("libprotobuf-lite.lib" if windows else "libprotobuf-lite.a")
        protobuf.parent.mkdir(parents=True)
        protobuf.write_bytes(b"!<arch>\n" + b"x" * 80)
        for _name, path in LINK.requested_paths(target, result):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"!<arch>\n" + b"x" * 80)
        return target, result

    def test_all_native_profiles_require_real_re2_at_consumer_search_path(self):
        for windows in (False, True):
            with self.subTest(windows=windows), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                target, result = self.fixture(root, windows)
                evidence = LINK.closure(root, target, result, CORE)
                self.assertGreater(len(evidence["archives"]), 40)
                self.assertFalse(evidence["final_symbol_link_qualified"])
                re2 = next(row for row in evidence["archives"] if row["name"] == "re2")
                (root / re2["path"]).unlink()
                # A genuine archive in a different directory cannot satisfy the
                # actual ort-sys search layout.
                (root / ("re2.lib" if windows else "libre2.a")).write_bytes(b"!<arch>\n" + b"x" * 80)
                with self.assertRaises((ValueError, FileNotFoundError)):
                    LINK.closure(root, target, result, CORE)

    def test_empty_stub_dynamic_binary_and_linked_archive_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target, result = self.fixture(root, False)
            path = next(path for name, path in LINK.requested_paths(target, result) if name == "re2")
            for content in (b"!<arch>\n", b"ELF" + b"x" * 80):
                path.write_bytes(content)
                with self.assertRaises(ValueError):
                    LINK.closure(root, target, result, CORE)

    def test_supplement_excludes_only_exact_extra_and_preserves_original_roster(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            CORE.write_json(root / "build-receipt.json", {"original": True})
            old = {"path": "b/Release/original.lib", "bytes": 80, "sha256": "a" * 64}
            extra = {"path": "b/Release/_deps/re2-build/re2.lib", "bytes": 90, "sha256": "b" * 64}
            receipt = {"schema": 1, "target": "x86_64-pc-windows-msvc", "result": "supplemental_re2_archive_built_not_final_link_qualified",
                       "original_build_receipt_sha256": CORE.digest(root / "build-receipt.json"), "original_libraries": [old], "library": extra}
            CORE.write_json(root / LINK.SUPPLEMENT, receipt)
            self.assertEqual(LINK.original_inventory(root, receipt["target"], "b/Release", [old, extra], CORE), [old])
            for rows in ([extra], [old], [dict(old, sha256="c" * 64), extra], [old, extra, dict(extra, path="other.lib")]):
                with self.assertRaises(ValueError):
                    LINK.original_inventory(root, receipt["target"], "b/Release", rows, CORE)
            (root / "build-receipt.json").write_text("changed")
            with self.assertRaises(ValueError):
                LINK.original_inventory(root, receipt["target"], "b/Release", [old, extra], CORE)

    def test_required_roster_covers_all_pinned_abseil_folders(self):
        with tempfile.TemporaryDirectory() as directory:
            target, result = self.fixture(Path(directory), True)
            paths = dict(LINK.requested_paths(target, result))
            self.assertIn("absl_demangle_rust", paths)
            self.assertIn("absl_decode_rust_punycode", paths)
            self.assertIn("absl_log_internal_message", paths)
            self.assertIn("absl_int128", paths)
            self.assertEqual(len(paths), len(LINK.requested_paths(target, result)))


if __name__ == "__main__":
    unittest.main()
