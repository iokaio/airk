# SPDX-License-Identifier: Apache-2.0
"""Join retained placement names to original declarations; never authorize CPU execution."""
import collections,hashlib,json,time
from pathlib import Path
E=Path('D:/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z')
P=Path('C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/gpu-placement-diagnostic-owned-r1/native-result.json')
S=E/'gpu-compact17-static-graph-inventory-r1.json'
def load(p,h):
 raw=p.read_bytes();assert hashlib.sha256(raw).hexdigest()==h;return json.loads(raw)
r=load(P,'9f027c05bd447f98422684e6ec33ff1008149a26fe7c134dd47c66f72fc01bce')
s=load(S,'9f48f32e31ad243f058b5aef19d630e3ee06991dc5bcc27e9aab14c73894a44d')
assert r['placement']['complete'] and len(r['placement']['nodes'])==2786
nodes=s['graph']['nodes'];byname=collections.defaultdict(list)
for n in nodes:byname[n['name']].append(n)
placed=r['placement']['nodes'];cpu=[p for p in placed if p['ep']=='CPUExecutionProvider']
facts={};tensors={}
for item in s['graph']['initializers']:
 if not item['external'] and item['element_type'] in ['INT32','INT64','BOOL']:
  tensors[item['name']]={'kind':'embedded_integer_initializer','shape_seed':False,'type':item['element_type']}
# This is only an original-graph ancestry candidate, not a final-graph proof.
allowed={'Gather','Slice','Unsqueeze','Squeeze','Concat','Reshape','Add','Sub','Mul','Div','Mod','Min','Max','Neg'}
for n in nodes:
 attrs={a['name']:a for a in n['attributes']};inputs=[tensors.get(x['name']) for x in n['inputs'] if x['name']]
 fact=None
 if n['domain'] in ['', 'ai.onnx']:
  if n['operator'] in ['Shape','Size']:
   fact={'kind':'original_shape_metadata_seed','shape_seed':True,'type':'INT64'}
  elif n['operator']=='Constant':
   ts=[a['tensor'] for a in n['attributes'] if 'tensor' in a]
   if len(ts)==1 and ts[0]['element_type'] in ['INT32','INT64','BOOL'] and not ts[0]['external']:
    fact={'kind':'embedded_integer_constant','shape_seed':False,'type':ts[0]['element_type']}
  elif inputs and all(inputs) and n['operator'] in allowed:
   data=inputs[0]
   if data['type'] in ['INT32','INT64'] and all(x['type'] in ['INT32','INT64'] for x in inputs):
    fact={'kind':'original_integer_ancestry_candidate','shape_seed':any(x['shape_seed'] for x in inputs),'type':data['type']}
  elif inputs and all(inputs) and n['operator']=='Cast' and attrs.get('to',{}).get('value') in [6,7]:
   fact={'kind':'original_integer_ancestry_candidate','shape_seed':any(x['shape_seed'] for x in inputs),'type':'INT64' if attrs['to']['value']==7 else 'INT32'}
 if fact:
  facts[n['index']]=fact
  for output in n['outputs']:tensors[output['name']]=fact
result=[]
for p in cpu:
 matches=byname[p['name']]
 exact=[n for n in matches if n['domain']==p['domain'] and n['operator']==p['op']]
 if p['subgraph_index']!=0:status='nonoriginal_subgraph_unproven'
 elif len(exact)==1:status='exact_name_domain_operator'
 elif len(exact)>1:status='ambiguous_original_duplicate'
 elif matches:status='same_name_changed_domain_or_operator'
 else:status='missing_original_name_possibly_generated'
 row={'placement':p,'join_status':status,'original_matches':[{'index':n['index'],'domain':n['domain'],'operator':n['operator']} for n in matches]}
 if status=='exact_name_domain_operator':
  n=exact[0];row['original_ancestry_candidate']=facts.get(n['index'])
  row['original_input_producers']=n['inputs'];row['original_output_consumers']=n['outputs']
  row['original_unclassified_input_names']=[x['name'] for x in n['inputs'] if x['name'] and x['name'] not in tensors]
  row['unproven']=['optimized input/output identity and edges','optimized element types and shapes','value/cardinality bounds','all final consumers and payload crossing','original-name match does not prove unchanged optimized node semantics']
 else:row['unproven']=['final optimized node definition, input/output edges, types, shapes and provenance']
 result.append(row)
counts=collections.Counter(x['join_status'] for x in result)
candidates=[x for x in result if x.get('original_ancestry_candidate')]
out={'schema':'airk.private-cpu-placement-static-join/1','placement_input':{'path':str(P),'sha256':'9f027c05bd447f98422684e6ec33ff1008149a26fe7c134dd47c66f72fc01bce'},'original_static_inventory':{'path':str(S),'sha256':'9f48f32e31ad243f058b5aef19d630e3ee06991dc5bcc27e9aab14c73894a44d'},'diagnostic_outcome':r['outcome'],'diagnostic_complete':r['diagnostic_complete'],'diagnostic_error':r['diagnostic_error'],'placement_complete':True,'placement_counts':dict(collections.Counter(p['ep'] for p in placed)),'cpu_operator_counts':dict(sorted(collections.Counter(p['domain']+'::'+p['op'] for p in cpu).items())),'join_counts':dict(counts),'original_ancestry_candidate_counts':dict(collections.Counter(x['original_ancestry_candidate']['kind'] for x in candidates)),'original_shape_seed_reachable_candidates':sum(x['original_ancestry_candidate']['shape_seed'] for x in candidates),'cpu_rows':result,'authorization':False,'limits':{'maximum_report_bytes':16777216},'interpretation':'Complete retained placement map is evidence despite overall diagnostic FAIL from log truncation. Exact names/domain/op only join original nodes; optimized edges/types are not known. Candidate original integer/shape ancestry is non-authoritative and deliberately excludes floating Cast and unknown inputs. No op whitelist or hybrid CPU permission follows. Metadata bounds and actual optimized graph closure require a bounded native collector. No session/model/weights loaded.','generator_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
raw=json.dumps(out,indent=2).encode()+b'\n';assert len(raw)<=16777216
p=E/'gpu-placement-static-cpu-join-r1.json'
with p.open('xb') as f:f.write(raw)
print(json.dumps({'path':str(p),'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw),**{k:out[k] for k in ['placement_counts','cpu_operator_counts','join_counts','original_ancestry_candidate_counts','original_shape_seed_reachable_candidates']}}))
