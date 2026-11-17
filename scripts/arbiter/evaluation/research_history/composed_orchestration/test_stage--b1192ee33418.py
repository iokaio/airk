# SPDX-License-Identifier: Apache-2.0
import copy,json,unittest
from pathlib import Path
from stage_request import ORIGINAL,ORIGINAL_SHA,checked,derive

class OpaqueStage(unittest.TestCase):
 def test_original_native_request_context_stays_exact(self):
  checked(ORIGINAL,ORIGINAL_SHA);original=json.loads(ORIGINAL.read_bytes());saved=copy.deepcopy(original)
  graph={k:{'path':'C:/future/'+k,'sha256':'1'*64} for k in ['graph','shape_proof','owned_creation','baseline_input']}
  staged=derive(original,graph,{'path':'C:/future/actual-build.json','sha256':'2'*64},[],Path('C:/future/fresh-native-output'),'3'*40)
  self.assertEqual(original,saved)
  for key in ['source_catalog','acquisition_receipt','asset_census','asset_root','repository_root']:self.assertEqual(staged[key],original[key])
  self.assertEqual(staged['compact_graph'],graph);self.assertEqual(staged['schema'],'airk.private.context-metadata-regeneration/2')
  staged['source_catalog']['path']='changed';self.assertEqual(original,saved)
 def test_existing_override_and_wrong_schema_refused(self):
  original=json.loads(ORIGINAL.read_bytes())
  for changed in [dict(original,schema='other'),dict(original,compact_graph={})]:
   with self.assertRaises(ValueError):derive(changed,{}, {},[],Path('C:/unused'),'0'*40)
if __name__=='__main__':unittest.main()