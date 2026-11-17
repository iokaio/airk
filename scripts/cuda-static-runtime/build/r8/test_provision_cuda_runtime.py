# SPDX-License-Identifier: Apache-2.0
"""Cheap producer controls only. No build/valid DLL/provider/model calls."""
import hashlib,importlib.util,json,stat,tempfile,unittest,zipfile
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
HERE=Path(__file__).resolve().parent
s=importlib.util.spec_from_file_location('gpu_producer',HERE/'provision_cuda_runtime.py');g=importlib.util.module_from_spec(s);s.loader.exec_module(g)
s=importlib.util.spec_from_file_location('cpu',g.ROOT/'scripts/provision-static-onnx-runtime.py');cpu=importlib.util.module_from_spec(s);s.loader.exec_module(cpu)
class ProducerControls(unittest.TestCase):
 def test_fp32_single_core_exact_toolchain_flags(self):
  stub=SimpleNamespace(configure_argv=lambda *a:['cmake','-Donnxruntime_USE_CUDA=OFF','-Donnxruntime_BUILD_SHARED_LIB=OFF','-Donnxruntime_DISABLE_CONTRIB_OPS=OFF'])
  c=g.gpu_command(stub,Path('C:/owned'),{},Path('C:/owned/frontend'))
  for expected in ['-Donnxruntime_USE_CUDA=ON','-Donnxruntime_BUILD_SHARED_LIB=OFF','-Donnxruntime_BUILD_CUDA_EP_AS_PLUGIN=OFF','-DCMAKE_CUDA_ARCHITECTURES=86','-DCMAKE_OBJECT_PATH_MAX=240','-Donnxruntime_USE_FLASH_ATTENTION=ON','-Donnxruntime_FLASH_NVCC_THREADS=1','-Donnxruntime_USE_MEMORY_EFFICIENT_ATTENTION=ON','-Donnxruntime_USE_TRT_FUSED_ATTENTION=OFF','-Donnxruntime_CUDA_MINIMAL=OFF']:
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
 def test_wrong_before_refuses_before_patch_dispatch(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);(root/'first.cc').write_bytes(b'trusted one');(root/'second.cc').write_bytes(b'changed two');calls=[]
   guards=[{'path':'first.cc','before_sha256':hashlib.sha256(b'trusted one').hexdigest()},{'path':'second.cc','before_sha256':hashlib.sha256(b'trusted two').hexdigest()}]
   with self.assertRaises(ValueError):g.after_patch_target_checks(cpu,root,guards,lambda:calls.append('launch'))
   self.assertEqual(calls,[])
 def test_all_declared_before_targets_allow_one_dispatch(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);(root/'file.cc').write_bytes(b'trusted');calls=[]
   result=g.after_patch_target_checks(cpu,root,[{'path':'file.cc','before_sha256':hashlib.sha256(b'trusted').hexdigest()}],lambda:(calls.append('launch'),'done')[1])
   self.assertEqual(result,'done');self.assertEqual(calls,['launch'])
 def test_exact_input_closure_same_size_drift_refused(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);rows=[]
   for name in ['original-cuda-source.zip','copied-mirror.zip','recorded-compiler.exe']:
    f=root/name;f.write_bytes(b'trusted-input');rows.append({'path':str(f),'bytes':13,'sha256':hashlib.sha256(b'trusted-input').hexdigest()})
   self.assertEqual(g.verify_input_closure(cpu,rows),3)
   for row in rows:
    with self.subTest(role=Path(row['path']).name):
     f=Path(row['path']);f.write_bytes(b'changed-input')
     with self.assertRaises(ValueError):g.verify_input_closure(cpu,rows)
     f.write_bytes(b'trusted-input')
 def test_exact_mirror_roster_refuses_extra_archive(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);(root/'declared.zip').write_bytes(b'trusted');expected=[{'path':'declared.zip','bytes':7,'sha256':hashlib.sha256(b'trusted').hexdigest()}]
   g.verify_mirror_closure(cpu,root,expected);(root/'extra.zip').write_bytes(b'unknown')
   with self.assertRaises(ValueError):g.verify_mirror_closure(cpu,root,expected)
 def test_nonwindows_dynamic_macros_preserved_byte_exact(self):
  with zipfile.ZipFile(g.INPUT/'cuda-source-dependencies/cudnn_frontend.zip') as z:original=z.read('cudnn-frontend-1.27.0/include/cudnn_frontend_shim.h').decode('utf8')
  key='#define NV_FE_CALL_TO_CUDA';i=original.index(key);j=original.index('\n#else\n',i);expected=original[i:j]
  after=(HERE/'frontend-shim.after').read_text(encoding='utf8');i=after.index(key);j=after.index('\n#else\n',i);k=after.index(key,j);end=after.index('\n#endif\n',k)
  self.assertEqual(after[k:end],expected)
  self.assertIn('#ifdef _WIN32',after[max(0,i-30):i]);self.assertIn('return cuda_symbol(__VA_ARGS__);',after[i:j])

 def test_descriptive_child_is_enforced_before_output_or_acl_process(self):
  with patch.object(g.subprocess,'run',return_value=SimpleNamespace(stdout=g.SID)) as processes:
   with self.assertRaisesRegex(ValueError,'exact fresh child'):
    g.identity_and_parent(cpu,g.PARENT/'cu4',{'build_path':{'fresh_child':'cu5'}})
   self.assertEqual(processes.call_count,1)

