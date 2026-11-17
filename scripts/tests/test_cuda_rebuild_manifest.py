# SPDX-License-Identifier: Apache-2.0
"""Literal rebuild-manifest controls; no build or filesystem fixtures."""
import copy,datetime,importlib.util,unittest,io,sys
from unittest.mock import patch
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('rebuild_inputs',ROOT/'scripts/cuda-static-runtime/build/rebuild.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
def value():
 pin={'path':str(ROOT/'scripts/literal.py'),'bytes':1,'sha256':'a'*64}
 return {'schema':'airk.cuda-parent-rebuild-input/1','repository_root':str(ROOT),'campaign_cutoff_utc':'2030-01-02T00:00:00+00:00','stages':[{'name':name,'source':dict(pin),'input_contract':{**pin,'path':'C:/inputs/'+name+'.json'},'argv':['literal-not-executed',name]} for name in ['r8','r12','flash','llm','final','compose','aggregate']],'inputs':[dict(pin)],'output_directories':['C:/fresh/output'],'qualification':False}
NOW=datetime.datetime(2030,1,1,tzinfo=datetime.timezone.utc)
class RebuildManifestControls(unittest.TestCase):
 def test_structure_never_claims_execution_or_qualification(self):
  result=m.validate_manifest(value(),ROOT,NOW)
  self.assertFalse(result['execution_authorized']);self.assertFalse(result['qualification']);self.assertFalse(result['fresh_run_launcher_available'])
 def test_historical_expired_deadline_is_not_rewritten(self):
  v=value();v['campaign_cutoff_utc']='2026-11-19T05:29:42+00:00'
  with self.assertRaises(ValueError):m.validate_manifest(v,ROOT,NOW)
 def test_ignored_source_cannot_be_rebuild_implementation(self):
  v=value();v['stages'][0]['source']['path']=str(ROOT/'artifacts/private.py')
  with self.assertRaises(ValueError):m.validate_manifest(v,ROOT,NOW)
 def test_missing_reordered_and_extra_fields_refuse(self):
  for mutation in ['missing','order','extra']:
   v=value()
   if mutation=='missing':v['stages'].pop()
   elif mutation=='order':v['stages'].reverse()
   else:v['execute']=True
   with self.assertRaises(ValueError):m.validate_manifest(v,ROOT,NOW)
 def test_fake_pin_qualification_and_output_alias_refuse(self):
  for mutation in ['pin','qualification','alias']:
   v=value()
   if mutation=='pin':v['inputs'][0]['sha256']='placeholder'
   elif mutation=='qualification':v['qualification']=True
   else:v['output_directories']=['C:/fresh/../other']
   with self.assertRaises(ValueError):m.validate_manifest(v,ROOT,NOW)

 def test_oversized_manifest_read_is_bounded_before_hash_or_inputs(self):
  stream=io.BytesIO(b'x'*((4<<20)+1))
  with patch.object(sys,'argv',['rebuild','--input-manifest','C:/input.json','--manifest-sha256','a'*64]),patch.object(m.Path,'open',return_value=stream),patch.object(stream,'read',wraps=stream.read) as read,patch.object(m.hashlib,'sha256') as hashed,patch.object(m,'check_pin') as inputs:
   with self.assertRaisesRegex(ValueError,'bounded reviewed'):m.main()
  read.assert_called_once_with((4<<20)+1);hashed.assert_not_called();inputs.assert_not_called()

if __name__=='__main__':unittest.main()
