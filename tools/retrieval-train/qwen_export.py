# SPDX-License-Identifier: Apache-2.0
"""Local full Qwen3.5 text architecture export; no remote model code execution.

The installed, pinned Apache Transformers architecture supplies all trained
layers. Only its GDN recurrence is lowered to ORT's built-in LinearAttention;
normalization, causal convolution, rotary/full attention and complete readouts
remain in the exported graph. This tool is an experiment until parity passes.
"""
import argparse
import hashlib
import importlib.metadata
import io
import json
from pathlib import Path
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as stream:
        while data := stream.read(4194304):
            h.update(data)
    return h.hexdigest()


def verify_asset(raw,asset):
    if len(raw)!=asset["bytes"]:raise ValueError("pinned asset length differs")
    sha=hashlib.sha256(raw).hexdigest()
    if asset["sha256"]:
        if sha!=asset["sha256"]:raise ValueError("pinned asset SHA256 differs")
    elif hashlib.sha1(("blob "+str(len(raw))+"\0").encode()+raw).hexdigest()!=asset["git_blob"]:
        raise ValueError("pinned asset Git blob differs")
    return sha


def make_wrapper(config, state=None, head=None, head_kind="intern"):
    import torch
    from torch import nn
    from transformers import Qwen3_5TextConfig, Qwen3_5TextModel
    from transformers.models.qwen3_5.modeling_qwen3_5 import torch_recurrent_gated_delta_rule
    cfg = Qwen3_5TextConfig.from_dict(config)
    cfg._attn_implementation = "eager"
    model = Qwen3_5TextModel(cfg)
    if state is not None:
        model.load_state_dict(state, strict=True)
    model = model.float().eval()
    class Recurrence(torch.autograd.Function):
        @staticmethod
        def forward(ctx, q, k, v, decay, beta, initial):
            return torch_recurrent_gated_delta_rule(q, k, v, decay, beta, initial, True, False)

        @staticmethod
        def symbolic(g, q, k, v, decay, beta, initial):
            reshape = g.op("Constant", value_t=torch.tensor([0,0,-1],dtype=torch.long))
            q3, k3, v3 = [g.op("Reshape", x, reshape) for x in (q,k,v)]
            q3 = g.op("Mul",q3,g.op("Constant",value_t=torch.tensor(cfg.linear_key_head_dim ** -.5,dtype=torch.float32)))
            y, state = g.op("com.microsoft::LinearAttention", q3,k3,v3,initial,decay,beta,
                q_num_heads_i=cfg.linear_num_value_heads, kv_num_heads_i=cfg.linear_num_value_heads,
                update_rule_s="gated_delta", scale_f=1., outputs=2)
            return g.op("Reshape", y, g.op("Shape",v)), state

    def lowered(q,k,v,g,beta,initial_state=None,output_final_state=False,use_qk_l2norm_in_kernel=False,**kwargs):
        if initial_state is not None:
            raise ValueError("this qualified profile is full prefill only")
        # Match the installed reference's FP32 normalization then query scaling.
        if use_qk_l2norm_in_kernel:
            q = q * torch.rsqrt((q*q).sum(-1,keepdim=True)+1e-6)
            k = k * torch.rsqrt((k*k).sum(-1,keepdim=True)+1e-6)
        initial = q.new_zeros(q.shape[0],cfg.linear_num_value_heads,cfg.linear_key_head_dim,cfg.linear_value_head_dim)
        y, final = Recurrence.apply(q,k,v,g,beta,initial)
        return y, final if output_final_state else None
    for layer in model.layers:
        if hasattr(layer,"linear_attn"):
            layer.linear_attn.chunk_gated_delta_rule = lowered
            layer.linear_attn.recurrent_gated_delta_rule = lowered

    class Scorer(nn.Module):
        def __init__(self):
            super().__init__()
            self.text = model
            self.kind = head_kind
            if head_kind == "intern":
                self.lm_head = nn.Linear(cfg.hidden_size,cfg.vocab_size,bias=False)
                self.lm_head.weight = self.text.embed_tokens.weight
                if head is not None and "lm_head.weight" in head:
                    self.lm_head.weight = nn.Parameter(head["lm_head.weight"].float())
            else:
                dimension = head["q.weight"].shape[0]
                self.q = nn.Linear(cfg.hidden_size,dimension)
                self.k = nn.Linear(cfg.hidden_size,dimension)
                self.scale = dimension ** -.5
                self.q.load_state_dict({key[2:]:value.float() for key,value in head.items() if key.startswith("q.")})
                self.k.load_state_dict({key[2:]:value.float() for key,value in head.items() if key.startswith("k.")})
                self.norm = nn.LayerNorm(cfg.hidden_size) if head_kind == "strands" else nn.Identity()
                if head_kind == "strands":
                    self.norm.load_state_dict({key[5:]:value.float() for key,value in head.items() if key.startswith("norm.")})

        def hidden(self, ids, mask, positions):
            hidden = self.text.embed_tokens(ids)
            position_embeddings = self.text.rotary_emb(hidden,positions)
            length = ids.shape[1]
            causal = torch.arange(length,device=ids.device).unsqueeze(0) <= torch.arange(length,device=ids.device).unsqueeze(1)
            allowed = causal.unsqueeze(0).unsqueeze(0) & mask[:,None,None,:].bool()
            additive = torch.where(allowed,torch.zeros((),device=ids.device),torch.full((),torch.finfo(torch.float32).min,device=ids.device))
            for layer in self.text.layers:
                hidden = layer(hidden,position_embeddings=position_embeddings,attention_mask=mask if hasattr(layer,"linear_attn") else additive,use_cache=False)
            return self.text.norm(hidden)

        def forward(self,input_ids,attention_mask,position_ids,decision_positions,candidate_positions,option_mask):
            hidden = self.hidden(input_ids,attention_mask,position_ids)
            decide = hidden.gather(1,decision_positions[:,None,None].expand(-1,1,cfg.hidden_size)).squeeze(1)
            if self.kind == "intern":
                # Full original trained LM head retained; only declared symbol
                # rows participate, exactly as the publisher restricts logits.
                rows = torch.nn.functional.embedding(candidate_positions,self.lm_head.weight)
                scores = (rows * decide[:,None,:]).sum(-1)
            else:
                options = hidden.gather(1,candidate_positions[:,:,None].expand(-1,-1,cfg.hidden_size))
                scores = (self.k(self.norm(options)) * self.q(self.norm(decide))[:,None,:]).sum(-1)*self.scale
            return scores.masked_fill(~option_mask,-1e4), hidden
    return Scorer().eval()


