# SPDX-License-Identifier: Apache-2.0
import importlib.util,types,unittest
from pathlib import Path
import torch
p=Path(__file__).with_name('train_finite_r2.py');spec=importlib.util.spec_from_file_location('prospective_finite_bag_trainer',p);trainer=importlib.util.module_from_spec(spec);spec.loader.exec_module(trainer)
class FiniteControls(unittest.TestCase):
 def test_finite_and_unused_gradient_allowed(self):trainer.require_finite_tensors([('finite',torch.tensor([0.,1.])),('unused',None)],'synthetic')
 def test_nan_and_infinities_refused_before_training(self):
  for value in [float('nan'),float('inf'),-float('inf')]:
   with self.assertRaisesRegex(ValueError,'nonfinite synthetic: value'):trainer.require_finite_tensors([('value',torch.tensor([value]))],'synthetic')
 def test_mixed_tensor_failure_not_hidden_by_finite_neighbors(self):
  with self.assertRaisesRegex(ValueError,'bad'):trainer.require_finite_tensors([('good',torch.zeros(1)),('bad',torch.tensor([0.,float('nan')]))],'weight')
 def test_optimizer_finite_and_non_tensor_state_allowed(self):trainer.finite_optimizer(types.SimpleNamespace(state={0:{'step':torch.tensor(1.),'exp_avg':torch.zeros(2),'inert':1}}),'state')
 def test_optimizer_nonfinite_refused(self):
  with self.assertRaisesRegex(ValueError,'nonfinite state'):trainer.finite_optimizer(types.SimpleNamespace(state={0:{'step':torch.tensor(1.),'exp_avg_sq':torch.tensor([float('inf')])}}),'state')
if __name__=='__main__':unittest.main()
