# SPDX-License-Identifier: Apache-2.0
"""Full-half graph conversion only, after owning FP32/G5 admission.
No native profile emission, model inference, device or qualification authority.
The existing owning runner supplies one absolute deadline and process custody.
"""
import argparse,hashlib,importlib.util,json,pathlib,sys,time

def digest(raw):return hashlib.sha256(raw).hexdigest()
def main():
 p=argparse.ArgumentParser(description=__doc__)
 p.add_argument('--model',choices=['focus','compact','large'],required=True)
 p.add_argument('--source',type=pathlib.Path,required=True)
 p.add_argument('--source-pins',type=pathlib.Path,required=True)
 p.add_argument('--source-pins-sha256',required=True)
 p.add_argument('--python-site',type=pathlib.Path,required=True)
 p.add_argument('--output',type=pathlib.Path,required=True)
 p.add_argument('--deadline-monotonic-ns',type=int,required=True)
 args=p.parse_args();deadline=args.deadline_monotonic_ns/1_000_000_000
 if time.monotonic()>=deadline:raise ValueError('original export deadline expired')
 raw=args.source_pins.read_bytes()
 if digest(raw)!=args.source_pins_sha256:raise ValueError('original closure pins differ')
 # Third-party imports are explicit and remain graph/array tools, never Torch/ORT.
 sys.path.insert(0,str(args.python_site.resolve()));import onnx,numpy
 spec=importlib.util.spec_from_file_location('airk_fp16_export',pathlib.Path(__file__).with_name('arbiter-fp16-export.py'))
 module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
 result=module.export_after_owner_admission(args.source,args.model,json.loads(raw),args.output,onnx,numpy,deadline)
 graph=args.output/'model.onnx';candidate=onnx.load_model(str(graph),load_external_data=False)
 nodes=[{'name':n.name,'operator':n.op_type,'domain':n.domain} for n in candidate.graph.node if n.op_type in ('GroupQueryAttention','MultiHeadAttention')]
 expected=28 if args.model!='large' else 36
 if len(nodes)!=expected or time.monotonic()>=deadline:raise ValueError('actual attention roster/deadline differs')
 # The roster comes from ACTUAL output, never from a predeclared proof of dispatch.
 result.update({'attention_nodes':nodes,'attention_roster_sha256':digest(json.dumps(nodes,sort_keys=True,separators=(',',':')).encode()),'toolchain':{'python':sys.version,'onnx':onnx.__version__,'numpy':numpy.__version__},'model_inference':False,'qualification':False,'original_deadline_monotonic_ns':args.deadline_monotonic_ns})
 result['files']=[]
 for path in [graph,args.output/'weights.bin']:
  hasher=hashlib.sha256()
  with path.open('rb') as handle:
   while chunk:=handle.read(4*1024*1024):
    if time.monotonic()>=deadline:raise ValueError('original export deadline expired')
    hasher.update(chunk)
  result['files'].append({'name':path.name,'bytes':path.stat().st_size,'sha256':hasher.hexdigest()})
 if time.monotonic()>=deadline:raise ValueError('original export deadline expired')
 with (args.output/'export-manifest.private.json').open('x',encoding='utf-8') as handle:json.dump(result,handle,indent=2);handle.write('\n')
 print(json.dumps({'output':str(args.output),'files':result['files'],'attention_roster_sha256':result['attention_roster_sha256'],'qualification':False},indent=2))
if __name__=='__main__':main()
