# SPDX-License-Identifier: Apache-2.0
"""Verify 252 direct FP32 learned products against actual Windows Large finalized session metadata."""
SITES={"self_attn/q_proj":("input_layernorm/Mul_1_output_0",2560,4096),
       "self_attn/k_proj":("input_layernorm/Mul_1_output_0",2560,1024),
       "self_attn/v_proj":("input_layernorm/Mul_1_output_0",2560,1024),
       "self_attn/o_proj":("self_attn/Reshape_7_output_0",4096,2560),
       "mlp/gate_proj":("post_attention_layernorm/Mul_1_output_0",2560,9728),
       "mlp/up_proj":("post_attention_layernorm/Mul_1_output_0",2560,9728),
       "mlp/down_proj":("mlp/Mul_output_0",9728,2560)}

def require(ok,message):
    if not ok:
        raise ValueError(message)

def verify(graph):
    rows=[r for r in graph["rows"] if r["graph"]==0]
    nodes={r["node"]:r for r in rows if r["kind"]=="node"}
    names={r["name"]:i for i,r in nodes.items()}
    values={r["value"]:r for r in rows if r["kind"]=="value"}
    dims={(r["value"],r["index"]):r["number"] for r in rows if r["kind"]=="dimension"}
    edges={(r["node"],r["role"],r["slot"]):r["value"] for r in rows if r["kind"]=="edge"}
    def v(i,role,slot):
        return values[edges[i,role,slot]]
    def cuda(n,op):
        require(n["domain"]=="" and n["ep"]=="CUDAExecutionProvider" and n["op"]==op,"unexpected Large CUDA "+op)
    stores=set()
    for layer in range(36):
        root=f"/scorer/layers.{layer}/"
        for site,(act,inner,out) in SITES.items():
            name=root+site+"/MatMul"
            i=names[name]
            cuda(nodes[i],"MatMul")
            a,w,y=v(i,0,0),v(i,0,1),v(i,2,0)
            require(a["element_type"]==w["element_type"]==y["element_type"]==1 and
                    a["name"]==root+act and y["name"]==name+"_output_0" and
                    a["rank"]==y["rank"]==3 and dims[a["value"],2]==inner and dims[y["value"],2]==out,
                    "direct activation/output signature differs: "+name)
            widen=nodes[w["producer"]]
            cuda(widen,"Cast")
            prefix="/airk_large_qkv_fp32" if site in ("self_attn/q_proj","self_attn/k_proj","self_attn/v_proj") else "/airk_large_body_fp32"
            stored=v(widen["node"],0,0)
            require(widen["name"]==prefix+name+"/weight32" and stored["rank"]==2 and stored["element_type"]==10 and
                    stored["provenance"]=="immutable_session_initializer" and tuple(dims[stored["value"],j] for j in (0,1))==(inner,out)
                    and stored["value"] not in stores,"independent Large half-weight identity or shape differs: "+name)
            stores.add(stored["value"])
        for suffix in ("Add","Add_1"):
            i=names[root+suffix]
            cuda(nodes[i],"Add")
            require(v(i,0,0)["element_type"]==v(i,0,1)["element_type"]==v(i,2,0)["element_type"]==1,"Large residual left FP32")
    require(len(stores)==252,"incomplete direct learned model")
    return {"direct_cuda_fp32_products":252,"distinct_half_weights":252,"fp32_residual_additions":72}
