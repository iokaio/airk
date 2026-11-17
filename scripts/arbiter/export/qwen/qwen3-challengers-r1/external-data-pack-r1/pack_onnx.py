# SPDX-License-Identifier: Apache-2.0
"""Bounded byte relocation only: one ONNX model and one external weight blob."""
import argparse,hashlib,json,math,os,shutil,time
from pathlib import Path
CHUNK=4*1024*1024
MAX_DATA=16*1024**3

def sha(path):
 h=hashlib.sha256()
 with Path(path).open('rb') as stream:
  while block:=stream.read(CHUNK):h.update(block)
 return h.hexdigest()

def regular(path):
 p=Path(path)
 if p.is_symlink() or not p.is_file() or getattr(p.lstat(),'st_file_attributes',0)&0x400:raise ValueError('alias or nonregular data file')
 return p

def source_range(root,location,offset,length):
 if type(offset) is not int or type(length) is not int or offset<0 or length<=0:raise ValueError('invalid external byte range')
 root=Path(root).resolve();candidate=(root/location).resolve()
 if candidate.parent!=root or candidate.name in ['.','..'] or candidate==root:raise ValueError('external data escapes flat authenticated producer directory')
 path=regular(candidate)
 if offset+length>path.stat().st_size:raise ValueError('external byte range exceeds authenticated data')
 return path