def load_candidate(root, candidate, base_only=False):
    import torch
    from safetensors.torch import load as load_tensors
    root = Path(root)
    folders={"intern08":"internlm--Intern-Decision-0.8B","intern4":"internlm--Intern-Decision-4B",
        "strands2":"StrandsAgents--strands-decider-2B-qwen3.5-v1-2610","kev4":"jaredpalmer--kev-4b"}
    chosen=root/folders[candidate]
    base=chosen if candidate.startswith("intern") else root/("Qwen--Qwen3.5-2B-Base" if candidate=="strands2" else "Qwen--Qwen3.5-4B-Base")
    catalog_path=root.parent/"research-asset-pins.json"
    catalog_raw=catalog_path.read_bytes()
    catalog=json.loads(catalog_raw)
    approved={(model["id"].replace("/","--"),asset["name"]):asset for model in catalog["models"] for asset in model["assets"]}
    state={}; head={}; seals={str(catalog_path):hashlib.sha256(catalog_raw).hexdigest()}
    def retained_asset(path):
        relative=path.relative_to(root)
        asset=approved[(relative.parts[0],"/".join(relative.parts[1:]))]
        with path.open("rb") as stream:raw=stream.read(asset["bytes"]+1)
        sha=verify_asset(raw,asset)
        seals[str(path)]=sha
        return raw
    config=json.loads(retained_asset(base/"config.json"))["text_config"]
    paths=[base/"config.json"]+sorted(base.glob("*.safetensors"))
    for path in paths:
        if path.suffix!=".safetensors":continue
        source=load_tensors(retained_asset(path))
        for key,value in source.items():
            if key.startswith("model.language_model."):
                state[key[len("model.language_model."):]]=value.float().clone()
            elif key=="lm_head.weight":head[key]=value.float().clone()
        del source
    kind="intern"
    if not candidate.startswith("intern") and not base_only:
        adapter_path=chosen/("lora/adapter_model.safetensors" if candidate=="strands2" else "adapter_model.safetensors")
        adapter_config=chosen/("lora/adapter_config.json" if candidate=="strands2" else "adapter_config.json")
        parameters=json.loads(retained_asset(adapter_config)); scaling=parameters["lora_alpha"]/parameters["r"]
        if any(parameters.get(k) for k in ("use_dora","use_rslora","rank_pattern","alpha_pattern","fan_in_fan_out")):
            raise ValueError("unsupported adapter parameterization")
        source=load_tensors(retained_asset(adapter_path))
        keys=set(source);used=set()
        for key in sorted(keys):
                if not key.endswith(".lora_A.weight"):continue
                stem=key[:-len(".lora_A.weight")]
                if stem.startswith("base_model.model.language_model."):
                    target=stem[len("base_model.model.language_model."):]+".weight"
                elif stem.startswith("base_model.model.layers."):
                    target=stem[len("base_model.model."):]+".weight"
                else:raise ValueError("unmapped adapter tensor")
                bkey=stem+".lora_B.weight"
                if target not in state or bkey not in keys:raise ValueError("adapter target missing")
                state[target].add_(source[bkey].float() @ source[key].float(),alpha=scaling)
                used.update((key,bkey))
        if used!=keys:raise ValueError("unconsumed adapter tensors")
        del source
        if candidate=="strands2":
            head_path=chosen/"head.safetensors";kind="strands"
            head={key:value.float().clone() for key,value in load_tensors(retained_asset(head_path)).items()}
        else:
            head_path=chosen/"head.pt";kind="kev"
            checkpoint=torch.load(io.BytesIO(retained_asset(head_path)),map_location="cpu",weights_only=True)
            if checkpoint["base_revision"]!="1001bb4d826a52d1f399e183466143f4da7b741b" or checkpoint.get("option_isolation") or checkpoint.get("special_embeddings"):
                raise ValueError("unsupported Kev checkpoint interpretation")
            head=dict(checkpoint["head"])
    return config,state,head,kind,seals


