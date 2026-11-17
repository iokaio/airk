# SPDX-License-Identifier: Apache-2.0
"""Synthetic CPU tensors only, no corpus/model weights/checkpoint training."""
import copy
import unittest
import torch
from bag_loss import encode_checkpointed, logits_for_bags, loss

torch.set_num_threads(1)

class BagLossTests(unittest.TestCase):
    def test_exact_max_and_first_tie_gradient(self):
        queries=torch.tensor([[1.,0.]])
        units=torch.tensor([[1.,0.],[1.,0.],[0.,1.]],requires_grad=True)
        actual=logits_for_bags(queries,units,[[0,1],[2]],temperature=1.)
        self.assertEqual(actual.tolist(),[[1.,0.]])
        actual[0,0].backward()
        self.assertEqual(units.grad.tolist(),[[1.,0.],[0.,0.],[0.,0.]])

    def test_empty_and_all_alternatives_masked_refuse(self):
        q=torch.tensor([[1.,0.]])
        u=torch.tensor([[1.,0.],[0.,1.]])
        with self.assertRaises(ValueError):logits_for_bags(q,u,[[]])
        with self.assertRaises(ValueError):loss(q,u,[[0],[1]],torch.tensor([[False,True]]),torch.tensor([0]))
        with self.assertRaises(ValueError):loss(q,u,[[0],[1]],torch.tensor([[True,False]]),torch.tensor([0]))

    def test_checkpointed_microbatches_preserve_dropout_loss_all_gradients_and_rng(self):
        torch.manual_seed(123)
        original=torch.nn.Sequential(torch.nn.Linear(6,8),torch.nn.Dropout(.25),torch.nn.Linear(8,4))
        original.register_parameter('intentionally_unused',torch.nn.Parameter(torch.tensor([3.])))
        query=torch.randn(2,6)
        passages=torch.randn(9,6)
        bags=[[0,1,2],[2,3,4],[4,5,6],[7,8]]
        mask=torch.tensor([[False,False,False,True],[False,False,True,False]])
        observed=[]
        for checkpointed in [False,True]:
            model=copy.deepcopy(original)
            def encode(value):return torch.nn.functional.normalize(model(value),dim=1)
            torch.manual_seed(789)
            q=encode(query)
            u=encode_checkpointed(encode,(passages,),microbatch=2,checkpointed=checkpointed)
            value=loss(q,u,bags,mask,torch.tensor([0,1]))
            value.backward()
            winners=[(q.detach()@u.detach()[bag].T).max(dim=1).indices for bag in bags]
            observed.append((value.detach(),[p.grad.clone() if p.grad is not None else None for p in model.parameters()],torch.get_rng_state(),winners))
        torch.testing.assert_close(observed[0][0],observed[1][0],rtol=0,atol=0)
        for a,b in zip(observed[0][1],observed[1][1]):
            if a is None:self.assertIsNone(b)
            else:torch.testing.assert_close(a,b,rtol=0,atol=0)
        self.assertTrue(torch.equal(observed[0][2],observed[1][2]))
        for a,b in zip(observed[0][3],observed[1][3]):self.assertTrue(torch.equal(a,b))

if __name__=='__main__':unittest.main()
