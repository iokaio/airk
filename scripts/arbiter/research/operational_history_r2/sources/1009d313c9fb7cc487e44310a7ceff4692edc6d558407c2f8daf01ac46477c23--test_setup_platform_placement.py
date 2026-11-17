# SPDX-License-Identifier: Apache-2.0
"""Static platform placement only; no native service/filesystem effects."""
from pathlib import Path
import re
import unittest
ROOT=Path(__file__).resolve().parents[2]/"crates/airk-service/src/setup_role"

class SetupPlatformPlacement(unittest.TestCase):
    def test_windows_removal_snapshot_matches_its_only_directory_cleanup_consumer(self):
        source=(ROOT/"mod.rs").read_text(encoding="utf-8")
        self.assertRegex(source,r"#\[cfg\(windows\)\]\s+let order = entries")
        self.assertRegex(source,r"#\[cfg\(windows\)\]\s+for folder in plan::program_folders\(&order\)")
        self.assertIn("for (relative, digest, bytes) in entries {",source)
        self.assertIn("effects::program_files::remove(&program_pin, &relative, digest, bytes)",source)
        self.assertIn(".remove_program_root(&program)",source)

    def test_only_inno_completion_helper_is_windows_gated_and_shared_recovery_survives(self):
        transaction=(ROOT/"transaction.rs").read_text(encoding="utf-8")
        self.assertRegex(transaction,r"#\[cfg\(windows\)\]\s+pub\(super\) fn completed_for\(")
        inno=(ROOT/"inno.rs").read_text(encoding="utf-8")
        self.assertIn("transaction::completed_for(&pointer, &operation)?",inno)
        seal=inno[inno.index("pub(super) fn seal("):inno.index("pub(super) fn seal_published(")]
        self.assertIn("#[cfg(windows)]",seal)
        self.assertIn("#[cfg(not(windows))]",seal)
        # Independent common-platform duties must not disappear with the
        # Windows sealer helper. Their immediately preceding line is not a cfg.
        for name in ("admission_time","startup_allowed"):
            declaration=re.search(r"(?m)^pub\(super\) fn "+name+r"\(",transaction)
            self.assertIsNotNone(declaration,name)
            previous=transaction[:declaration.start()].rstrip().splitlines()[-1]
            self.assertNotIn("#[cfg",previous,name)
        self.assertIn("pub fn observe(",transaction)
        self.assertIn("pub fn reopen_failed(",transaction)
        self.assertIn("fn completed(setup: &Setup)",(ROOT/"recovery.rs").read_text(encoding="utf-8"))

if __name__=="__main__":unittest.main()