def pack_ranges(root,specifications,destination):
 destination=Path(destination)
 if destination.exists():raise ValueError('fresh packed data file required')
 planned=[]
 for spec in specifications:
  path=source_range(root,spec['location'],spec['offset'],spec['length']);planned.append((spec,path))
 rows=[];cache={}
 with destination.open('xb') as output:
  for spec,path in planned:
   key=(str(path),spec['offset'],spec['length'])
   if key in cache:
    rows.append(dict(cache[key],name=spec['name']));continue
   offset=output.tell();padding=(-offset)%64
   if offset+padding+spec['length']>MAX_DATA:raise ValueError('packed blob exceeds existing native16GiB asset limit')
   output.write(b'\0'*padding);offset=output.tell();h=hashlib.sha256()
   with path.open('rb') as source:
    source.seek(spec['offset']);remaining=spec['length']
    while remaining:
     block=source.read(min(CHUNK,remaining))
     if not block:raise ValueError('source changed or short external data')
     output.write(block);h.update(block);remaining-=len(block)
   row={'name':spec['name'],'source_file':path.name,'source_offset':spec['offset'],'length':spec['length'],'packed_offset':offset,'tensor_bytes_sha256':h.hexdigest()};rows.append(row);cache[key]=row
 with destination.open('rb') as stream:
  for row in rows:
   stream.seek(row['packed_offset']);remaining=row['length'];h=hashlib.sha256()
   while remaining:
    block=stream.read(min(CHUNK,remaining))
    if not block:raise ValueError('packed byte range truncated')
    h.update(block);remaining-=len(block)
   if h.hexdigest()!=row['tensor_bytes_sha256']:raise ValueError('packed tensor bytes differ')
 return rows

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--source',type=Path,required=True);p.add_argument('--source-receipt-sha256',required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 started=time.monotonic();source=a.source.resolve();output=a.output.resolve();repo=Path(__file__).resolve().parents[5]
 if output.exists() or not output.is_relative_to(repo/'artifacts') or output.is_relative_to(source) or source.is_relative_to(output):raise ValueError('fresh disjoint private output required')
 receipt_path=regular(source/'receipt.json')
 if sha(receipt_path)!=a.source_receipt_sha256:raise ValueError('reviewed source export receipt changed')
 prior=json.loads(receipt_path.read_bytes())
 if prior.get('complete') is not True or prior.get('stage')!='qwen3-scores-only-export' or prior.get('model_id') not in ['Qwen/Qwen3-1.7B','Qwen/Qwen3-4B']:raise ValueError('matching complete source export required')
 pins=prior['artifact_sha256']
 for name,pin in pins.items():
  if Path(name).name!=name or sha(regular(source/name))!=pin:raise ValueError('source artifact identity changed')
 graph_path=regular(source/'model.onnx')
 if graph_path.stat().st_size>64*1024**2:raise ValueError('graph exceeds existing native64MiB model admission')
 import onnx
 from onnx.external_data_helper import uses_external_data
 graph=onnx.load(str(graph_path),load_external_data=False);original=graph.SerializeToString(deterministic=True)
 specifications=[];metadata=[]
 widths={1:4,2:1,3:1,4:2,5:2,6:4,7:8,9:1,10:2,11:8,12:4,13:8,16:2}
 names=set()
 for tensor in graph.graph.initializer:
  if not uses_external_data(tensor):continue
  if tensor.name in names:raise ValueError('duplicate initializer identity')
  names.add(tensor.name);external={entry.key:entry.value for entry in tensor.external_data}
  if len(external)!=len(tensor.external_data) or not set(external)<={'location','offset','length'} or 'location' not in external or tensor.raw_data:raise ValueError('ambiguous or unsupported external tensor metadata')
  if tensor.data_type not in widths or any(axis<0 for axis in tensor.dims):raise ValueError('unsupported external tensor type/shape')
  expected=math.prod(tensor.dims)*widths[tensor.data_type]
  offset=int(external.get('offset','0'));length=int(external.get('length',str(expected)))
  if length!=expected:raise ValueError('external length differs from exact typed tensor bytes')
  path=source_range(source,external['location'],offset,length)
  if path.name not in pins:raise ValueError('unreviewed external source artifact')
  specifications.append({'name':tensor.name,'location':external['location'],'offset':offset,'length':length});metadata.append((tensor,[(e.key,e.value) for e in tensor.external_data]))
 if not specifications:raise ValueError('full selected model has no external weights')
 # Any nested external tensor is refused by the native owner; do not conceal it.
 from onnx.external_data_helper import _get_all_tensors
 if sum(uses_external_data(t) for t in _get_all_tensors(graph))!=len(specifications):raise ValueError('nested external tensor unsupported by native owner')
 output.mkdir();rows=pack_ranges(source,specifications,output/'weights.bin')
 for (tensor,_),row in zip(metadata,rows):
  del tensor.external_data[:]
  for key,value in [('location','weights.bin'),('offset',str(row['packed_offset'])),('length',str(row['length']))]:
   entry=tensor.external_data.add();entry.key=key;entry.value=value
 packed=graph.SerializeToString(deterministic=True)
 if len(packed)>64*1024**2:raise ValueError('packed graph exceeds native model admission')
 (output/'model.onnx').write_bytes(packed)
 # Restore ONLY metadata, then compare the complete protobuf: no node, shape,
 # datatype, value, attribute, input or output alteration may pass this proof.
 for tensor,old in metadata:
  del tensor.external_data[:]
  for key,value in old:entry=tensor.external_data.add();entry.key=key;entry.value=value
 if graph.SerializeToString(deterministic=True)!=original:raise ValueError('nonrelocation graph bytes changed')
 for name in ['case-0.npz','case-1.npz','case-2.npz']:
  if name not in pins:raise ValueError('complete actual reference probes required')
  shutil.copyfile(source/name,output/name)
 for name,pin in pins.items():
  if sha(source/name)!=pin:raise ValueError('source changed during packing')
 result={'schema':'airk.private-onnx-external-byte-relocation/1','model_id':prior['model_id'],'source_receipt_sha256':a.source_receipt_sha256,'source_graph_sha256':pins['model.onnx'],'source_script_sha256':sha(__file__),'unchanged_graph_except_external_locations_ranges':True,'all_tensor_bytes_sha256_preserved':True,'external_files':1,'tensor_rows':rows,'elapsed_seconds':time.monotonic()-started,'artifact_sha256':{f.name:sha(f) for f in output.iterdir() if f.is_file()},'scope':'Byte relocation and unchanged graph structure only. No NN executed; native ORT/resource/quality/calibration/platform/publication qualification remains required.'}
 (output/'packing-receipt.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps({k:v for k,v in result.items() if k!='tensor_rows'}),flush=True)
if __name__=='__main__':main()
