# SPDX-License-Identifier: Apache-2.0
"""Synthetic wrapper boundary guards. Never opens a FINAL input or starts a job."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('reviewed_prepare_wrapper', HERE / 'run-prepare-reviewed.py')
wrapper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wrapper)


class WrapperGuards(unittest.TestCase):
    def test_exact_retained_job_helper_executes_without_path_reopen(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'helper.py'
            marker = Path(directory) / 'FORBIDDEN'
            raw = b'VALUE=17\n'
            path.write_bytes(raw)
            retained = path.read_bytes()
            path.write_text('from pathlib import Path\nPath(' + repr(str(marker)) + ').write_text("SECRET")\n')
            module = wrapper.import_retained('verified_helper', path, retained)
            self.assertEqual(module.VALUE, 17)
            self.assertFalse(marker.exists())

    def test_actual_owner_strict_normal_close_rejects_timeout_force_and_refusal(self):
        path = HERE.parents[3] / 'scripts/provision-static-onnx-runtime.py'
        owner = wrapper.import_retained('actual_owned_job', path, path.read_bytes())
        clean = {'exit_code': 0, 'timed_out': False, 'log_oversized': False,
                 'owned_descendants_settled': True, 'owned_job_forced_termination': False}
        self.assertTrue(wrapper.strict_normal_close(owner, clean))
        for patch in ({'exit_code': 1}, {'timed_out': True}, {'log_oversized': True},
                      {'owned_descendants_settled': False}, {'owned_job_forced_termination': True},
                      {'cleanup_refusal': 'SECRET'}):
            self.assertFalse(wrapper.strict_normal_close(owner, {**clean, **patch}))
        self.assertFalse(wrapper.strict_normal_close(owner, {k: v for k, v in clean.items() if k != 'owned_job_forced_termination'}))

    def test_capture_binds_independent_literal_digest_and_detects_replaced_inode(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'input'
            path.write_bytes(b'abc')
            before = wrapper.capture(path)
            self.assertEqual(before[0], 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad')
            self.assertEqual(before[-1], 3)
            replacement = Path(directory) / 'replacement'
            replacement.write_bytes(b'abc')
            replacement.replace(path)
            after = wrapper.capture(path)
            self.assertEqual(before[0], after[0])
            self.assertNotEqual(before[1:3], after[1:3])

    def test_hardlinked_input_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            source, alias = Path(directory) / 'source', Path(directory) / 'alias'
            source.write_bytes(b'abc')
            os.link(source, alias)
            with self.assertRaises(SystemExit):
                wrapper.capture(alias)


if __name__ == '__main__':
    unittest.main()
