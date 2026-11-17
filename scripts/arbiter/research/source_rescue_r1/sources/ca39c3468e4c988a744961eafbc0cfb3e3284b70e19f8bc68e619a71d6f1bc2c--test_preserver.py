# SPDX-License-Identifier: Apache-2.0
"""Cheap literal Cargo/PE/access-policy controls only; never actual native image copies."""
import importlib.util,json,tempfile,unittest
from pathlib import Path
s=importlib.util.spec_from_file_location('preserver',Path(__file__).with_name('preserve_cuda_checkpoint_images.py'));p=importlib.util.module_from_spec(s);s.loader.exec_module(p)
class Controls(unittest.TestCase):
 def rows(self):
  return [{'reason':'build-finished','success':True},{'reason':'build-script-executed','package_id':'registry#ort-sys@2.0.0-rc.10','linked_libs':['static=onnxruntime'],'linked_paths':['native='+str(p.ARCHIVE.parent)]}]+[{'reason':'compiler-artifact','profile':{'test':True},'executable':'fixture.exe','target':{'kind':['lib'],'name':name.replace('-','_')},'manifest_path':str(p.R/'crates'/name/'Cargo.toml'),'features':['gpu-cuda']} for name in ['airk-inference','airk-retrieval']]
 def test_exact_two_actual_json_artifact_selection(self):
  _,rows=p.cargo_rows('\n'.join(map(json.dumps,self.rows())));self.assertEqual([name for name,row in rows],['airk-inference','airk-retrieval'])
 def test_cpu_link_and_missing_gpu_feature_refused(self):
  for change in ['link','feature']:
   rows=self.rows()
   if change=='link':rows[1]['linked_paths']=['native=historical-cpu']
   else:rows[2]['features']=[]
   with self.assertRaises(ValueError):p.cargo_rows('\n'.join(map(json.dumps,rows)))
 def test_duplicate_artifact_or_build_finish_refused(self):
  for extra in [self.rows()[2],{'reason':'build-finished','success':False}]:
   with self.assertRaises(ValueError):p.cargo_rows('\n'.join(map(json.dumps,self.rows()+[extra])))
 def test_other_ort_copy_refused(self):
  rows=self.rows();rows.append(dict(rows[1],package_id='registry#ort-sys@other'))
  with self.assertRaises(ValueError):p.cargo_rows('\n'.join(map(json.dumps,rows)))
 def test_pe64_only_executable_header(self):
  with tempfile.TemporaryDirectory(dir=Path(__file__).parent) as t:
   q=Path(t)/'fixture.exe';data=bytearray(128);data[:2]=b'MZ';data[60:64]=(64).to_bytes(4,'little');data[64:68]=b'PE\0\0';data[68:70]=(0x8664).to_bytes(2,'little');data[88:90]=(0x20b).to_bytes(2,'little');q.write_bytes(data);self.assertEqual(p.pe(q)['machine_u16'],0x8664)
   data[86:88]=(0x2000).to_bytes(2,'little');q.write_bytes(data)
   with self.assertRaises(ValueError):p.pe(q)
 def test_private_parent_rejects_other_writer_without_acl_changes(self):
  p.private_owner({'owner_sid':p.SID,'rules':[{'sid':'foreign-read','rights':0x1200a9,'type':'Allow'}]})
  with self.assertRaises(ValueError):p.private_owner({'owner_sid':p.SID,'rules':[{'sid':'foreign-write','rights':2032127,'type':'Allow'}]})
if __name__=='__main__':unittest.main()
