# SPDX-License-Identifier: Apache-2.0
import hashlib
from pathlib import Path
import tempfile
import unittest

from split_model_assets import split_asset


class LogicalPartTests(unittest.TestCase):
    def test_ordered_parts_reconstruct_exact_approved_bytes_and_detect_wrong_digest(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);raw=bytes(range(53));(root/'weights.data').write_bytes(raw)
            asset={'name':'weights.data','bytes':53,'sha256':hashlib.sha256(raw).hexdigest()}
            report=split_asset(root,asset,root/'good','model',part_size=17)
            actual=b''.join((root/'good'/part['file']).read_bytes() for part in report['parts'])
            self.assertEqual(actual,raw);self.assertEqual([part['bytes'] for part in report['parts']],[17,17,17,2])
            for part in report['parts']:self.assertEqual(hashlib.sha256((root/'good'/part['file']).read_bytes()).hexdigest(),part['sha256'])
            with self.assertRaisesRegex(ValueError,'authenticated asset'):
                split_asset(root,{**asset,'sha256':'0'*64},root/'bad','model',part_size=17)
