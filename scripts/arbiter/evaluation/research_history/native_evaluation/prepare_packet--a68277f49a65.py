# SPDX-License-Identifier: Apache-2.0
"""Prepare one exact original S8 proposed native packet; never executes a model."""
import argparse,hashlib,json,shutil
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4];HERE=Path(__file__).resolve().parent;MODELS=HERE.parent
FOCUS=ROOT/'artifacts/advanced-split/focus-native-qualification-preparation-r1/native-parity-packet-released-r2.json'
FOCUS_SHA='c817dee774520f1dcf3bfa549da1a57a4ea644565993e1e44d7229b77cdc77a6'
def sha(path):
 with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def validate_build_proof(receipt_path,build,cargo_json):
 log=Path(build['log'])
 if not log.is_absolute():log=Path(receipt_path).resolve().parent/log
 if build.get('exit_code')!=0 or any(build.get(name) is not True for name in ['inputs_unchanged','runner_unchanged','runtime_unchanged','command_inputs_unchanged']) or log.resolve()!=Path(cargo_json).resolve():raise ValueError('actual successful unchanged matching Cargo receipt required')
def main():
 p=argparse.ArgumentParser();p.add_argument('--cargo-json',type=Path,required=True);p.add_argument('--compile-receipt',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 if a.output.exists() or a.output.parent.resolve()!=HERE.resolve() or sha(FOCUS)!=FOCUS_SHA:raise ValueError('fresh packet and original runtime closure required')
 build=json.loads(a.compile_receipt.read_bytes())
 validate_build_proof(a.compile_receipt,build,a.cargo_json)
 if build['groups']['native']['sha256']!='3a64a188c7188d44a3da18afd9ca2efa5c3e996f171d28b62bd8a3f89c9f031f':raise ValueError('exact Root-reviewed R102 native source required')
 rows=[]
 for line in a.cargo_json.read_text().splitlines():
  try:row=json.loads(line)
  except json.JSONDecodeError:continue
  if row.get('reason')=='compiler-artifact' and row.get('target',{}).get('name')=='airk_kernel' and row.get('profile',{}).get('test') is True and row.get('executable'):rows.append(row)
 if len(rows)!=1:raise ValueError('exact actual Kernel test image from Cargo JSON required')
 binary=Path(rows[0]['executable']).resolve();pins={path:pin for path,pin in json.loads(FOCUS.read_bytes())['pins'].items() if not Path(path).is_relative_to(ROOT)}
 if len(pins)!=19176:raise ValueError('exact unchanged original19176 runtime closure required')
 for name,pin in build['groups']['native']['files'].items():
  path=ROOT/name
  if pin=='missing':
   if path.exists():raise ValueError('source missing-row changed')
  else:
   if sha(path)!=pin:raise ValueError('actual compiled native source changed')
   pins[str(path.resolve())]=pin
 master=HERE/'one-family-input.json';descriptor=json.loads(master.read_bytes());model=MODELS/'minilm-shipped-baseline-ort130-r1/s8';oracle=MODELS/'minilm-original-native-inputs-r2/receipt.json'
 if len(descriptor)!=1 or descriptor[0]['family']!='original-shipped-s8-ort130-evaluation':raise ValueError('one original S8 family only')
 powershell=shutil.which('pwsh.exe')
 if not powershell:raise ValueError('actual PowerShell image required')
 paths=[master,a.cargo_json,a.compile_receipt,binary,HERE/'run_native.py',HERE/'prepare_packet.py',HERE/'anchor_runtime.py',HERE/'anchor_bridge.ps1',HERE/'test_controls.py',Path(powershell),ROOT/'tools/retrieval-train/training_execution.py',MODELS/'a1-fresh-final-materializer-r5/prepare-anchor.ps1',MODELS/'observe-qasper-final-anchor-r3.ps1',MODELS/'original-minilm-retention-decision-r1.json',oracle,*[model/name for name in ['model.json','model.onnx','tokenizer.json','LICENSE']]]
 for path in paths:pins[str(path.resolve())]=sha(path)
 if pins[str((MODELS/'original-minilm-retention-decision-r1.json').resolve())]!='cef9af82caf81a6bd755161ffb8a00943d954cf7110eeb822ba775789a4433c2':raise ValueError('exact Root retention decision changed')
 if pins[str((ROOT/'tools/retrieval-train/training_execution.py').resolve())]!='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975':raise ValueError('original reviewed process-owner algorithm changed')
 if pins[str(model/'model.onnx')]!='4278337fd0ff3c68bfb6291042cad8ab363e1d9fbc43dcb499fe91c871902474' or pins[str(model/'model.json')]!='333b6babaaae8bda229e99c533a9ad57524704566eadb902fe52f37f38c78ce5' or pins[str(oracle)]!='97ec355d47ca8b8da028fa512fa3a7127cfd66748b1a7bfe80c8c1e97725552d':raise ValueError('exact original source pins differ')
 packet={'schema':'airk.private-original-s8-native-execution/1','status':'PREPARED_FOR_ROOT_REVIEW','binary':{'path':str(binary),'sha256':sha(binary),'bytes':binary.stat().st_size},'master':str(master),'output':str(HERE/'native-result-r1'),'private_anchor_prefix':str(MODELS/'native-parent-bag-native-private-anchor-original-s8-r1'),'anchor_powershell':str(Path(powershell).resolve()),'limits':{'seconds':1800,'host_bytes':8589934592,'max_job_processes':2},'pins':pins,'compile_receipt':{'path':str(a.compile_receipt.resolve()),'sha256':sha(a.compile_receipt),'native_source_group':build['groups']['native']['sha256']},'runtime_parent_sha256':FOCUS_SHA,'scope':'One original S8 numeric-only stage, all12 exact independent masked bucket cases/serialB1, fixed samegraph1e-5 and honest original FP32 deltas. No six-model/205/DEV/FINAL or task/release/platform promotion.'};a.output.write_text(json.dumps(packet,indent=2)+'\n');print(json.dumps({'packet':str(a.output),'sha256':sha(a.output),'model_executed':False,'runtime_rows':19176}))
if __name__=='__main__':main()
