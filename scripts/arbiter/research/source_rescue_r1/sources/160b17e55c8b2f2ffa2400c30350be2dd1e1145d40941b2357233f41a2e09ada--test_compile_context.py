# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,tempfile,unittest
from pathlib import Path
s=importlib.util.spec_from_file_location('preserver',Path(__file__).with_name('preserve_first_fp32_images.py'));p=importlib.util.module_from_spec(s);s.loader.exec_module(p)
class Controls(unittest.TestCase):
 def test_absent_or_incomplete_context(self):
  self.assertIsNone(p.compile_context(None,None))
  for path,sha in [(None,'a'*64),(Path('missing'),None)]:
   with self.assertRaises(ValueError):p.compile_context(path,sha)
 def test_exact_byte_pin_and_drift(self):
  with tempfile.TemporaryDirectory(dir=Path(__file__).parent) as t:
   q=Path(t)/'context.json';q.write_bytes(b'{"scope":"inert test"}');sha=hashlib.sha256(q.read_bytes()).hexdigest();self.assertEqual(p.compile_context(q,sha)['sha256'],sha);q.write_bytes(b'changed')
   with self.assertRaises(ValueError):p.compile_context(q,sha)
if __name__=='__main__':unittest.main()
