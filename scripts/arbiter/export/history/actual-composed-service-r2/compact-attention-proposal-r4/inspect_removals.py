# SPDX-License-Identifier: Apache-2.0
"""Read-only original graph metadata/dependency audit; never builds a candidate."""
from pathlib import Path
import argparse,collections,hashlib,json,sys

def main():
    p=argparse.ArgumentParser();p.add_argument('--output');args=p.parse_args()
    base=Path(__file__).resolve().parent
    plan=json.loads((base/'graph-plan.json').read_bytes())
    sys.path.insert(0,plan['python_site']);import onnx
    raw=Path(plan['source']['path']).read_bytes()
    assert hashlib.sha256(raw).hexdigest()==plan['source']['sha256']=='c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4'
    m=onnx.ModelProto();m.ParseFromString(raw)
    producers={o:n for n in m.graph.node for o in n.output};consumers=collections.defaultdict(list)
    for n in m.graph.node:
        for x in n.input:consumers[x].append(n)
    known={v.name for v in m.graph.input}|{i.name for i in m.graph.initializer}
    def reach(overrides):
        pending=[o.name for o in m.graph.output];live=set();visited=set()
        while pending:
            value=pending.pop()
            if value in known or value in visited:continue
            visited.add(value)
            if value in overrides:pending.extend(overrides[value]);continue
            n=producers[value];live.add(n.name);pending.extend(x for x in n.input if x)
        return live
    original=reach({})
    overrides={f'/scorer/layers.{i}/self_attn/Reshape_7_output_0':[
        f'/scorer/layers.{i}/self_attn/Add_output_0',f'/scorer/layers.{i}/self_attn/Add_1_output_0',
        f'/scorer/layers.{i}/self_attn/v_proj/MatMul_output_0','/scorer/Where_output_0','attention_mask'] for i in range(28)}
    required=reach(overrides);removed=[n for n in m.graph.node if n.name not in required];removed_names={n.name for n in removed}
    outside=[n for n in removed if not any(n.name.startswith(f'/scorer/layers.{i}/self_attn/') for i in range(28))]
    from gate_protocol import removable_attention_axis_constant
    details=[];layer_counts=[0]*28
    for n in outside:
        t=n.attribute[0].t;value=onnx.numpy_helper.to_array(t).tolist()
        users=[u for o in n.output for u in consumers[o]]
        row={'name':n.name,'op':n.op_type,'inputs':list(n.input),'outputs':list(n.output),'dtype':t.data_type,'dims':list(t.dims),'values':value,
            'source_node_sha256':hashlib.sha256(n.SerializeToString(deterministic=True)).hexdigest(),
            'consumers':[{'name':u.name,'op':u.op_type,'inputs':list(u.input),'retained_after_boundary_substitution':u.name in required} for u in users]}
        layer=removable_attention_axis_constant(row['name'],row['op'],row['inputs'],row['outputs'],(row['dtype'],row['dims'],row['values']),row['consumers'],removed_names)
        layer_counts[layer]+=1;details.append(row)
    projections=[n.name for n in removed if any(part in n.name for part in ['/q_proj/','/k_proj/','/v_proj/','/o_proj/','/q_norm/','/k_norm/'])]
    assert len(m.graph.node)==7162 and len(original)==7162 and len(removed)==3248 and len(details)==336 and layer_counts==[12]*28 and projections==[]
    result={'source_sha256':hashlib.sha256(raw).hexdigest(),'scope':'Original graph metadata and boundary-dependency reachability only; no candidate built',
        'original_nodes':7162,'original_reachable_nodes':7162,'original_dead_nodes':0,'hypothetical_removed_nodes':3248,
        'hypothetical_removed_op_counts':dict(collections.Counter(n.op_type for n in removed)),
        'nonattention_removal_count':336,'global_axis_constants_per_layer':layer_counts,'removed_projection_or_qknorm_nodes':projections,'nonattention_nodes':details}
    if args.output:
        out=Path(args.output);assert out.parent.resolve()==base and not out.exists()
        with out.open('x',encoding='utf-8') as f:json.dump(result,f,indent=2);f.write('\n')
    print(json.dumps({k:v for k,v in result.items() if k!='nonattention_nodes'}))
if __name__=='__main__':main()