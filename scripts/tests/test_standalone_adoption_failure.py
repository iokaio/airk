# SPDX-License-Identifier: Apache-2.0
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("standalone_failure", Path(__file__).parents[1] / "check-standalone-adoption-failure.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class StandaloneAdoptionFailureTests(unittest.TestCase):
    def object_fixture(self, base):
        root = base / "copied"
        payload = b'{"retained":"authentic copied bytes"}'
        path = root / "collections" / "saved" / "objects" / "artifacts" / hashlib.sha256(payload).hexdigest()
        path.parent.mkdir(parents=True)
        path.write_bytes(payload)
        for name in ("index.sqlite3", "generations/revision/manifest.json", "maintenance/status.json"):
            item = root / "collections" / "saved" / name
            item.parent.mkdir(parents=True, exist_ok=True)
            item.write_bytes(b"Preserved exact header/manifest/status bytes")
        return root, path, payload

    def test_only_one_copied_retained_object_changes_without_length_or_metadata_changes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, path, payload = self.object_fixture(Path(temporary))
            original = Path(temporary) / "original"
            original.write_bytes(payload)
            proof = CHECK.corrupt_copied_object(root, "collections", "saved")
            self.assertEqual(original.read_bytes(), payload)
            self.assertEqual(path.stat().st_size, len(payload))
            self.assertNotEqual(path.read_bytes(), payload)
            differences = [name for name in proof["copied_inventory_before"]
                           if proof["copied_inventory_before"][name] != proof["copied_inventory_after"][name]]
            self.assertEqual(differences, [proof["path"]])

    def test_corruption_rejects_already_damaged_object_and_other_layout(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, path, _ = self.object_fixture(Path(temporary))
            before = path.read_bytes()
            with self.assertRaises(ValueError):
                CHECK.corrupt_copied_object(root, "collections-1.2", "saved")
            self.assertEqual(path.read_bytes(), before)
            path.write_bytes(b"already changed")
            with self.assertRaises(AssertionError):
                CHECK.corrupt_copied_object(root, "collections", "saved")

    def test_corruption_refuses_hardlink_to_original(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, path, payload = self.object_fixture(Path(temporary))
            original = Path(temporary) / "original"
            try:
                os.link(path, original)
            except OSError as error:
                self.skipTest(str(error))
            with self.assertRaises(AssertionError):
                CHECK.corrupt_copied_object(root, "collections", "saved")
            self.assertEqual(original.read_bytes(), payload)

    def restoration(self):
        original = {"instance": "original", "runtime_sha256": "payload", "executable_sha256": "image"}
        current = {**original, "instance": "restored", "policy": {"accepting": True}}
        setup = {"registration": {"binding": "same exact native backing"}, "key": {"nonce": "group"}}
        journal = {"phase": "restored", "operation": "batch", "original_running": True,
                   "original_explicit_user_stop": False, "original_health": original,
                   "restoration": {"operation": "batch", "restarted": True, "originally_explicitly_stopped": False,
                                   "service_instance": "restored"}, "restoration_state_sha256": "snapshot",
                   "adoption_plans": [{"setup": setup}]}
        receipt = {"stage": "prepared", "completion": None, "request": setup,
                   "legacy_adoption": "plan", "preparation": {"legacy_adoption": "plan"}}
        return original, current, journal, receipt

    def test_restoration_requires_exact_prepared_plan_new_ready_owner_and_preserved_stop_intent(self):
        original, current, journal, receipt = self.restoration()
        CHECK.validate_restoration("batch", original, current, journal, receipt)
        for subject, key, value in (("receipt", "stage", "ready"), ("receipt", "legacy_adoption", "other"),
                                    ("current", "instance", "original"), ("journal", "operation", "other"),
                                    ("journal", "original_explicit_user_stop", True),
                                    ("journal", "restoration_state_sha256", None), ("journal", "adoption_plans", [])):
            changed = {"current": copy.deepcopy(current), "journal": copy.deepcopy(journal), "receipt": copy.deepcopy(receipt)}
            changed[subject][key] = value
            with self.subTest(subject=subject, key=key), self.assertRaises((AssertionError, IndexError)):
                CHECK.validate_restoration("batch", original, changed["current"], changed["journal"], changed["receipt"])
        current["policy"]["accepting"] = False
        with self.assertRaises(AssertionError):
            CHECK.validate_restoration("batch", original, current, journal, receipt)

    def test_saved_native_snapshot_requires_unchanged_raw_digest_and_bounded_regular_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "native.json"
            raw = b'{"schema":1,"complete":true,"unknown_native_field":"retained"}'
            path.write_bytes(raw)
            digest = CHECK.framed_digest(b"airk/checkpoint-native-state/1", raw)
            self.assertEqual(CHECK.read_verified_snapshot(path, digest)["unknown_native_field"], "retained")
            path.write_bytes(raw + b" ")
            with self.assertRaises(AssertionError):
                CHECK.read_verified_snapshot(path, digest)
            path.write_bytes(b" " * (512 * 1024 + 1))
            with self.assertRaises(AssertionError):
                CHECK.read_verified_snapshot(path, digest)

    def test_actual_not_initialized_publication_and_exact_native_pending_work_are_required(self):
        receipt = {"operation_id": "exact", "request": {"key": {"namespace": "native"}, "registration": {"binding": "adopted"}}}
        raw = json.dumps(receipt).encode()
        before = {"namespace": "native", "jobs": [], "requests": [], "unfinished": [], "retained_intents": [],
                  "retained_intent_contract": 1, "retained_intent_disposition": "ordinary_admission_after_commit", "publications": []}
        after = copy.deepcopy(before)
        after["unfinished"] = [{"owner": "index_operation_receipts", "id": "exact", "kind": "catalog_index_setup",
                                "state": "prepared", "original_engine": None,
                                "payload_digest": CHECK.framed_digest(b"airk/checkpoint-operation/1", raw)}]
        after["publications"] = [{"binding": "adopted", "registered": True, "publication": {"state": "not_initialized"}}]
        CHECK.validate_snapshot(before, after, receipt, raw)
        for key, value in (("jobs", ["unrequested"]), ("unfinished", []),
                           ("publications", [{"binding": "adopted", "publication": {"state": "current"}}])):
            changed = copy.deepcopy(after)
            changed[key] = value
            with self.subTest(key=key), self.assertRaises(AssertionError):
                CHECK.validate_snapshot(before, changed, receipt, raw)


if __name__ == "__main__":
    unittest.main()
