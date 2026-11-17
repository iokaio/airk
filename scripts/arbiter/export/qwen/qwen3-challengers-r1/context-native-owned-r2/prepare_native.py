# SPDX-License-Identifier: Apache-2.0
"""Build proposed native packet from actual Rust master and actual Cargo JSON image."""
import argparse,hashlib,json
from pathlib import Path
from run_native_context import ROOT,OWNER,OWNER_SHA,PROFILES,digest
HERE=Path(__file__).resolve().parent

def cargo_image(raw):
 images=set()
 for line in raw.decode('utf-8').splitlines():
  try:value=json.loads(line)
  except json.JSONDecodeError:continue
  if value.get('reason')=='compiler-artifact' and value.get('target',{}).get('name')=='airk_inference' and value.get('profile',{}).get('test') is True and value.get('executable'):images.add(value['executable'])
 if len(images)!=1:raise ValueError('Exactly one actual airk_inference test image from Cargo JSON required')
 return Path(images.pop()).resolve()
def main():
 p=argparse.ArgumentParser();p.add_argument('--master',type=Path,required=True);p.add_argument('--master-sha256',required=True);p.add_argument('--cargo-json',type=Path,required=True);p.add_argument('--cargo-json-sha256',required=True);p.add_argument('--output-packet',type=Path,required=True);a=p.parse_args()
 if a.output_packet.exists() or digest(a.master)!=a.master_sha256 or digest(a.cargo_json)!=a.cargo_json_sha256:raise ValueError('Fresh packet and exact actual Rust/Cargo provenance required')
 master=json.loads(a.master.read_bytes())
 if master.get('schema')!='airk.private.selected-native-profile-master/1' or len(master['profiles'])!=1:raise ValueError('One actual Rust-selected Profile master required')
 row=master['profiles'][0];model=row['model'];scope=PROFILES[model];profile=Path(row['profile']).resolve()
 if digest(profile)!=row['profile_sha256']:raise ValueError('Actual emitted Profile differs')
 profile_data=json.loads(profile.read_bytes())
 if profile_data['model']!=model or profile_data.get('qualification') is not None or profile_data['memory_reservation_bytes']!=scope['reservation'] or profile_data.get('qualified_targets') or profile_data.get('downloads'):raise ValueError('Exact unqualified experimental Context descriptor required')
 if digest(OWNER)!=OWNER_SHA:raise ValueError('Unchanged native process owner required')
 binary=cargo_image(a.cargo_json.read_bytes())
 if not binary.is_file() or binary.is_symlink():raise ValueError('Actual Cargo image missing or aliased')
 files=row['files'];fixture=[x for x in profile_data['assets'] if x['role']=='fixture']
 if len(fixture)!=1 or set(files)!={x['name'] for x in profile_data['assets']}:raise ValueError('Exact actual native asset filemap required')
 label='17' if model.endswith('17b') else '4';descriptor=HERE/('native-input-'+label+'.json');work=HERE/('native-'+label+'-result')
 if descriptor.exists() or work.exists():raise ValueError('Fresh model-specific native paths required')
 actual={'profile':str(profile),'profile_sha256':row['profile_sha256'],'files':files,'fixtures':files[fixture[0]['name']]};descriptor.write_text(json.dumps(actual,indent=2)+'\n')
 pins={str(a.master.resolve()):a.master_sha256,str(a.cargo_json.resolve()):a.cargo_json_sha256,str(profile):row['profile_sha256'],str(descriptor):digest(descriptor),str(OWNER):OWNER_SHA}
 for asset in profile_data['assets']:
  path=Path(files[asset['name']]).resolve()
  if path.is_symlink() or not path.is_file() or path.stat().st_size!=asset['bytes']:raise ValueError('Actual authenticated asset type/length differs')
  pins[str(path)]=asset['sha256']
 # Capture current native sources and emitted public contract fixtures while Root holds the source freeze.
 for folder in [ROOT/'crates',ROOT/'fixtures/split',ROOT/'.cargo']:
  for path in folder.rglob('*'):
   if path.is_file() and path.suffix in ['.rs','.toml','.md','.json','.jsonl','.txt'] and not path.is_symlink():pins[str(path.resolve())]=digest(path)
 for path in [ROOT/'Cargo.toml',ROOT/'Cargo.lock',ROOT/'rust-toolchain.toml',HERE/'prepare_native.py',HERE/'test_native_owner.py']:pins[str(path)]=digest(path)
 packet={'schema':'airk.private-context-native-execution/1','status':'PREPARED_FOR_ROOT_REVIEW','model':model,'binary':{'path':str(binary),'sha256':digest(binary),'bytes':binary.stat().st_size},'cargo_json_source':{'path':str(a.cargo_json.resolve()),'sha256':a.cargo_json_sha256},'runner_sha256':digest(HERE/'run_native_context.py'),'cpu_fit':profile_data['cpu_fit'],'candidate_memory_reservation_bytes':scope['reservation'],'limits':{'wall_seconds':scope['seconds'],'host_committed_bytes':scope['host']},'output':str(work),'environment':{'CUDA_VISIBLE_DEVICES':'','OMP_NUM_THREADS':'1','MKL_NUM_THREADS':'1','OPENBLAS_NUM_THREADS':'1','PYTHONDONTWRITEBYTECODE':'1','PYTHONNOUSERSITE':'1','AIRK_CONTEXT_NATIVE_INPUT':str(descriptor),'AIRK_CONTEXT_NATIVE_INPUT_SHA256':digest(descriptor)},'pins':pins,'scope':'Actual native all-live FP32 numeric/resource candidate only; separate Root release required. No CAL, combined service fit, product floor or release qualification.'}
 a.output_packet.write_text(json.dumps(packet,indent=2)+'\n');print(json.dumps({'packet':str(a.output_packet),'sha256':digest(a.output_packet),'native_image_from_actual_Cargo_JSON':str(binary),'pins':len(pins),'NN_executed':False}))
if __name__=='__main__':main()
