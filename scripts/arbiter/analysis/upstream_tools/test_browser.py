# SPDX-License-Identifier: Apache-2.0
"""Pure signed-metadata refusal and tree-hash arithmetic controls."""
import hashlib
import json
import base64
from pathlib import Path
import unittest
import project_browser as p

class BrowserControls(unittest.TestCase):
    def test_independent_rsa_vector_and_mutations(self):
        vector=json.loads(Path(__file__).with_name('rsa-test-vector.json').read_bytes())
        key=base64.b64decode(vector['public_key']);signed={'payload':vector['payload']}
        signature={'protected':vector['protected'],'signature':vector['signature']}
        p.verify_signature(signed,signature,key)
        with self.assertRaises(ValueError):p.verify_signature({'payload':'dGFtcGVyZWQ'},signature,key)
        mutated=bytearray(p.decode(signature['signature']));mutated[-1]^=1
        signature['signature']=base64.urlsafe_b64encode(mutated).decode().rstrip('=')
        with self.assertRaises(ValueError):p.verify_signature(signed,signature,key)
    def test_single_block_standard_vector(self):
        self.assertEqual(p.tree_hash(b'abc',4096,4096).hex(),'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad')
    def test_two_blocks_preserve_order(self):
        expected=hashlib.sha256(hashlib.sha256(b'a'*4096).digest()+hashlib.sha256(b'b').digest()).digest()
        self.assertEqual(p.tree_hash(b'a'*4096+b'b',4096,4096),expected)
        self.assertNotEqual(p.tree_hash(b'b'+b'a'*4096,4096,4096),expected)
    def test_unknown_block_sizes_refuse(self):
        with self.assertRaises(ValueError):p.tree_hash(b'x',2048,4096)
    def test_malformed_key_and_base64_refuse(self):
        with self.assertRaises(ValueError):p.rsa_key(b'wrong')
        with self.assertRaises(ValueError):p.decode('not canonical=')

if __name__=='__main__':unittest.main(verbosity=2)
