# SPDX-License-Identifier: Apache-2.0
"""Verify the four additional FP32 products at all 36 Large layers in actual ORT session metadata."""
from collections import defaultdict

CUDA="CUDAExecutionProvider"
SITES={"self_attn/o_proj":(4096,2560), "mlp/gate_proj":(2560,9728),
       "mlp/up_proj":(2560,9728), "mlp/down_proj":(9728,2560)}

def require(ok,message):
    if not ok:
        raise ValueError(message)

def verify(graph):
    rows=[r for r in graph["rows"] if r["graph"]==0]
    nodes={r["node"]:r for r in rows if r["kind"]=="node"}
    names={r["name"]:i for i,r in nodes.items()}
    values={r["value"]:r for r in rows if r["kind"]=="value"}
    dims={(r["value"],r["index"]):r["number"] for r in rows if r["kind"]=="dimension"}
    ports=defaultdict(dict)
    users=defaultdict(list)
    for row in rows:
        if row["kind"]=="edge" and row["node"] is not None:
            port=ports[row["node"],row["role"]]
            require(row["slot"] not in port,"duplicate actual node port")
            port[row["slot"]]=row["value"]
            if row["role"]==0:
                users[row["value"]].append(row["node"])
    def typed(i,role,types):
        p=ports[i,role]
        require(set(p)==set(range(len(types))),"original input/output arity differs")
        v=[values[p[j]] for j in range(len(types))]
        require(all(x["type_state"]=="tensor" and x["element_type"]==t for x,t in zip(v,types,strict=True)),
                "Large live projection value dtype differs")
        return v
    def op(i,kind):
        n=nodes[i]
        require(n["op"]==kind and n["domain"]=="" and n["ep"]==CUDA,"expected CUDA "+kind+": "+n["name"])
        return n["name"]
    def prior(v):
        i=v["producer"]
        name=op(i,"Cast")
        y,=typed(i,2,(1,))
        require(y["value"]==v["value"],"disconnected widening Cast")
        x,=typed(i,0,(10,))
        return x,name
    weights=set()
    activations=set()
    for layer in range(36):
        root=f"/scorer/layers.{layer}/"
        for site,size in SITES.items():
            name=root+site+"/MatMul"
            stem="/airk_large_body_fp32"+name
            i=names[name]
            op(i,"MatMul")
            a,b=typed(i,0,(1,1))
            y,=typed(i,2,(1,))
            half,cast=prior(b)
            require(cast==stem+"/weight32" and half["provenance"]=="immutable_session_initializer"
                    and half["rank"]==2 and tuple(dims[half["value"],j] for j in (0,1))==size
                    and half["value"] not in weights,"Large original stored weight, shape or use differs: "+name)
            weights.add(half["value"])
            x,activation=prior(a)
            require(x["rank"]==3 and dims[x["value"],2]==size[0],"original FP16 activation shape differs: "+name)
            if site=="self_attn/o_proj":
                require(x["name"] in (f"/airk_g5/attention/{layer}/raw16","/airk_g5/weight/"+name+"/input16"),
                        "output projection detached from original attention operand")
            else:
                allowed=(("mlp/gate_proj","mlp/up_proj") if site in ("mlp/gate_proj","mlp/up_proj") else (site,))
                require(x["name"] in {"/airk_g5/weight/"+root+s+"/MatMul/input16" for s in allowed},
                        "MLP projection detached from original half operand: "+name)
            activations.add(activation)
            nxt,=users[y["value"]]
            require(op(nxt,"Cast")==stem+"/output16","FP32 product did not narrow exactly once: "+name)
            out,=typed(nxt,2,(10,))
            require(out["name"]=="/airk_g5/weight/"+name+"/output16","original half boundary changed: "+name)
    require(len(weights)==144,"incomplete Large extra projection roster")
    return {"additional_cuda_fp32_matmuls":len(weights),"distinct_stored_half_weights":len(weights),
            "distinct_activation_upcasts":len(activations),"original_fp16_output_boundaries":144}
