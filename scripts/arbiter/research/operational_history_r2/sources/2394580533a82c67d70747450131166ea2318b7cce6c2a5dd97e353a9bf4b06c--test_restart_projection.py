# SPDX-License-Identifier: Apache-2.0
import copy, tempfile, unittest
from pathlib import Path
import prepare_release as owner
class RestartProjection(unittest.TestCase):
 def test_exact_owner_restart_and_four_foreign_refusals(self):
  with tempfile.TemporaryDirectory() as temp:
   root=Path(temp);control=root/'control';control.mkdir();foreign=root/'foreign';foreign.mkdir()
   old={'control_root':str(control),'health':{'instance':'original','catalog_clock':{'namespace':'same'}}}
   status={'configuration':{'revision':3}}
   saved={'control':str(control),'configuration':{'revision':3},'prior_instance':'original'}
   ready={'health':{'instance':'new','catalog_clock':{'namespace':'same'}}}
   owner.same_owner_projection(old,status,saved,ready,control)
   for field,value in [('control',str(foreign)),('configuration',{'revision':2}),('prior_instance','foreign')]:
    bad=copy.deepcopy(saved);bad[field]=value
    with self.assertRaises(ValueError):owner.same_owner_projection(old,status,bad,ready,control)
   bad=copy.deepcopy(ready);bad['health']['instance']='original'
   with self.assertRaises(ValueError):owner.same_owner_projection(old,status,saved,bad,control)
   bad=copy.deepcopy(ready);bad['health']['catalog_clock']['namespace']='foreign'
   with self.assertRaises(ValueError):owner.same_owner_projection(old,status,saved,bad,control)
