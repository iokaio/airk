# SPDX-License-Identifier: Apache-2.0
"""Synthetic decision boundaries; no native emitter/model/Cargo execution."""
import copy,hashlib,json,unittest,uuid
from pathlib import Path
import prepare_native_bridge as bridge

class OpaqueInput(unittest.TestCase):
 def setUp(self):
  self.before={'source_manifest':{'original':'manifest'},'profile':{'original':'profile'},'files':{'neural':'original-source','metadata':'old-metadata'},'unknown_future_field':{'keep':[1,2,3]},'numeric_fixture':None}
  self.profile={'assets':[{'role':'model','name':'neural'},{'role':'compiler','name':'metadata'}]}
 def test_new_native_source_and_metadata_paths_preserve_all_semantics(self):
  fresh=copy.deepcopy(self.before);fresh['source_manifest']={'new':'manifest'};fresh['profile']={'new':'profile'};fresh['files']['metadata']='new-metadata'
  bridge.compare_request(self.before,fresh,self.profile)
  self.assertEqual(fresh['unknown_future_field'],{'keep':[1,2,3]})
 def test_unknown_native_semantics_cannot_disappear_or_change(self):
  for variant in ['changed','missing','added']:
   fresh=copy.deepcopy(self.before)
   if variant=='changed':fresh['unknown_future_field']['keep'][1]=99
   elif variant=='missing':fresh.pop('unknown_future_field')
   else:fresh['unreviewed_new_field']=True
   with self.assertRaises(ValueError):bridge.compare_request(self.before,fresh,self.profile)
 def test_equal_byte_neural_copy_path_still_refuses(self):
  fresh=copy.deepcopy(self.before);fresh['files']['neural']='flat-copy-of-same-bytes'
  with self.assertRaisesRegex(ValueError,'reference-sealed'):bridge.compare_request(self.before,fresh,self.profile)
 def test_asset_roster_and_fixture_cannot_change(self):
  for changed in ['roster','fixture']:
   fresh=copy.deepcopy(self.before)
   if changed=='roster':fresh['files']['other']='new'
   else:fresh['numeric_fixture']={'unreviewed':'fixture'}
   with self.assertRaises(ValueError):bridge.compare_request(self.before,fresh,self.profile)

class CargoInputs(unittest.TestCase):
 def test_genuine_record_requirements_are_not_inferred_from_inventory(self):
  rows=[{'reason':'compiler-artifact','profile':{'test':True},'executable':'synthetic-a'},
        {'reason':'compiler-artifact','profile':{'test':True},'executable':'synthetic-b'},
        {'reason':'build-script-executed','package_id':'registry#ort-sys@2.0.0-rc.10','linked_libs':['static=onnxruntime'],'linked_paths':['native=synthetic-static-ort']}]
  images=[{'original_path':'synthetic-a'},{'original_path':'synthetic-b'}]
  scratch=bridge.HERE/'test-state';scratch.mkdir(exist_ok=True)
  def capture(values):
   path=scratch/('synthetic-cargo-'+uuid.uuid4().hex+'.jsonl');path.write_text('\n'.join(json.dumps(row) for row in values)+'\n',encoding='utf-8')
   return str(path),hashlib.sha256(path.read_bytes()).hexdigest()
  result=bridge.cargo_inputs(*capture(rows),images)
  self.assertEqual(len(result['actual_artifacts']),2)
  self.assertEqual(len(result['actual_static_ort_records']),1)
  for values in [rows[:2],rows[1:],rows+[rows[0]]]:
   with self.assertRaises(ValueError):bridge.cargo_inputs(*capture(values),images)
 def test_mechanical_bridge_has_exact_raw_and_decoded_source_binding(self):
  preparation,_,_,sources=bridge.baseline()
  self.assertEqual(len(sources),19)
  proof=json.loads((bridge.HERE/'line-ending-source-bridge.json').read_bytes())
  self.assertEqual(len(proof['sources']),4)
  for row in proof['sources']:
   original=bridge.checked(row['proposal']).read_bytes();live=bridge.checked(row['live']).read_bytes()
   self.assertEqual(original.decode('utf-8').splitlines(),live.decode('utf-8').splitlines())
   self.assertEqual(original.replace(b'\r\n',b'\n'),live.replace(b'\r\n',b'\n'))
   self.assertEqual(preparation['source_changes'][row['repository_path']],row['live']['sha256'])

if __name__=='__main__':unittest.main()