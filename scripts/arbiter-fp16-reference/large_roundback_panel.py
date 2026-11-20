# SPDX-License-Identifier: Apache-2.0
"""Compare the actual layer-zero shared half input, independent FP32 linear, and measured CUDA treatment."""
import hashlib
from pathlib import Path

GAR = "c5e99be11646cc665075ee098d24a539664b1e282caaa7806360fb9a86b75988"
LARGE_GRAPH = "e9a59c3de81194f49c7aacd0f6a9104cc156feaeb198ffd1cc94d73f7634cc2f"
COMPACT_GRAPH = "cbcc3706b621225507c9e1e955116dd0aa35d162db1f6c1c97a031974457b40b"

def graph_cuts(graph, layers):
    rows = [r for r in graph["rows"] if r["graph"]==0]
    nodes = {r["node"]:r for r in rows if r["kind"]=="node"}
    values = {r["value"]:r for r in rows if r["kind"]=="value"}
    names = {r["name"]:r["node"] for r in nodes.values()}
    edges = {(r["node"],r["role"],r["slot"]):r["value"] for r in rows if r["kind"]=="edge"}
    consumers = {}
    for (node,role,slot), value in edges.items():
        if role==0 and node is not None:
            consumers.setdefault(value,[]).append(node)
    counts = {"direct_input_fp32":0,"widened_half_input":0,"immediate_half_result":0,"direct_fp32_result":0,
              "fp32_residual_additions":0}
    for i in range(layers):
        root=f"/scorer/layers.{i}/"
        for label in ("self_attn/q_proj","self_attn/k_proj","self_attn/v_proj","self_attn/o_proj","mlp/gate_proj","mlp/up_proj","mlp/down_proj"):
            node=names[root+label+"/MatMul"]
            a=values[edges[node,0,0]]
            y=values[edges[node,2,0]]
            assert a["element_type"]==y["element_type"]==1 and nodes[node]["ep"]=="CUDAExecutionProvider"
            producer=nodes[a["producer"]]
            narrowed=producer["op"]=="Cast" and values[edges[producer["node"],0,0]]["element_type"]==10
            counts["widened_half_input" if narrowed else "direct_input_fp32"]+=1
            downstream=consumers[y["value"]]
            half=all(nodes[n]["op"]=="Cast" and values[edges[n,2,0]]["element_type"]==10 for n in downstream)
            counts["immediate_half_result" if half else "direct_fp32_result"]+=1
        for name in ("Add","Add_1"):
            n=names[root+name]
            assert nodes[n]["op"]=="Add" and all(values[edges[n,role,slot]]["element_type"]==1 for role,slot in ((0,0),(0,1),(2,0)))
            counts["fp32_residual_additions"]+=1
    return counts

def metrics(np, lhs, rhs):
    d=lhs.astype("float64")-rhs.astype("float64")
    return {"words":int(d.size),"different":int(np.count_nonzero(d)),"max_abs":float(np.abs(d).max()),
            "rms":float(np.sqrt(np.mean(d*d)))}

def measure(np,torch,half,exact,read_tensor,load,campaign,row,full,capture_owner):
    native=load({"path":campaign/"large-fp16-t32-layer0-three-gemm-r1/execution.json","sha256":GAR})
    large=load({"path":campaign/"windows-large-compensated-graph-r1/execution.json","sha256":LARGE_GRAPH})
    compact=load({"path":campaign/"compact-cutlass-full-model-graph-r1/execution.json","sha256":COMPACT_GRAPH})
    assert all(x["owned"]["exit_code"]==0 and x["owned"]["owned_job_final_active_processes"]==0 for x in (large,compact,native))
    assert large["result"]["result"]["typed_graph_sha256"]=="5a8a442d6dc18279ded0f0f9d8d7db2ce5d05c47de7f25339a28f536480f6338"
    census={"large":graph_cuts(large["result"]["result"]["graph"],36),"compact":graph_cuts(compact["result"]["graph"],28)}
    gpu=native["observation"]["gpu_treatment"]
    assert gpu["original_reproduced"] and native["observation"]["input_digest"]==row["input_digest"]
    embed=torch.from_numpy(read_tensor("embed_tokens.weight",row["input_ids"])).float().unsqueeze(0)
    norm=half["rms"](torch,embed,torch.from_numpy(read_tensor("layers.0.input_layernorm.weight")),1e-6)
    words=np.asarray(full[capture_owner["SHARED"]],dtype="<u2").reshape(32,2560)
    rounded=norm.half().contiguous().numpy().view("<u2").reshape(32,2560)
    mask=[0,15,31]
    assert np.array_equal(words[mask],rounded[mask])
    left=exact["half_array_units"](np,words[mask].tobytes()).reshape(3,2560)
    products={}
    torch.set_float32_matmul_precision("highest")
    for site,width in (("k",1024),("v",1024),("q",4096)):
        right=read_tensor("layers.0.self_attn."+site+"_proj.weight")
        w=torch.from_numpy(right.T.copy()).float()
        guarded=torch.nn.functional.linear(norm.half().float(),w)[0,mask].numpy()
        widened=torch.nn.functional.linear(norm,w)[0,mask].numpy()
        exact_rows=np.empty((3,width),dtype="<u2")
        for start in range(0,width,64):
            b=right[:,start:start+64].copy()
            rhs=exact["half_array_units"](np,b.tobytes()).reshape(b.shape)
            bound=int(abs(left).max())*int(abs(rhs).sum(axis=0).max())
            assert bound < 2**63
            dot=left@rhs
            exact_rows[:,start:start+b.shape[1]]=np.fromiter((exact["rounded_half_word"](int(x)) for x in dot.flat),dtype="<u2",count=dot.size).reshape(dot.shape)
        cpu=metrics(np,guarded.astype("<f2"),exact_rows.view("<f2"))
        g=gpu["routes"][site]
        assert g["evaluated_exact_dots"]==cpu["words"] and g["replayed_original_vs_captured_word_mismatches"]==0
        products[site]={"cpu_fp32_then_half_vs_exact":cpu,
                        "gpu_fp32_then_half_vs_exact_wrong_words":g["fp32_then_half_vs_exact_mismatches"],
                        "guaranteed_cpu_vs_gpu_different_words":abs(cpu["different"]-g["fp32_then_half_vs_exact_mismatches"]),
                        "effect_of_output_roundback":metrics(np,guarded,guarded.astype("<f2")),
                        "effect_of_input_roundback":metrics(np,widened,guarded),
                        "weight_only_fp32_vs_current_rounded":metrics(np,widened,guarded.astype("<f2"))}
    return {"actual_final_graph_census":census,"input_digest":row["input_digest"],"normalized_activation_words":int(words.size),
            "normalized_activation_different_words":int(np.count_nonzero(words!=rounded)),
            "activation_sha256":hashlib.sha256(words.tobytes()).hexdigest(),"positions":mask,"routes":products,
            "uses_prior_cuda_operator_measurements":True,"new_full_model_native_forwards":0}
