# SPDX-License-Identifier: Apache-2.0
import json
from pathlib import Path
import tempfile
import unittest
from prepare_fixture import cargo_image, pin, ROOT
from run_fixture import closure_matches, same_control_path
import os

class PreparationTests(unittest.TestCase):
    def test_actual_test_selector_is_unique(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'cargo.json'
            row = {'reason': 'compiler-artifact', 'target': {'name': 'airk_service'},
                   'profile': {'test': True}, 'executable': 'actual.exe'}
            path.write_text('diagnostic\n' + json.dumps(row) + '\n', encoding='utf-8')
            self.assertEqual(cargo_image(path), 'actual.exe')
            row['executable'] = 'different.exe'
            with path.open('a', encoding='utf-8') as out:
                out.write(json.dumps(row) + '\n')
            with self.assertRaises(ValueError):
                cargo_image(path)

    def test_non_test_image_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'cargo.json'
            path.write_text('{}\n', encoding='utf-8')
            with self.assertRaises(ValueError):
                cargo_image(path)

    def test_pin_detects_equal_length_drift(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'input'
            path.write_bytes(b'first')
            before = pin(path)
            path.write_bytes(b'other')
            self.assertNotEqual(before, pin(path))

    def test_workspace_root(self):
        self.assertTrue((ROOT / 'Cargo.toml').is_file())

    def test_only_same_instance_native_closure_counts(self):
        work = str(ROOT / 'artifacts' / 'not-created')
        control = str(Path(work) / 'control')
        ready = {'schema': 'airk.private-arbiter-fixture/1', 'control_root': control,
                 'inputs_sha256': 'input', 'health': {'instance': 'actual-one', 'executable_sha256': 'image'}}
        closed = {'schema': 'airk.private-arbiter-fixture-closed/1', 'control_root': control,
                  'instance': 'actual-one', 'shutdown_completed': True, 'service_lease_reacquired': True}
        self.assertTrue(closure_matches(ready, closed, work, 'input', 'image'))
        closed['instance'] = 'other'
        self.assertFalse(closure_matches(ready, closed, work, 'input', 'image'))

    def test_missing_ready_cannot_claim_closure(self):
        self.assertFalse(closure_matches(None, {'shutdown_completed': True}, 'work', 'input', 'image'))

    def test_native_extended_local_path_matches_actual_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            control = Path(directory) / 'control'
            control.mkdir()
            extended = '\\\\?\\' + str(control.resolve()) if os.name == 'nt' else str(control.resolve())
            self.assertTrue(same_control_path(extended, str(control)))
            self.assertFalse(same_control_path(str(Path(directory) / 'absent'), str(control)))

if __name__ == '__main__':
    unittest.main()
