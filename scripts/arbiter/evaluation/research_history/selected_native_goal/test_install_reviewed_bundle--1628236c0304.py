# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
import install_reviewed_bundle as owner

class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.target = self.root / "crates/airk-service/src/service_runtime/arbiter.rs"
        self.target.parent.mkdir(parents=True)
        self.target.write_bytes(b"old native source")
        self.proposal = self.root / "artifacts/advanced-split/private/arbiter.proposed"
        self.proposal.parent.mkdir(parents=True)
        self.proposal.write_bytes(b"reviewed native source")
        self.row = {"target": str(self.target.relative_to(self.root)), "base_sha256": owner.digest(self.target.read_bytes()), "proposed": str(self.proposal.relative_to(self.root)), "sha256": owner.digest(self.proposal.read_bytes())}
        self.manifest = self.proposal.parent / "manifest.json"
    def pin(self, rows=None):
        self.manifest.write_text(json.dumps({"files": rows or [self.row]}))
        return owner.digest(self.manifest.read_bytes())
    def test_verify_has_no_target_write(self):
        rows = owner.prepare(self.root, self.manifest, self.pin())
        self.assertEqual(len(rows), 1)
        self.assertEqual(self.target.read_bytes(), b"old native source")
    def test_live_drift_refuses_before_backup_or_write(self):
        pin = self.pin()
        self.target.write_bytes(b"unrelated user change")
        backup = self.root / "artifacts/advanced-split/mount"
        with self.assertRaisesRegex(ValueError, "live source drift"):
            owner.mount(self.root, self.manifest, pin, backup)
        self.assertFalse(backup.exists())
        self.assertEqual(self.target.read_bytes(), b"unrelated user change")
    def test_duplicate_target_refuses(self):
        pin = self.pin([self.row, self.row])
        with self.assertRaisesRegex(ValueError, "duplicate"):
            owner.prepare(self.root, self.manifest, pin)
    def test_changed_proposal_refuses(self):
        pin = self.pin()
        self.proposal.write_bytes(b"unreviewed source")
        with self.assertRaisesRegex(ValueError, "proposed source drift"):
            owner.prepare(self.root, self.manifest, pin)
    def test_containment_refuses(self):
        self.row["proposed"] = "../outside"
        pin = self.pin()
        with self.assertRaisesRegex(ValueError, "relative contained"):
            owner.prepare(self.root, self.manifest, pin)
    def test_reviewed_mount_retains_exact_backup_and_receipt(self):
        pin = self.pin()
        backup = self.root / "artifacts/advanced-split/mount"
        result = owner.mount(self.root, self.manifest, pin, backup)
        self.assertTrue(result["complete"])
        self.assertEqual(self.target.read_bytes(), b"reviewed native source")
        self.assertEqual((backup / self.row["target"]).read_bytes(), b"old native source")
        self.assertEqual(json.loads((backup / "mount-receipt.json").read_text()), result)

if __name__ == "__main__":
    unittest.main()
