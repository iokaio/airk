# SPDX-License-Identifier: Apache-2.0
import importlib.util,json,subprocess,unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock,patch
candidate=Path(__file__).with_name('policy_owner.py')
if not candidate.exists():candidate=Path(__file__).resolve().parents[1]/'cuda-static-runtime/verification/policy_owner.py'
spec=importlib.util.spec_from_file_location('policy_owner',candidate);owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
class PolicyControls(unittest.TestCase):
 def test_exact_historical_policy_with_stronger_identity_guard(self):
  policy={'owner':'TYAI'+chr(92)+'tdotn','sddl':owner.expected_parent_sddl()}
  with patch.object(owner,'digest',return_value=owner.GUARD_SHA),patch.object(Path,'exists',return_value=False),patch.object(owner.subprocess,'run',return_value=SimpleNamespace(stdout=json.dumps(policy))) as run:
   self.assertEqual(owner.identity_and_parent(SimpleNamespace(plain=lambda p:p),owner.PARENT/'fresh',{'build_path':{'fresh_child':'fresh'}}),policy)
  self.assertIn('Assert-AirkOutputParent',run.call_args.args[0][-1]);self.assertEqual(run.call_args.kwargs['timeout'],10)
 def test_wrong_destination_refuses_before_process(self):
  with patch.object(Path,'exists',return_value=False),patch.object(owner.subprocess,'run') as run:
   with self.assertRaises(ValueError):owner.identity_and_parent(SimpleNamespace(plain=lambda p:p),Path('C:/foreign/fresh'),{'build_path':{'fresh_child':'fresh'}})
  run.assert_not_called()
 def test_guard_refusal_or_changed_source_is_not_bypassed(self):
  for changed in [False,True]:
   with patch.object(owner,'digest',return_value='wrong' if changed else owner.GUARD_SHA),patch.object(Path,'exists',return_value=False),patch.object(owner.subprocess,'run',side_effect=subprocess.CalledProcessError(1,'guard')):
    with self.assertRaises((ValueError,subprocess.CalledProcessError)):owner.identity_and_parent(SimpleNamespace(plain=lambda p:p),owner.PARENT/'fresh',{'build_path':{'fresh_child':'fresh'}})
 def test_late_guard_return_cannot_authorize_output(self):
  policy={'owner':'TYAI'+chr(92)+'tdotn','sddl':owner.expected_parent_sddl()}
  with patch.object(owner,'digest',return_value=owner.GUARD_SHA),patch.object(Path,'exists',return_value=False),patch.object(owner.subprocess,'run',return_value=SimpleNamespace(stdout=json.dumps(policy))),patch.object(owner.time,'monotonic',side_effect=[1,2,42]):
   with self.assertRaises(TimeoutError):owner.identity_and_parent(SimpleNamespace(plain=lambda p:p),owner.PARENT/'fresh',{'build_path':{'fresh_child':'fresh'}},deadline=42)
 def test_cpu_owner_drift_refuses_before_import(self):
  with patch.object(owner,'digest',return_value='wrong'),patch.object(owner.importlib.util,'spec_from_file_location') as load:
   with self.assertRaises(ValueError):owner.cpu_owner({'cpu_owner_sha256':'expected','cpu_lock_sha256':'expected'})
  load.assert_not_called()
if __name__=='__main__':unittest.main()
