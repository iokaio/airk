# SPDX-License-Identifier: Apache-2.0
"""Synthetic pre/post seal and partial-output refusal; no TRAIN corpus/model."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import types
import unittest
HERE=Path(__file__).resolve().parent
def sha(raw):return hashlib.sha256(raw).hexdigest()
class SourceStageTests(unittest.TestCase):
    def fixture(self,mutate=False):
        owned=tempfile.TemporaryDirectory(dir=HERE,prefix='source-stage-test-');self.addCleanup(owned.cleanup);root=Path(owned.name).resolve()
        (root/'base/Lib').mkdir(parents=True);(root/'base/DLLs').mkdir()
        runtime=root/'source_runtime.py';runtime.write_bytes((HERE/'source_runtime.py').read_bytes())
        converter=root/'converter.py';code='from pathlib import Path\nimport sys\np=Path(sys.argv[-1]);p.mkdir();(p/"manifest.private.json").write_text("{}")\n'
        if mutate:code+='Path(__file__).write_text("raise RuntimeError(123)\\n")\n'
        converter.write_text(code);output=root/'output'
        rows=[{'path':str(p),'sha256':sha(p.read_bytes()),'bytes':p.stat().st_size} for p in (runtime,converter)]
        review={'stdlib_runtime_file_rows':[],'inputs':rows,'python_executable':{'path':str(root/'base/python.exe')},'source_command':[str(root/'base/python.exe'),'-I','-S','-B','-X','pycache_prefix='+str(root/'cache'),str(converter),'--output',str(output)]}
        packet=root/'review.json';packet.write_text(json.dumps(review));module=types.ModuleType('_synthetic_stage');module.__file__=str(root/'source_stage.py');exec(compile((HERE/'source_stage.py').read_bytes(),module.__file__,'exec'),module.__dict__)
        return module,packet,converter,output
    def run_stage(self,module,packet):
        saved=sys.argv
        try:sys.argv=[module.__file__,str(packet),sha(packet.read_bytes())];module.main()
        finally:sys.argv=saved
    def test_valid_captured_source_has_postchecked_receipt(self):
        m,p,_,output=self.fixture();self.run_stage(m,p);r=json.loads((output/'operation-receipt.json').read_bytes());self.assertTrue(r['no_model_or_native_execution']);self.assertEqual(r['verified_files_pre_and_post'],2)
    def test_changed_input_refuses_before_any_output(self):
        m,p,converter,output=self.fixture();converter.write_text('raise RuntimeError("replacement")')
        with self.assertRaises(ValueError):self.run_stage(m,p)
        self.assertFalse(output.exists())
    def test_postexecution_drift_never_yields_success_receipt(self):
        m,p,_,output=self.fixture(mutate=True)
        with self.assertRaises(ValueError):self.run_stage(m,p)
        self.assertTrue((output/'manifest.private.json').exists());self.assertFalse((output/'operation-receipt.json').exists())
if __name__=='__main__':unittest.main()
