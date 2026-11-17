# SPDX-License-Identifier: Apache-2.0
"""Source-only runbook and diagnostic fragment inverse controls."""
import ast
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
from unittest import mock
import copy
BASE=Path(__file__).resolve().parent
ROOT=BASE.parents[3]
class RunbookIdentity(unittest.TestCase):
    def test_exact_documents_and_independent_fragment_range(self):
        rows=json.loads((BASE/'source-provenance.json').read_bytes())['files'];self.assertEqual(len(rows),3)
        for row in rows:
            raw=(ROOT/row['source']).read_bytes();self.assertEqual((len(raw),hashlib.sha256(raw).hexdigest()),(row['bytes'],row['original_sha256']))
            if row['changes']:
                begin=raw.index(b'```python')+len(b'```python');begin=raw.index(b'\n',begin)+1;end=raw.index(b'```',begin);body=raw[begin:end]
                first,second=row['changes'];self.assertEqual((first['offset'],first['bytes'],first['extracted_sha256']),(begin,len(body),hashlib.sha256(body).hexdigest()))
                self.assertEqual(first['kind'],'byte-range-extraction');self.assertEqual(second['kind'],'spdx-prefix');expected=second['prefix'].encode()+body;ast.parse(expected)
            else:expected=raw
            actual=(ROOT/row['destination']).read_bytes();self.assertEqual(actual,expected);self.assertEqual((len(actual),hashlib.sha256(actual).hexdigest()),(row['maintained_bytes'],row['maintained_sha256']))
    def test_explicit_roster_and_overwrite_refusal(self):
        doc=json.loads((BASE/'source-provenance.json').read_bytes());expected={'README.md','extract.py','test_identity.py','source-provenance.json','windows-qualification-plan.md','native-help-runbook.md','windows-handle-probe.py.in'}
        self.assertEqual({p.name for p in BASE.iterdir()},expected)
        spec=importlib.util.spec_from_file_location('runbook_extract',BASE/'extract.py');tool=importlib.util.module_from_spec(spec);spec.loader.exec_module(tool)
        with self.assertRaises(ValueError):tool.apply(doc)
    def test_duplicate_source_and_wrong_destination_refuse_before_writes(self):
        doc=json.loads((BASE/'source-provenance.json').read_bytes())
        spec=importlib.util.spec_from_file_location('runbook_extract_controls',BASE/'extract.py');tool=importlib.util.module_from_spec(spec);spec.loader.exec_module(tool)
        duplicate=copy.deepcopy(doc);duplicate['files'][1]['source']=duplicate['files'][0]['source']
        wrong=copy.deepcopy(doc);wrong['files'][0]['destination']='scripts/arbiter/source_rescue/runbook_history/unexpected.md'
        for invalid in (duplicate,wrong):
            with self.subTest(invalid=invalid['files'][0]['destination']), mock.patch.object(Path,'exists',return_value=False), mock.patch.object(Path,'open') as open_file, mock.patch.object(tool,'payload',side_effect=AssertionError('Payload consumed before roster refusal')):
                with self.assertRaises(ValueError):tool.apply(invalid)
                open_file.assert_not_called()
if __name__=='__main__':unittest.main(verbosity=2)
