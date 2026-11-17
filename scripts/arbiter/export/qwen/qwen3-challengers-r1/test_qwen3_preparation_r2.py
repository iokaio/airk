# SPDX-License-Identifier: Apache-2.0
"""No Torch/tensors: cheap independent admission refusal controls."""
import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
import qwen3_reference_r2 as subject
ASSETS = Path(__file__).resolve().parent.parent / 'qwen3-challenger-assets-r1'
class PreparationTests(unittest.TestCase):
    def config(self):
        return json.loads((ASSETS / 'candidate-assets/Qwen--Qwen3-4B/config.json').read_bytes())
    def test_manifest_identity(self):
        self.assertEqual(subject.sha(ASSETS/'research-asset-pins.json'),subject.MANIFEST_SHA)
    def test_both_exact_configs(self):
        for name,layers,hidden,heads in [('Qwen--Qwen3-1.7B',28,2048,16),('Qwen--Qwen3-4B',36,2560,32)]:
            c=json.loads((ASSETS/'candidate-assets'/name/'config.json').read_bytes())
            subject.validate_config(c)
            self.assertEqual((c['num_hidden_layers'],c['hidden_size'],c['num_attention_heads'],c['head_dim']),(layers,hidden,heads,128))
    def test_qwen35_refused(self):
        c=self.config();c['model_type']='qwen3_5'
        with self.assertRaisesRegex(ValueError,'ordinary dense'):subject.validate_config(c)
    def test_unsupported_parameterization_refused(self):
        for key,value in [('use_sliding_window',True),('tie_word_embeddings',False),('head_dim',80),('attention_bias',True),('rope_scaling',{'type':'dynamic'}),('rope_theta',10000)]:
            c=self.config();c[key]=value
            with self.assertRaises(ValueError,msg=key):subject.validate_config(c)
    def test_asset_digest_and_size_refusals(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'asset';p.write_bytes(b'publisher');sha=hashlib.sha256(b'publisher').hexdigest()
            subject.verify_asset(p,{'bytes':9,'sha256':sha})
            with self.assertRaisesRegex(ValueError,'length'):subject.verify_asset(p,{'bytes':8,'sha256':sha})
            with self.assertRaisesRegex(ValueError,'digest'):subject.verify_asset(p,{'bytes':9,'sha256':'0'*64})
    def test_git_blob_not_raw_sha1(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'asset';p.write_bytes(b'publisher')
            pin={'bytes':9,'sha256':None,'git_blob':hashlib.sha1(b'blob 9\0publisher').hexdigest()}
            subject.verify_asset(p,pin);pin['git_blob']=hashlib.sha1(b'publisher').hexdigest()
            with self.assertRaisesRegex(ValueError,'digest'):subject.verify_asset(p,pin)
if __name__=='__main__':unittest.main(verbosity=2)
