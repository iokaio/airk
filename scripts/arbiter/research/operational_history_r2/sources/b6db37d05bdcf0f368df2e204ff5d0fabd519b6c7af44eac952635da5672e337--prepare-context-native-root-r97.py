# SPDX-License-Identifier: Apache-2.0
"""Prepare only the Root-approved LargeCPU16 R97 native packet and preserve its actual images."""
import hashlib,json,os,struct,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
BASE=ROOT/'artifacts/advanced-split/models/qwen3-challengers-r1/context-native-owned-r2'
BUILD=ROOT/'artifacts/advanced-split/receipts/stage-integration-r97'
FOCUS=ROOT/'artifacts/advanced-split/focus-native-qualification-preparation-r1/native-parity-packet-released-r2.json'
FOCUS_SHA='c817dee774520f1dcf3bfa549da1a57a4ea644565993e1e44d7229b77cdc77a6'
PARENT=ROOT/'artifacts/advanced-split/models/qwen3-challengers-r1/context-native-owned-r1/build-target-evidence-r1/target-evidence.json'
PARENT_SHA='ea6c1d82d4c7bcd84e688415f42579692d2435c553dab0bdf68731f13044b2f1'
TARGET='x86_64-pc-windows-msvc'
def sha(path):
 with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def artifact(path,name):
 rows=[]
 for line in path.read_text(encoding='utf-8').splitlines():
  try:row=json.loads(line)
  except json.JSONDecodeError:continue
  if row.get('reason')=='compiler-artifact' and row.get('target',{}).get('name')==name and row.get('profile',{}).get('test') is True and row.get('executable'):rows.append(row)
 if len(rows)!=1:raise ValueError('Exactly one actual R97 Cargo test artifact required')
 return rows[0]
def pe(path):
 with Path(path).open('rb') as f:
  h=f.read(64)
  if h[:2]!=b'MZ':raise ValueError('Actual image lacks DOS header')
  f.seek(struct.unpack_from('<I',h,60)[0]);h=f.read(26)
  if h[:4]!=b'PE\0\0' or struct.unpack_from('<H',h,4)[0]!=0x8664 or struct.unpack_from('<H',h,24)[0]!=0x20b:raise ValueError('Exact current AMD64 PE32+ image required')
 return {'machine_u16':0x8664,'machine_hex':'0x8664','optional_header_magic_u16':0x20b,'optional_header_magic_hex':'0x020b','recomputed_from_actual_preserved_bytes':True}
