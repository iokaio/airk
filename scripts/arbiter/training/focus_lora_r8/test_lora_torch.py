# SPDX-License-Identifier: Apache-2.0
"""PREPARED ONLY: Root runs after source review. CPU toy tensors, never selected models."""
import copy,types,unittest
import torch
from focus_worker import inject_lora,merge_lora,response_loss
class Attention(torch.nn.Module):
    def __init__(self):
        super().__init__();self.q_proj=torch.nn.Linear(8,8,bias=False);self.k_proj=torch.nn.Linear(8,8,bias=False);self.v_proj=torch.nn.Linear(8,8,bias=False);self.o_proj=torch.nn.Linear(8,8,bias=False)
    def forward(self,x):return self.o_proj(self.q_proj(x)+self.k_proj(x)+self.v_proj(x))
class Layer(torch.nn.Module):
    def __init__(self):super().__init__();self.self_attn=Attention()
class Backbone(torch.nn.Module):
    def __init__(self):super().__init__();self.embed_tokens=torch.nn.Embedding(10000,8);self.layers=torch.nn.ModuleList([Layer(),Layer()])
class Tiny(torch.nn.Module):
    def __init__(self):
        super().__init__();self.model=Backbone();self.lm_head=torch.nn.Linear(8,10000,bias=False);self.lm_head.weight=self.model.embed_tokens.weight;self.config=types.SimpleNamespace(num_hidden_layers=2,vocab_size=10000)
    def forward(self,x):
        for layer in self.model.layers:x=layer.self_attn(x)
        return x
class PreparedTorchControls(unittest.TestCase):
    def model(self):
        torch.manual_seed(1908);m=Tiny()
        for p in m.parameters():p.requires_grad_(False)
        return m
    def test_zero_adapter_is_exact_original_and_target_roster_literal(self):
        m=self.model();x=torch.randn(2,8);before=m(x);targets=inject_lora(torch,m)
        self.assertEqual(targets,['model.layers.0.self_attn.q_proj','model.layers.0.self_attn.k_proj','model.layers.0.self_attn.v_proj','model.layers.0.self_attn.o_proj','model.layers.1.self_attn.q_proj','model.layers.1.self_attn.k_proj','model.layers.1.self_attn.v_proj','model.layers.1.self_attn.o_proj'])
        self.assertTrue(torch.equal(before,m(x)))
        self.assertTrue(all(not p.requires_grad for n,p in m.named_parameters()if'lora_'not in n))
    def test_actual_full_vocabulary_loss_and_gradients_not_two_head_loss(self):
        logits=torch.zeros(1,10000,requires_grad=True)
        with torch.no_grad():logits[0,17]=20;logits[0,9693]=2
        loss=response_loss(torch,logits,1,10000);self.assertGreater(loss.item(),17.99);loss.backward();self.assertGreater(logits.grad[0,17].item(),.99);self.assertLess(logits.grad[0,9693].item(),-.99)
        with self.assertRaises(ValueError):response_loss(torch,torch.zeros(1,2),1,10000)
    def test_merge_matches_trained_adapter_and_keeps_tied_full_head(self):
        m=self.model();targets=inject_lora(torch,m);x=torch.randn(2,8)
        with torch.no_grad():
            for name,p in m.named_parameters():
                if'lora_B'in name:p.fill_(.002)
        before=m(x).detach();merge_lora(torch,m,targets);self.assertTrue(torch.allclose(before,m(x),atol=1e-6,rtol=1e-6));self.assertEqual(m.lm_head.weight.data_ptr(),m.model.embed_tokens.weight.data_ptr());self.assertFalse(any('lora_'in n for n,_ in m.named_parameters()))
    def test_final_partial_accumulation_uses_actual_group_size(self):
        a=torch.tensor([1.,2.,3.],requires_grad=True);sum(v/3 for v in a).backward();self.assertTrue(torch.allclose(a.grad,torch.tensor([1/3]*3)))
if __name__=='__main__':unittest.main(verbosity=2)
