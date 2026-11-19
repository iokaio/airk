# SPDX-License-Identifier: Apache-2.0
"""Pure explicit-custody routing/refusal controls; no private fixture or deletion."""
import json
import unittest
from unittest.mock import patch
import cleanup
import custody

MAP='scripts/research/source-map.json'
SOURCE='artifacts/reviewed/source.py'
BODY=b'print(1)\n'

def map_raw():return json.dumps({'files':[{'source':SOURCE,'original_sha256':cleanup.digest(BODY),'original_bytes':len(BODY)}]}).encode()
def reader(values):
    def read(path):
        if path not in values:raise FileNotFoundError(path)
        return values[path]
    return read

class CustodyControls(unittest.TestCase):
    def test_present_original_has_explicit_custody_result(self):
        with patch.object(cleanup,'read',side_effect=reader({MAP:map_raw(),SOURCE:BODY})):
            result=custody.audit_maps([MAP])
        self.assertEqual((result['selected'],result['passed'],result['refused']),(1,1,0))
        self.assertEqual(result['files'][0]['status'],'present-original')
    def test_missing_or_changed_original_never_skips(self):
        for values in ({MAP:map_raw()},{MAP:map_raw(),SOURCE:b'changed'}):
            with self.subTest(present=SOURCE in values),patch.object(cleanup,'read',side_effect=reader(values)):
                result=custody.audit_maps([MAP])
            self.assertEqual((result['selected'],result['passed'],result['refused']),(1,0,1))
    def test_missing_original_routes_only_to_exact_pinned_removal(self):
        result_pin={'path':'artifacts/audit/result.json','sha256':'a'*64,'bytes':99}
        raw=json.dumps({'schema':'airk.source-cleanup-removal-roster/1','files':[{'source':SOURCE,'result':result_pin}]}).encode()
        roster={'path':'artifacts/audit/roster.json','sha256':cleanup.digest(raw),'bytes':len(raw)}
        with patch.object(cleanup,'read',side_effect=reader({MAP:map_raw(),roster['path']:raw})),patch.object(cleanup,'audit_original',return_value='proved-removed-original') as audit:
            result=custody.audit_maps([MAP],roster)
        self.assertEqual(result['passed'],1);audit.assert_called_once()
        self.assertEqual(audit.call_args.args[1],result_pin)
        self.assertEqual(audit.call_args.args[0]['source_map']['sha256'],cleanup.digest(map_raw()))
    def test_map_drift_at_final_check_refuses(self):
        count=0
        def read(path):
            nonlocal count
            if path==SOURCE:return BODY
            count+=1;return map_raw() if count==1 else b'changed'
        with patch.object(cleanup,'read',side_effect=read),self.assertRaisesRegex(ValueError,'pinned input changed'):custody.audit_maps([MAP])
    def test_default_cli_does_not_open_originals(self):
        with patch.object(custody,'audit_maps',side_effect=AssertionError('unexpected audit')):
            self.assertFalse(custody.custody_cli([MAP],[]))
    def test_removal_roster_requires_audit_and_complete_pin(self):
        for argv in (['--removal-roster','artifacts/x.json'],['--audit-originals','--removal-roster','artifacts/x.json'],['--audit-originals','--unknown']):
            with self.subTest(argv=argv),self.assertRaises(ValueError):custody.custody_cli([MAP],argv)

if __name__=='__main__':unittest.main(verbosity=2)
