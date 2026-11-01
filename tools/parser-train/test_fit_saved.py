# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('fit_saved', Path(__file__).with_name('fit-saved.py'))
fit_saved = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit_saved)


class SavedFitContracts(unittest.TestCase):
    def test_recorded_bytes_and_rows_are_required_for_replay(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'examples.jsonl'
            raw = (json.dumps({'text': 'Find the struct PacketStore.', 'intent': 'find_definition'}) + '\n').encode()
            path.write_bytes(raw)
            record = {'sha256': hashlib.sha256(raw).hexdigest(), 'rows': 1}
            rows, observed = fit_saved.load_examples(path, record)
            self.assertEqual(len(rows), 1)
            self.assertEqual(observed['sha256'], record['sha256'])
            with self.assertRaisesRegex(ValueError, 'exact recorded bytes or row count'):
                fit_saved.load_examples(path, {**record, 'rows': 2})
            path.write_bytes(raw + b'\n')
            with self.assertRaisesRegex((ValueError, json.JSONDecodeError), '.*'):
                fit_saved.load_examples(path, record)

    def test_custom_examples_require_finite_labels_and_bounded_text(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'custom.jsonl'
            for row in ({'text': 'run tests', 'intent': 'shell'},
                        {'text': '', 'intent': 'search'},
                        {'text': 'x' * 4097, 'intent': 'search'}):
                path.write_text(json.dumps(row) + '\n', encoding='utf-8')
                with self.assertRaisesRegex(ValueError, 'bounded text'):
                    fit_saved.load_examples(path)


if __name__ == '__main__':
    unittest.main()
