# SPDX-License-Identifier: Apache-2.0
"""Pure end-barrier and batched-observer controls; no child process is launched."""
import hashlib
import json
import subprocess
import unittest
from unittest.mock import Mock,patch
import cleanup as owner

class PlanningControls(unittest.TestCase):
    def test_declared_original_and_maintained_map_lengths_refuse(self):
        from test_cleanup import inputs
        for key in ('original_bytes','source_bytes','bytes','maintained_bytes','destination_bytes'):
            row,mapped,kept,observation,review=inputs()
            mapped['files'][0][key]=999
            with self.subTest(key=key),self.assertRaisesRegex(ValueError,'map-declared'):
                owner.candidate(row,mapped,kept,observation,review)
    def pins(self):
        return [dict(path=n,sha256=hashlib.sha256(b'fixed').hexdigest(),bytes=5) for n in ('request.json','consumer.json','map.json','cleanup.py')]
    def test_unchanged_snapshot_checks_each_input_and_two_heads(self):
        head=Mock(return_value='a'*40);reader=Mock(return_value=b'fixed')
        owner.revalidate_snapshot('a'*40,self.pins(),head,reader)
        self.assertEqual(head.call_count,2);self.assertEqual(reader.call_count,4)
    def test_head_change_before_or_after_refuses(self):
        for heads in (['b'*40],['a'*40,'b'*40]):
            with self.subTest(heads=heads),self.assertRaisesRegex(ValueError,'HEAD changed'):
                owner.revalidate_snapshot('a'*40,self.pins(),Mock(side_effect=heads),lambda p:b'fixed')
    def test_each_input_class_drift_refuses(self):
        for changed in ('request.json','consumer.json','map.json','cleanup.py'):
            with self.subTest(changed=changed),self.assertRaisesRegex(ValueError,'input changed'):
                owner.revalidate_snapshot('a'*40,self.pins(),lambda:'a'*40,lambda p:b'drift' if p['path']==changed else b'fixed')
    def test_length_is_verified_independently(self):
        pins=self.pins();pins[0]['bytes']=6
        with self.assertRaisesRegex(ValueError,'input changed'):owner.revalidate_snapshot('a'*40,pins,lambda:'a'*40,lambda p:b'fixed')
    def test_one_batch_process_uses_stdin_and_preserves_roster(self):
        sources=['artifacts/a.py','artifacts/b.py'];rows=[dict(source=s,error='synthetic refusal') for s in sources]
        completed=subprocess.CompletedProcess([],0,json.dumps(rows).encode(),b'')
        with patch.object(owner.subprocess,'run',return_value=completed) as run:
            result=owner.observe_many(sources)
        run.assert_called_once();args,kwargs=run.call_args
        self.assertEqual(args[0][-1],'-BatchStdin');self.assertNotIn(sources[0],args[0]);self.assertEqual(json.loads(kwargs['input']),sources)
        self.assertEqual(set(result),set(sources))
    def test_batch_missing_or_substituted_roster_refuses(self):
        for rows in ([],[dict(source='artifacts/other.py')],dict(source='artifacts/a.py')):
            completed=subprocess.CompletedProcess([],0,json.dumps(rows).encode(),b'')
            with patch.object(owner.subprocess,'run',return_value=completed),self.assertRaises(ValueError):owner.observe_many(['artifacts/a.py'])

if __name__=='__main__':unittest.main(verbosity=2)