def main():
    import numpy as np
    import onnx
    import onnxruntime as ort
    import torch
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate",choices=("tiny","intern08","intern4","strands2","kev4"),required=True)
    parser.add_argument("--assets",type=Path,default=ROOT/"artifacts/advanced-split/models/candidate-assets")
    parser.add_argument("--output",type=Path,required=True)
    args=parser.parse_args()
    output=args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/"artifacts"):parser.error("fresh private output required")
    source_raw=Path(__file__).read_bytes(); started=time.monotonic(); torch.manual_seed(1908);torch.set_num_threads(2)
    torch.backends.cuda.matmul.allow_tf32=False
    if args.candidate=="tiny":
        config={"hidden_size":32,"intermediate_size":64,"num_hidden_layers":4,"layer_types":["linear_attention"]*3+["full_attention"],
            "num_attention_heads":4,"num_key_value_heads":2,"head_dim":8,"linear_num_key_heads":2,"linear_num_value_heads":4,
            "linear_key_head_dim":4,"linear_value_head_dim":4,"linear_conv_kernel_dim":4,"vocab_size":256,
            "rope_parameters":{"rope_type":"default","rope_theta":10000000,"partial_rotary_factor":.5,"mrope_section":[1,1,0]}}
        state=head=None;kind="intern";seals={}
    else:config,state,head,kind,seals=load_candidate(args.assets,args.candidate)
    model=make_wrapper(config,state,head,kind).to("cuda").eval();del state,head
    output.mkdir(parents=True)
    inputs=(torch.arange(1,18,device="cuda",dtype=torch.long).unsqueeze(0),torch.ones(1,17,device="cuda",dtype=torch.long),
        torch.arange(17,device="cuda",dtype=torch.long).unsqueeze(0),torch.tensor([16],device="cuda"),
        torch.tensor([[3,7,11]],device="cuda"),torch.ones(1,3,device="cuda",dtype=torch.bool))
    names=["input_ids","attention_mask","position_ids","decision_positions","candidate_positions","option_mask"]
    with torch.no_grad():expected=model(*inputs)
    torch.onnx.export(model,inputs,str(output/"model.onnx"),input_names=names,output_names=["scores","hidden_states"],
        opset_version=18,dynamo=False,do_constant_folding=True,external_data=True,
        dynamic_axes={"input_ids":{1:"tokens"},"attention_mask":{1:"tokens"},"position_ids":{1:"tokens"},
            "candidate_positions":{1:"candidates"},"option_mask":{1:"candidates"},"scores":{1:"candidates"},"hidden_states":{1:"tokens"}})
    options=ort.SessionOptions();options.intra_op_num_threads=1;options.inter_op_num_threads=1
    options.enable_cpu_mem_arena=False
    session=ort.InferenceSession(str(output/"model.onnx"),options,providers=["CPUExecutionProvider"])
    feed={name:value.detach().cpu().numpy() for name,value in zip(names,inputs)}
    actual=session.run(None,feed)
    errors=[float(np.max(np.abs(a-e.detach().cpu().numpy()))) for a,e in zip(actual,expected)]
    np.savez_compressed(output/"reference.npz",**feed,expected_scores=expected[0].detach().cpu().numpy(),
        expected_hidden=expected[1].detach().cpu().numpy(),actual_scores=actual[0],actual_hidden=actual[1])
    if Path(__file__).read_bytes()!=source_raw or any(digest(p)!=h for p,h in seals.items()):raise ValueError("source/model drift")
    result={"candidate":args.candidate,"head_kind":kind,"config":config,"maximum_absolute_errors":errors,
        "model_sha256":digest(output/"model.onnx"),"reference_sha256":digest(output/"reference.npz"),
        "source_sha256":hashlib.sha256(source_raw).hexdigest(),"input_seals":seals,"elapsed_seconds":time.monotonic()-started,
        "versions":{n:importlib.metadata.version(n) for n in ("torch","transformers","onnx","onnxruntime")},
        "scope":"Full text architecture and complete original trained scoring readout experiment; text-only full-prefill B1, no vision/nontext/cache profile. Raw scores only, own AIRK calibration and compiler/quality qualification still required."}
    (output/"runner.py").write_bytes(source_raw)
    (output/"receipt.json").write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps(result),flush=True)
    if max(errors)>1e-3:raise ValueError("architecture parity failed frozen1e-3 absolute tolerance")


if __name__=="__main__":main()
