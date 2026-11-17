# SPDX-License-Identifier: Apache-2.0
"""Cheap producer controls only. No build/valid DLL/provider/model calls."""
import hashlib,importlib.util,json,stat,tempfile,unittest,zipfile
from pathlib import Path
from types import SimpleNamespace
HERE=Path(__file__).resolve().parent
s=importlib.util.spec_from_file_location('gpu_producer',HERE/'provision_cuda_runtime.py');g=importlib.util.module_from_spec(s);s.loader.exec_module(g)
s=importlib.util.spec_from_file_location('cpu',g.ROOT/'scripts/provision-static-onnx-runtime.py');cpu=importlib.util.module_from_spec(s);s.loader.exec_module(cpu)
class ProducerControls(unittest.TestCase):
 def test_fp32_single_core_exact_toolchain_flags(self):
  stub=SimpleNamespace(configure_argv=lambda *a:['cmake','-Donnxruntime_USE_CUDA=OFF','-Donnxruntime_BUILD_SHARED_LIB=OFF','-Donnxruntime_DISABLE_CONTRIB_OPS=OFF'])
  c=g.gpu_command(stub,Path('C:/owned'),{},Path('C:/owned/frontend'))
  for expected in ['-Donnxruntime_USE_CUDA=ON','-Donnxruntime_BUILD_SHARED_LIB=OFF','-Donnxruntime_BUILD_CUDA_EP_AS_PLUGIN=OFF','-DCMAKE_CUDA_ARCHITECTURES=86','-Donnxruntime_USE_FLASH_ATTENTION=OFF','-Donnxruntime_USE_MEMORY_EFFICIENT_ATTENTION=OFF','-Donnxruntime_USE_TRT_FUSED_ATTENTION=OFF','-Donnxruntime_CUDA_MINIMAL=OFF']:
   self.assertIn(expected,c)
  self.assertTrue(any(v.startswith('-DCMAKE_CUDA_COMPILER=') and v.replace(chr(92),'/').endswith('bin/nvcc.exe') for v in c));self.assertNotIn('-Donnxruntime_USE_CUDA=OFF',c)
 def test_cpu_recipe_contract_drift_refused(self):
  with self.assertRaises(ValueError):g.gpu_command(SimpleNamespace(configure_argv=lambda *a:['cmake']),Path('C:/owned'),{},Path('C:/owned/frontend'))
 def fixture(self,root,extra=None):
  arc=root/'source.zip';rows=[('cudnn-frontend-1.27.0/include/test.h',b'fixture header\n',stat.S_IFREG|0o644),('cudnn-frontend-1.27.0/.claude/skills',b'../skills',stat.S_IFLNK|0o777)]
  if extra:rows.append(extra)
  with zipfile.ZipFile(arc,'w') as z:
   for name,data,mode in rows:
    info=zipfile.ZipInfo(name);info.create_system=3;info.external_attr=mode<<16;z.writestr(info,data)
  lock={'dependencies':[{'name':'cudnn_frontend','bytes':arc.stat().st_size,'sha256':g.digest(arc),'entries':len(rows),'expanded_bytes':sum(len(r[1]) for r in rows)}],'excluded_link':{'path':'.claude/skills','bytes':9,'target_sha256':hashlib.sha256(b'../skills').hexdigest()}}
  return arc,lock
 def test_exact_nonbuild_link_exclusion_no_materialized_alias(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);arc,lock=self.fixture(root);dst=root/'out';r=g.safe_frontend(cpu,arc,dst,lock)
   self.assertEqual(r['excluded_inert_link'],['.claude/skills']);self.assertEqual((dst/'include/test.h').read_bytes(),b'fixture header\n');self.assertFalse((dst/'.claude/skills').exists())
 def test_extra_link_fails_before_materialization(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);arc,lock=self.fixture(root,('cudnn-frontend-1.27.0/include/evil',b'../../outside',stat.S_IFLNK|0o777));dst=root/'out'
   with self.assertRaises(ValueError):g.safe_frontend(cpu,arc,dst,lock)
   self.assertFalse(dst.exists())
 def test_traversal_fails_before_materialization(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);arc,lock=self.fixture(root,('cudnn-frontend-1.27.0/../outside',b'bad',stat.S_IFREG|0o644));dst=root/'out'
   with self.assertRaises(ValueError):g.safe_frontend(cpu,arc,dst,lock)
   self.assertFalse(dst.exists())
 def test_archive_hash_drift_refused(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);arc,lock=self.fixture(root);arc.write_bytes(arc.read_bytes()+b'drift');dst=root/'out'
   with self.assertRaises(ValueError):g.safe_frontend(cpu,arc,dst,lock)
   self.assertFalse(dst.exists())
if __name__=='__main__':unittest.main()