def main():
 if Path.cwd().resolve()!=ROOT:raise ValueError('Run from owning repository root')
 if sha(FOCUS)!=FOCUS_SHA or sha(PARENT)!=PARENT_SHA:raise ValueError('Original reviewed Focus runtime/target evidence changed')
 parent=json.loads(FOCUS.read_bytes());runtime={path:pin for path,pin in parent['pins'].items() if not Path(path).is_relative_to(ROOT)}
 if len(runtime)!=19176:raise ValueError('Exact original successful runtime19176 closure required')
 clippy_path=BUILD/'clippy.json';clippy=json.loads(clippy_path.read_bytes());source=clippy['groups']['native']['sha256']
 if clippy['exit_code']!=0 or clippy['inputs_unchanged'] is not True:raise ValueError('Actual R97 unchanged full Clippy required')
 master=BASE/'selected-context-4-profile-r2/selected-profiles.private.json';profile_receipt=BUILD/'context-profile-4-cpu16.json';factory=json.loads(profile_receipt.read_bytes())
 if factory['exit_code']!=0 or factory['inputs_unchanged'] is not True or factory['groups']['native']['sha256']!=source:raise ValueError('Actual same-source R97 CPU16 Rust factory proof required')
 archive=BASE/'build-target-evidence-r97'
 if archive.exists():raise ValueError('Fresh image archive required')
 archive.mkdir();snapshots=[]
 def snapshot(path,name):
  path=Path(path).resolve();destination=archive/name
  if path.is_symlink() or not path.is_file() or destination.exists():raise ValueError('Fresh regular independent image/evidence copy required')
  before=sha(path);count=0;h=hashlib.sha256()
  with path.open('rb') as f,destination.open('xb') as out:
   while block:=f.read(4<<20):out.write(block);h.update(block);count+=len(block)
  if h.hexdigest()!=before or sha(destination)!=before or sha(path)!=before or count!=path.stat().st_size:raise ValueError('Current image/evidence changed during materialization')
  row={'path':str(destination),'sha256':before,'bytes':count,'original_path':str(path),'independent_copy':True};snapshots.append(row);return row
 image_rows=[]
 for name in ['airk-inference','airk-kernel']:
  path=BUILD/(name+'-image.log');receipt_path=path.with_suffix('.json');receipt=json.loads(receipt_path.read_bytes());row=artifact(path,name.replace('-','_'))
  if receipt['exit_code']!=0 or receipt['inputs_unchanged'] is not True or receipt['groups']['native']['sha256']!=source or '--target' in receipt['command'] or any(str(x).startswith('--target=') for x in receipt['command']):raise ValueError('Actual same-source unchanged host/default R97 image build required')
  if Path(row['executable']).parent!=Path(receipt['target_directory'])/'debug/deps':raise ValueError('Actual image outside recorded default-target directory')
  cargo=snapshot(path,name+'-cargo.jsonl');build=snapshot(receipt_path,name+'-build-receipt.json');materialized=snapshot(Path(row['executable']),name+'-r97.exe')
  image_rows.append({'image_sha256':materialized['sha256'],'target':TARGET,'artifact_kind':name.replace('-','_'),'evidence_kind':'materialized_current_PE_and_recorded_build','materialized_image':materialized,'pe':pe(Path(materialized['path'])),'cargo_json':cargo,'cargo_selector':{'reason':'compiler-artifact','target_name':name.replace('-','_'),'profile_test':True,'executable':row['executable']},'actual_cargo_artifact':row,'build_receipt':build,'native_source_group':source,'build_command_has_explicit_target':False})
 snapshot(clippy_path,'clippy-receipt.json');snapshot(profile_receipt,'context-profile-4-receipt.json');snapshot(master,'rust-profile-master.json');cache=snapshot(ROOT/'artifacts/v190/p10-target/.rustc_info.json','target-rustc-info.json');cached=json.loads(Path(cache['path']).read_bytes());outputs=[x.get('stdout','') for x in cached['outputs'].values() if x.get('success') is True]
 if not any('host: '+TARGET in x for x in outputs) or not any(all(k in x for k in ['target_env="msvc"','target_arch="x86_64"','target_os="windows"']) for x in outputs):raise ValueError('Actual current cached rustc target differs')
 evidence={'schema':'airk.private-native-build-target-evidence/1','complete':True,'target':TARGET,'images':image_rows,'target_basis':{'retained_original_target_evidence':{'path':str(PARENT),'sha256':PARENT_SHA},'current_cached_rustc_target':cache,'no_explicit_cross_target_argument_in_actual_build_commands':True},'source_snapshots':snapshots,'script_sha256':sha(__file__),'NN_executed':False,'Cargo_executed':False,'scope':'Additive actual R97 image/materialization/target proof only; original R95 and historical Focus target evidence remain under original identities. No new qualification inferred.'};target_receipt=archive/'target-evidence.json';target_receipt.write_text(json.dumps(evidence,indent=2)+'\n')
 for path in archive.iterdir():
  if path.is_file():os.chmod(path,0o444)
 cargo=BUILD/'airk-inference-image.log';proposal=BASE/'native-packet-4-proposed-root-r97.json';released=BASE/'native-packet-4-released-root-r97.json'
 if proposal.exists() or released.exists():raise ValueError('Fresh R97 proposed/released packet paths required')
 subprocess.run([sys.executable,'-B',str(BASE/'prepare_native.py'),'--master',str(master),'--master-sha256',sha(master),'--cargo-json',str(cargo),'--cargo-json-sha256',sha(cargo),'--output-packet',str(proposal)],check=True)
 packet=json.loads(proposal.read_bytes())
 if packet['model']!='arbiter-context-qwen3-4b' or packet['cpu_fit']!={'minimum_physical_cores':16,'intra_threads':16,'inter_threads':1,'sequential':True,'spinning':False}:raise ValueError('Only actual corrected LargeCPU16 candidate is releasable')
 packet['pins'].update(runtime);packet['pins'][str(target_receipt)]=sha(target_receipt);packet['pins'][str(Path(__file__).resolve())]=sha(__file__);packet['pins'][str(clippy_path)]=sha(clippy_path);packet['pins'][str(profile_receipt)]=sha(profile_receipt)
 packet['status']='ROOT_RELEASED';packet['root_review']='Root approved one new LargeCPU16 minimum16physical hardware requirement candidate after actual CPU8 first-call30s failure. Same exact FP32 model/assets/math and <=.001 gates;40GiB reservation/48GiB1800s job/20measurements/original30s deadline and first-failure stop unchanged. Not a CPU8 pass or model/precision/CPU tournament. No quality/CAL/composed/platform-release claim.'
 packet['provenance']={'native_source_receipt':str(cargo.with_suffix('.json')),'native_source_group':source,'runtime_base_rows':len(runtime),'runtime_parent_packet_sha256':FOCUS_SHA,'target_evidence':{'path':str(target_receipt),'sha256':sha(target_receipt)},'component_only':True};released.write_text(json.dumps(packet,indent=2)+'\n')
 print(json.dumps({'released_packet':str(released),'sha256':sha(released),'base_runtime_rows':len(runtime),'target_evidence':str(target_receipt),'target_evidence_sha256':sha(target_receipt),'images':[{'image_sha256':x['image_sha256'],'target':x['target']} for x in image_rows],'launch_command':[sys.executable,'-B',str(BASE/'run_native_context.py'),'--packet',str(released),'--packet-sha256',sha(released),'--execute-reviewed'],'NN_executed':False,'Cargo_executed':False}),flush=True)
if __name__=='__main__':main()