class CapturedConfigurationControls(unittest.TestCase):
 CAPTURED=Path('C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/cu6')
 def setUp(self):
  self.receipt=json.loads((self.CAPTURED/'gpu-build-receipt.json').read_text(encoding='utf8'))
  self.lock=json.loads((HERE/'cuda-runtime-producer-lock.json').read_text(encoding='utf8'))
  self.command=g.gpu_command(cpu,self.CAPTURED,self.receipt['toolchain'],self.CAPTURED/'frontend')
  self.cache=(self.CAPTURED/'b/Release/CMakeCache.txt').read_text(encoding='utf8')
  self.ninja=(self.CAPTURED/'b/Release/build.ninja').read_text(encoding='utf8')
 def test_actual_captured_cache_all_flags_pass_without_cpu_changes(self):
  cache=cpu.verify_cmake_cache(self.CAPTURED/'b/Release/CMakeCache.txt',self.command)
  self.assertEqual(cache['FETCHCONTENT_SOURCE_DIR_CUDNN_FRONTEND'],(self.CAPTURED/'frontend').as_posix())
  self.assertGreater(sum(x.startswith('-D') for x in self.command),50)
 def test_captured_cache_semantic_changes_refused(self):
  import re
  changes={'FETCHCONTENT_SOURCE_DIR_CUDNN_FRONTEND':(self.CAPTURED/'other-frontend').as_posix(),'onnxruntime_USE_CUDA':'OFF','onnxruntime_BUILD_SHARED_LIB':'ON','CMAKE_CUDA_ARCHITECTURES':'80','onnxruntime_USE_FLASH_ATTENTION':'OFF','CMAKE_CUDA_COMPILER':'C:/foreign/nvcc.exe'}
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   for key,value in changes.items():
    with self.subTest(key=key):
     altered,count=re.subn(r'^'+re.escape(key)+r':([^=]*)=.*$',lambda m:key+':'+m.group(1)+'='+value,self.cache,count=1,flags=re.M);self.assertEqual(count,1)
     f=Path(t)/'cache.txt';f.write_bytes(altered.encode())
     with self.assertRaises(ValueError):cpu.verify_cmake_cache(f,self.command)
 def proxy(self,root,ninja=None,cache=None,wrong_source=False):
  actual_ninja=self.CAPTURED/'b/Release/build.ninja';actual_cache=self.CAPTURED/'b/Release/CMakeCache.txt';capi=self.CAPTURED/'source'/self.lock['final_placement']['path']
  if ninja is not None:(root/'build.ninja').write_bytes(ninja.encode())
  if cache is not None:(root/'cache.txt').write_bytes(cache.encode())
  def plain(path,regular=False):
   path=Path(path)
   if path==actual_ninja and ninja is not None:return cpu.plain(root/'build.ninja',regular=regular)
   return cpu.plain(path,regular=regular)
  def verify(path,command):return cpu.verify_cmake_cache(root/'cache.txt' if cache is not None else path,command)
  def digest(path):return '0'*64 if wrong_source and Path(path)==capi else cpu.digest(path)
  return SimpleNamespace(plain=plain,verify_cmake_cache=verify,digest=digest)
 def test_actual_configured_source_and_targets_pass(self):
  result=g.configured_build_preflight(cpu,self.CAPTURED,self.command,self.lock)
  self.assertGreater(result['objects'],0);self.assertLessEqual(result['max_object_path'],240)
 def test_missing_build_target_refused(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   proxy=self.proxy(Path(t),ninja=self.ninja.replace('build re2:','build foreign-re2:',1))
   with self.assertRaises(ValueError):g.configured_build_preflight(proxy,self.CAPTURED,self.command,self.lock)
 def test_changed_final_source_refused_before_compile(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   with self.assertRaises(ValueError):g.configured_build_preflight(self.proxy(Path(t),wrong_source=True),self.CAPTURED,self.command,self.lock)
 def test_unsupported_compiler_override_refused(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   proxy=self.proxy(Path(t),cache=self.cache+'\nCMAKE_CUDA_FLAGS:STRING=--allow-unsupported-compiler\n')
   with self.assertRaises(ValueError):g.configured_build_preflight(proxy,self.CAPTURED,self.command,self.lock)

if __name__=='__main__':unittest.main()
