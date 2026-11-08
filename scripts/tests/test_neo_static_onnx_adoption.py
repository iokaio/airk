# SPDX-License-Identifier: Apache-2.0
"""Security boundaries for importing the one completed Neo static build."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).resolve().parents[1] / filename)
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


CORE = module("neo_test_core", "provision-static-onnx-runtime.py")
ADOPT = module("neo_test_adoption", "adopt-neo-static-onnx-runtime.py")


class NeoAdoptionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()

    def test_fixed_root_and_native_architecture_are_required(self):
        with patch.object(ADOPT.platform, "system", return_value="Darwin"), patch.object(ADOPT.platform, "machine", return_value="arm64"):
            with self.assertRaisesRegex(ValueError, "moved"):
                ADOPT.fixed_root(self.root, ADOPT.TARGET, CORE)
        with patch.object(ADOPT.platform, "system", return_value="Darwin"), patch.object(ADOPT.platform, "machine", return_value="x86_64"):
            with self.assertRaisesRegex(ValueError, "native"):
                ADOPT.fixed_root(Path(ADOPT.ROOT), ADOPT.TARGET, CORE)

    def source_fixture(self):
        source = self.root / "onnxruntime-source"
        source.mkdir()
        (source / "model.cc").write_bytes(b"fictional source\n")
        for name in (".git", "mirror"):
            (source / name).mkdir()
            (source / name / "excluded").write_bytes(b"separate evidence\n")
        rows = ADOPT.source_inventory(source, CORE)
        seal = hashlib.sha256((json.dumps(rows, indent=2, sort_keys=True) + "\n").encode()).hexdigest()
        return source, {"source": {"files": 1, "inventory_sha256": seal}}

    def test_complete_source_bytes_and_extra_files_are_checked(self):
        source, lock = self.source_fixture()
        with patch.object(CORE, "LOCK", lock):
            self.assertEqual(ADOPT.verify_source(self.root, CORE), lock["source"]["inventory_sha256"])
            (source / "unexpected.cc").write_bytes(b"extra\n")
            with self.assertRaisesRegex(ValueError, "complete sealed"):
                ADOPT.verify_source(self.root, CORE)

    def test_source_does_not_normalize_crlf_or_trust_git_status(self):
        source, lock = self.source_fixture()
        (source / "model.cc").write_bytes(b"fictional source\r\n")
        with patch.object(CORE, "LOCK", lock):
            with self.assertRaisesRegex(ValueError, "complete sealed"):
                ADOPT.verify_source(self.root, CORE)

    def test_source_rejects_hardlink_alias(self):
        source, _lock = self.source_fixture()
        (source / "alias.cc").hardlink_to(source / "model.cc")
        with self.assertRaisesRegex(ValueError, "unaliased"):
            ADOPT.source_inventory(source, CORE)

    def inspection_fixture(self):
        original = {"libraries": [{"path": "libfixture" + str(index) + ".a"} for index in range(79)]}
        commands = [{"argv": ["/usr/bin/lipo", "-archs", ADOPT.ROOT + "/b2/Release/" + row["path"]],
                     "exit_code": 0, "stdout": "arm64\n"} for row in original["libraries"]]
        commands.append({"argv": ["/usr/bin/otool", "-l", ADOPT.ROOT + "/postbuild-evidence/session-first-object.o"],
                         "exit_code": 0, "stdout": "minos 13.3\n sdk 26.5\n"})
        inspection = {"producer_receipt_sha256": ADOPT.EVIDENCE[ADOPT.PRODUCER], "result": "archive_metadata_verified",
                      "library_count": 79, "all_archives_arm64": True, "libraries_hash_match": True,
                      "all_original_library_hashes_still_match": True, "commands": commands,
                      "representative_object": {"minimum_os": "13.3", "archive": "libonnxruntime_session.a", "member": "IOBinding.cc.o"}}
        return original, inspection

    def test_inspection_requires_every_archive_at_its_original_path(self):
        original, inspection = self.inspection_fixture()
        ADOPT.validate_inspection(inspection, original)
        inspection["commands"][0]["argv"][2] = "/another-root/libfixture0.a"
        with self.assertRaisesRegex(ValueError, "coverage"):
            ADOPT.validate_inspection(inspection, original)

    def test_arm64_boolean_does_not_hide_a_wrong_architecture(self):
        original, inspection = self.inspection_fixture()
        inspection["commands"][0]["stdout"] = "x86_64 arm64\n"
        with self.assertRaisesRegex(ValueError, "architecture"):
            ADOPT.validate_inspection(inspection, original)

    def test_sdk_and_deployment_minimum_remain_distinct(self):
        original, inspection = self.inspection_fixture()
        inspection["commands"][-1]["stdout"] = "minos 13.3\n sdk 13.3\n"
        with self.assertRaisesRegex(ValueError, "actual SDK"):
            ADOPT.validate_inspection(inspection, original)

    def test_retained_apple_compiler_metadata_requires_exact_full_build_version(self):
        metadata = ('set(CMAKE_CXX_COMPILER_ID "AppleClang")\n'
                    'set(CMAKE_CXX_COMPILER_VERSION "21.0.0.21000101")\n')
        ADOPT.validate_compiler_metadata(metadata)
        for wrong in ("21.0.0", "21.0.0.21000102", "22.0.0.21000101"):
            with self.subTest(version=wrong), self.assertRaisesRegex(ValueError, "C\\+\\+ metadata"):
                ADOPT.validate_compiler_metadata(metadata.replace("21.0.0.21000101", wrong))

    def test_changed_producer_fails_before_source_or_notices_are_touched(self):
        path = self.root / ADOPT.PRODUCER
        path.parent.mkdir()
        path.write_bytes(b'{"counterfeit":true}\n')
        with patch.object(ADOPT, "fixed_root", return_value=self.root), patch.object(ADOPT, "verify_source") as source, patch.object(CORE, "notice_items") as notices:
            with self.assertRaisesRegex(ValueError, "original evidence"):
                ADOPT.inputs(self.root, ADOPT.TARGET, path, CORE)
            source.assert_not_called()
            notices.assert_not_called()
        self.assertFalse((self.root / "owner.json").exists())

    def test_adoption_preserves_failed_and_corrected_records_without_building(self):
        observed = {"original_build": {"elapsed_seconds": 654.312, "result": "original incomplete qualification"},
                    "original_failed_attempt": {"error": "missing Python", "elapsed_seconds": 51.333},
                    "libraries": [{"path": "b2/Release/libfictional.a", "sha256": "original"}]}
        with patch.object(ADOPT, "fixed_root", return_value=self.root), patch.object(ADOPT, "inputs", return_value=observed), patch.object(CORE, "collect_notices", return_value=[]), patch.object(CORE, "run_owned") as runner:
            result = ADOPT.adopt(self.root, ADOPT.TARGET, self.root / ADOPT.PRODUCER, CORE)
            self.assertEqual(result["receipt"]["original_build"], observed["original_build"])
            self.assertEqual(result["receipt"]["original_failed_attempt"], observed["original_failed_attempt"])
            self.assertEqual(result["receipt"]["qualification"], ADOPT.QUALIFICATION)
            self.assertEqual(result["environment"]["ORT_LIB_LOCATION"], str(self.root / "b2"))
            self.assertEqual(len(result["environment"]), 5)
            runner.assert_not_called()
            self.assertEqual(ADOPT.verify(self.root, ADOPT.TARGET, result["receipt"], CORE), result)
            # Simulate interruption between the receipt and owner writes. The
            # fully authenticated resume preserves the original adoption time.
            (self.root / "owner.json").unlink()
            resumed = ADOPT.adopt(self.root, ADOPT.TARGET, self.root / ADOPT.PRODUCER, CORE)
            self.assertEqual(resumed, result)
            with self.assertRaisesRegex(ValueError, "original location"):
                ADOPT.adopt(self.root, ADOPT.TARGET, self.root / "foreign.json", CORE)
            result["receipt"]["qualification"]["fresh_build"] = True
            with self.assertRaisesRegex(ValueError, "qualification"):
                ADOPT.verify(self.root, ADOPT.TARGET, result["receipt"], CORE)


if __name__ == "__main__":
    unittest.main()
