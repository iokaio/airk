# SPDX-License-Identifier: Apache-2.0
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch
from run_capture import require_absent

class AbsenceTests(unittest.TestCase):
    def test_actual_absence_passes(self):
        with tempfile.TemporaryDirectory() as directory:
            require_absent(Path(directory) / 'missing')

    def test_actual_present_refuses(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'present'; path.write_bytes(b'present')
            with self.assertRaises(ValueError):
                require_absent(path)

    def test_dangling_symlink_refuses(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'dangling'
            try:
                path.symlink_to(Path(directory) / 'nonexistent-target')
            except OSError as error:
                self.skipTest('native symlink creation unavailable: ' + str(error))
            self.assertFalse(path.exists())
            with self.assertRaises(ValueError):
                require_absent(path)

    def test_dangling_alias_guard_without_creation_privilege(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'dangling'
            with patch('run_capture.os.path.lexists', return_value=False), patch('run_capture.Path.is_symlink', return_value=True):
                with self.assertRaises(ValueError):
                    require_absent(path)

if __name__ == '__main__':
    unittest.main()
