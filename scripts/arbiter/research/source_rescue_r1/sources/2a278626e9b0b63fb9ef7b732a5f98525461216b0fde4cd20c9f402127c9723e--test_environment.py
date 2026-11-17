# SPDX-License-Identifier: Apache-2.0
import ast,importlib.util,json,os,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
HERE=Path(__file__).resolve().parent
p=HERE/'producer_environment.py';s=importlib.util.spec_from_file_location('producer_env',p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
class EnvironmentControls(unittest.TestCase):
 def test_exact_original_environment_ast(self):
  source=json.loads((HERE/'environment-source.json').read_bytes());want=ast.parse(source['raw_environment_block']).body;tree=ast.parse(p.read_bytes());actual=next(n for n in tree.body if isinstance(n,ast.FunctionDef)).body[:-1]
  self.assertEqual([ast.dump(n,include_attributes=False) for n in actual],[ast.dump(n,include_attributes=False) for n in want])
 def test_forbidden_flags_and_provider_variables_scrubbed(self):
  incoming={k:'poison' for k in ['CC','CXX','CFLAGS','CXXFLAGS','CPPFLAGS','LDFLAGS','CUDAFLAGS','CUDACXX','CUDAHOSTCXX','CUDAARCHS','NVCC_PREPEND_FLAGS','NVCC_APPEND_FLAGS','CL','_CL_','LINK','LIB','LIBPATH','INCLUDE','PYTHONPATH','PYTHONHOME','CUDNN_FRONTEND_CUDART_LIB_NAME','CMAKE_X','ORT_X','CUDNN_X','CUDA_X']};incoming['KEEP_MARKER']='keep'
  with tempfile.TemporaryDirectory(dir=HERE) as t,patch.dict(os.environ,incoming,clear=True):
   value=m.producer_environment(Path(t),Path('C:/approved/cuda'))
   self.assertEqual(value['KEEP_MARKER'],'keep')
   for key in incoming:
    if key!='KEEP_MARKER':self.assertNotIn(key,value)
   self.assertEqual(dict(os.environ),incoming)
 def test_private_temp_cache_and_controlled_path(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t,patch.dict(os.environ,{},clear=True):
   root=Path(t);value=m.producer_environment(root,Path('C:/approved/cuda'))
   for key in ['TEMP','TMP','TMPDIR']:self.assertEqual(value[key],str(root/'t'))
   self.assertEqual(value['CUDA_CACHE_PATH'],str(root/'cc'));self.assertTrue((root/'t').is_dir());self.assertEqual(value['PYTHONNOUSERSITE'],'1');self.assertTrue(value['PATH'].startswith('C:/Windows/System32;C:/Windows;'))
 def test_no_guessed_telemetry_override_or_second_helper_reuse(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t,patch.dict(os.environ,{},clear=True):
   value=m.producer_environment(Path(t),Path('C:/approved/cuda'));self.assertNotIn('VSCMD_SKIP_SENDTELEMETRY',value);self.assertNotIn('VCTIP_NO_NOTIFY',value)
   with self.assertRaises(FileExistsError):m.producer_environment(Path(t),Path('C:/approved/cuda'))
if __name__=='__main__':unittest.main()
