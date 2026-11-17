# SPDX-License-Identifier: Apache-2.0
"""Inventory request for the actual Rust Profile factory; never emits a Profile."""
import argparse,hashlib,json
from pathlib import Path

def sha(path):
 with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def main():
 p=argparse.ArgumentParser();p.add_argument('--storage-receipt',type=Path,required=True);p.add_argument('--storage-sha256',required=True);p.add_argument('--readout-receipt',type=Path,required=True);p.add_argument('--readout-sha256',required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 if a.output.exists() or sha(a.storage_receipt)!=a.storage_sha256 or sha(a.readout_receipt)!=a.readout_sha256:raise ValueError('actual producer pins and fresh request required')
 storage=json.loads(a.storage_receipt.read_bytes());readout=json.loads(a.readout_receipt.read_bytes());model=storage['model_id']
 if storage.get('complete') is not True or storage.get('stage')!='qwen3-context-fp32-storage' or storage.get('unchanged_graph_except_external_locations_ranges') is not True or storage.get('all_tensor_bytes_sha256_preserved') is not True or readout.get('complete') is not True or readout.get('stage')!='qwen3-context-fp32-readout' or readout.get('model_id')!=model:raise ValueError('matching complete exact FP32 storage/readout required')
 if model not in ['Qwen/Qwen3-1.7B','Qwen/Qwen3-4B']:raise ValueError('selected Context only')
 compact=model.endswith('1.7B');cpu=4 if compact else 16;reservation=(16 if compact else 40)*1024**3;root=Path(__file__).resolve().parents[2];publisher=root/'qwen3-challenger-assets-r1';manifest=publisher/'research-asset-pins.json'
 if sha(manifest)!='4cac4fc751d996e987bb4701fb90ce8cbd600af06152151b4ea99bed8d6c664c':raise ValueError('official asset manifest changed')
 entry=next(x for x in json.loads(manifest.read_bytes())['models'] if x['id']==model);folder=publisher/'candidate-assets'/model.replace('/','--');assets=[]
 def asset(path,name,role,pin):
  if path.is_symlink() or not path.is_file() or getattr(path.lstat(),'st_file_attributes',0)&0x400:raise ValueError('asset alias/nonfile')
  assets.append({'asset':{'name':name,'role':role,'bytes':path.stat().st_size,'sha256':pin},'path':str(path.resolve())})
 for name,role in [('model.onnx','model'),('model.onnx.data','external_weights')]:
  path=a.storage_receipt.parent/'model'/name;item=next(x for x in storage['graph_closure']['files'] if x['name']==name)
  if path.stat().st_size!=item['bytes'] or storage['artifact_sha256']['model/'+name]!=item['sha256']:raise ValueError('complete packed asset identity differs')
  asset(path,name,role,item['sha256'])
 for name,role in [('tokenizer.json','tokenizer'),('LICENSE','notice')]:
  publisher_pin=next(x for x in entry['assets'] if x['name']==name);path=folder/name;raw=path.read_bytes();pin=hashlib.sha256(raw).hexdigest()
  if len(raw)!=publisher_pin['bytes'] or (publisher_pin['sha256'] is not None and pin!=publisher_pin['sha256']) or (publisher_pin['sha256'] is None and hashlib.sha1(('blob %d\0'%len(raw)).encode()+raw).hexdigest()!=publisher_pin['git_blob']):raise ValueError('original public tokenizer/license changed')
  asset(path,name,role,pin)
 fixture=a.readout_receipt.parent/'numeric-fixture.json';pin=readout['artifact_sha256'][fixture.name]
 if sha(fixture)!=pin:raise ValueError('actual independent numeric fixture changed')
 asset(fixture,fixture.name,'fixture',pin)
 request={'model':'arbiter-context-qwen3-17b' if compact else 'arbiter-context-qwen3-4b','profile_id':'private-context-qwen3-'+('17b' if compact else '4b')+'-fp32-cpu16-r2','assets':assets,'cpu_fit':{'minimum_physical_cores':cpu,'intra_threads':cpu,'inter_threads':1,'sequential':True,'spinning':False},'memory_reservation_bytes':reservation,'peak_disk_bytes':sum(x['asset']['bytes'] for x in assets)+2*65536+4*131072+16*1024**2,'maximum_candidates':8,'calibration':None}
 a.output.write_text(json.dumps([request],indent=2)+'\n');print(json.dumps({'native_request':str(a.output),'sha256':sha(a.output),'profiles_emitted':0,'NN_executed':False,'scope':'Actual Rust factory authenticates all asset bytes and alone constructs Core Profile/compiler/raw-only calibration. This request is not qualified/distributable metadata.'}))
if __name__=='__main__':main()
