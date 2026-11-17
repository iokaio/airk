# SPDX-License-Identifier: Apache-2.0
"""Read-only protobuf inventory; no weights, inference, shape inference or graph rewriting."""
import collections,hashlib,json,sys,time
from pathlib import Path
ROOT=Path('D:/code/Github/airk')
E=ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z'
U=Path('C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z')
request=U/'gpu-first-fp32-nn0-preparation-r3/compact-fp32-probe-metadata-r1/native-input.json'
plan=json.loads((E/'gpu-fp16-export-run-plan-r1/plan.json').read_bytes())
sys.path.insert(0,plan['python_site'])
import onnx
assert onnx.__version__=='1.19.0'
def sha(raw):return hashlib.sha256(raw).hexdigest()
def stat(p):
 s=p.stat();return {'bytes':s.st_size,'file_id':s.st_ino,'volume':s.st_dev,'links':s.st_nlink,'mtime_ns':s.st_mtime_ns}
started=time.monotonic();requestraw=request.read_bytes();req=json.loads(requestraw)
model=Path(req['files']['model.onnx']);external=Path(req['files']['model.onnx.data'])
before=stat(model);external_before=stat(external);raw=model.read_bytes();digest=sha(raw)
assert digest.startswith('c79')
graph=onnx.load_model_from_string(raw)
def tensor(t):
 return {'name':t.name,'element_type':onnx.TensorProto.DataType.Name(t.data_type),'dimensions':list(t.dims),'external':t.data_location==onnx.TensorProto.EXTERNAL,'external_metadata':{x.key:x.value for x in t.external_data},'embedded_raw_bytes':len(t.raw_data)}
def declared(v):
 t=v.type
 result={'name':v.name,'type_proto':str(t).strip()}
 if t.HasField('tensor_type'):
  tt=t.tensor_type;result['element_type']=onnx.TensorProto.DataType.Name(tt.elem_type)
  result['shape']=[d.dim_value if d.HasField('dim_value') else d.dim_param if d.HasField('dim_param') else None for d in tt.shape.dim] if tt.HasField('shape') else None
 return result
def inspect(g,scope):
 producers={out:i for i,n in enumerate(g.node) for out in n.output if out}
 consumers=collections.defaultdict(list)
 for i,n in enumerate(g.node):
  for slot,name in enumerate(n.input):
   if name:consumers[name].append({'node_index':i,'input_slot':slot})
 nodes=[];subgraphs=[]
 for i,n in enumerate(g.node):
  attrs=[]
  for a in n.attribute:
   x={'name':a.name,'type':onnx.AttributeProto.AttributeType.Name(a.type)}
   if a.type==onnx.AttributeProto.INT:x['value']=a.i
   elif a.type==onnx.AttributeProto.INTS:x['values']=list(a.ints)
   elif a.type==onnx.AttributeProto.TENSOR:
    x['tensor']=tensor(a.t)
    if a.t.data_type in [onnx.TensorProto.INT32,onnx.TensorProto.INT64] and not a.t.raw_data and len(a.t.int64_data)+len(a.t.int32_data)<=16:
     x['embedded_integer_values']=list(a.t.int64_data)+list(a.t.int32_data)
   elif a.type==onnx.AttributeProto.GRAPH:subgraphs.append(inspect(a.g,scope+'/'+str(i)+'/'+a.name))
   elif a.type==onnx.AttributeProto.GRAPHS:
    subgraphs.extend(inspect(sg,scope+'/'+str(i)+'/'+a.name+'/'+str(j)) for j,sg in enumerate(a.graphs))
   attrs.append(x)
  nodes.append({'index':i,'name':n.name,'domain':n.domain,'operator':n.op_type,'inputs':[{'name':name,'producer_node_index':producers.get(name)} for name in n.input],'outputs':[{'name':name,'consumers':consumers.get(name,[])} for name in n.output],'attributes':attrs})
 shape_seeds=[i for i,n in enumerate(g.node) if n.domain in ['', 'ai.onnx'] and n.op_type in ['Shape','Size']]
 candidates=[{'node_index':i,'operator':n.op_type,'direct_shape_size_inputs':[name for name in n.input if producers.get(name) in shape_seeds]} for i,n in enumerate(g.node) if n.op_type in ['Shape','Size','Gather','Cast','Add']]
 return {'scope':scope,'name':g.name,'node_count':len(nodes),'operator_counts':dict(sorted(collections.Counter((n.domain+'::'+n.op_type) for n in g.node).items())),'inputs':[declared(v) for v in g.input],'outputs':[declared(v) for v in g.output],'declared_intermediates':[declared(v) for v in g.value_info],'initializers':[tensor(t) for t in g.initializer],'sparse_initializer_count':len(g.sparse_initializer),'nodes':nodes,'shape_size_seed_nodes':shape_seeds,'candidate_observations':candidates,'subgraphs':subgraphs}
inventory=inspect(graph.graph,'main')
assert model.read_bytes()==raw and stat(model)==before and stat(external)==external_before
out={'schema':'airk.private-static-onnx-inventory/1','request':{'path':str(request),'sha256':sha(requestraw)},'model':{'path':str(model),'sha256':digest,'stat_before_after':before},'external_weights':{'path':str(external),'stat_before_after':external_before,'contents_read':False},'onnx_version':onnx.__version__,'onnx_module':onnx.__file__,'ir_version':graph.ir_version,'local_function_count':len(graph.functions),'opsets':[{'domain':o.domain,'version':o.version} for o in graph.opset_import],'graph':inventory,'elapsed_seconds':time.monotonic()-started,'limits':{'max_report_bytes':16777216},'interpretation':'Static declarations and producer/consumer adjacency only. Shape/Size are candidate metadata sources; Gather/Cast/Add names or direct adjacency do not prove shape-only work. Graph dataflow, values, all consumers and actual optimized-node placement remain required. No runtime placement, model session, shape inference or external weights load.','generator_sha256':sha(Path(__file__).read_bytes())}
encoded=json.dumps(out,indent=2).encode()+b'\n';assert len(encoded)<=16777216
path=E/'gpu-compact17-static-graph-inventory-r1.json'
with path.open('xb') as f:f.write(encoded)
print(json.dumps({'report':str(path),'sha256':sha(encoded),'bytes':len(encoded),'model_sha256':digest,'nodes':inventory['node_count'],'declared_intermediates':len(inventory['declared_intermediates']),'operator_counts':inventory['operator_counts'],'elapsed_seconds':out['elapsed_seconds']}))
