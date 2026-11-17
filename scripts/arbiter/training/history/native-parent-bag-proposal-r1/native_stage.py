# SPDX-License-Identifier: Apache-2.0
"""Retain/execute one Root-selected native zero-forward image; never builds."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import time
import types
ROOT=Path(__file__).resolve().parents[4]
HERE=Path(__file__).resolve().parent
MODELS=ROOT/'artifacts/advanced-split/models'
SELECTOR='inference_owner::evaluation_tests::train_units::emit_actual_native_train_units_and_parent_coverage'
def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def census(base):
    result={}
    for directory,dirs,files in os.walk(base,followlinks=False):
        for name in dirs+files:
            path=Path(directory)/name;m=path.stat(follow_symlinks=False)
            if path.is_symlink() or getattr(m,'st_file_attributes',0)&0x400:raise ValueError('source alias/reparse refused')
        for name in files:
            path=Path(directory)/name;result[path.relative_to(base.parent).as_posix()]=sha(path)
    return result
def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('operation',choices=['retain','execute'])
    p.add_argument('--binary',type=Path,required=True);p.add_argument('--binary-sha256',required=True)
    p.add_argument('--dep-info',type=Path,required=True);p.add_argument('--dep-info-sha256',required=True)
    p.add_argument('--native-source-sha256',required=True);p.add_argument('--source-group',required=True)
    p.add_argument('--retained-dir',type=Path,required=True)
    args=p.parse_args()
    source=ROOT/'crates/airk-kernel/src/inference_owner/evaluation_tests/train_units.rs'
    if sha(source)!=args.native_source_sha256 or sha(args.binary)!=args.binary_sha256 or sha(args.dep_info)!=args.dep_info_sha256:raise ValueError('Root-selected native image/source identity changed')
    if args.dep_info.resolve()!=args.binary.with_suffix('.d').resolve():raise ValueError('dep-info is not the selected Kernel image sibling')
    if 'inference_owner/evaluation_tests/train_units.rs' not in args.dep_info.read_text().replace('\\','/'):raise ValueError('actual rustc.d does not consume train_units')
    retained=args.retained_dir.resolve();target=retained/'airk-kernel.exe'
    if not retained.is_relative_to(ROOT/'artifacts/advanced-split/native-harnesses'):raise ValueError('fresh private native retention root required')
    if args.operation=='retain':
        if retained.exists():raise ValueError('never overwrite retained native image')
        observed=subprocess.run([str(args.binary),'--exact',SELECTOR,'--ignored','--list'],capture_output=True,text=True,timeout=15,check=True)
        if [line for line in observed.stdout.splitlines() if line.endswith(': test')]!=[SELECTOR+': test']:raise ValueError('exact ignored selector did not select one test')
        before=args.binary.stat();retained.mkdir(parents=True);shutil.copyfile(args.binary,target)
        if sha(args.binary)!=args.binary_sha256 or sha(target)!=args.binary_sha256:raise ValueError('native image drift during retention')
        proof={'schema':'airk.private-native-TRAIN-unit-retention/1','source_binary':str(args.binary.resolve()),'retained_binary':str(target),'binary_sha256':args.binary_sha256,'bytes':before.st_size,'mtime_ns':before.st_mtime_ns,'dep_info':str(args.dep_info.resolve()),'dep_info_sha256':args.dep_info_sha256,'native_source_sha256':args.native_source_sha256,'native_source_group':args.source_group,'selector':SELECTOR,'list_stdout':observed.stdout,'qualification':'Clippy/cheap-test identities supplied by Root; no unit/model execution'}
        (retained/'retention.json').write_text(json.dumps(proof,indent=2)+'\n');print(json.dumps(proof),flush=True);return
    if not target.exists() or sha(target)!=args.binary_sha256:raise ValueError('exact retained image absent/changed')
    proof=json.loads((retained/'retention.json').read_bytes())
    if proof['native_source_group']!=args.source_group or proof['native_source_sha256']!=args.native_source_sha256 or proof['dep_info_sha256']!=args.dep_info_sha256:raise ValueError('retained Root source/dep-info binding differs')
    manifest=MODELS/'native-parent-bag-source-r1/manifest.private.json'
    master=MODELS/'native-encoder-qualification-input-r1.json'
    pins={manifest:'4eefba97d8212eec6fea34abe880180110904513c7efb8099e700a637dd20cae',master:'3ead240726ebdba10577db6778bffbf5232eab10c50f2ae128d589e0a961ab81',target:args.binary_sha256,source:args.native_source_sha256}
    for path,pin in pins.items():
        if sha(path)!=pin:raise ValueError('native stage input changed')
    roster=json.loads(manifest.read_bytes());expected={r['file']:r['sha256'] for r in roster['files']}
    if len(expected)!=54 or len(roster['paragraphs'])!=1789 or census(manifest.parent/'sources')!=expected:raise ValueError('closed actual TRAIN source roster differs')
    for row in roster['files']:pins[manifest.parent/row['file']]=row['sha256']
    # This exact native owner opens the original mixed control only for its
    # unchanged tokenizer/planner. It does not open the failed new candidate.
    row=next(r for r in json.loads(master.read_bytes()) if r['family']=='original-mixed-u8-ort130-evaluation')
    folder=Path(row['directory']);model=folder/'model.json';pins[model]=row['model_json_sha256'];metadata=json.loads(model.read_bytes())
    for name,key in [('model.onnx','model_sha256'),('tokenizer.json','tokenizer_sha256'),('LICENSE','license_sha256')]:pins[folder/name]=metadata[key]
    owner_path=ROOT/'tools/retrieval-train/training_execution.py';owner_raw=owner_path.read_bytes()
    if hashlib.sha256(owner_raw).hexdigest()!='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975':raise ValueError('qualified suspended owner changed')
    pins[owner_path]='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975'
    pins[Path(__file__).resolve()]=sha(Path(__file__))
    for path,pin in pins.items():
        if sha(path)!=pin:raise ValueError('actual model/source asset drift before native stage')
    owner=types.ModuleType('_verified_native_owner');owner.__file__=str(owner_path);exec(compile(owner_raw,str(owner_path),'exec'),owner.__dict__)
    work=HERE/'native-unit-execution-r1';output=MODELS/'native-parent-bag-units-r1'
    if work.exists() or output.exists():raise ValueError('fresh owned output required')
    work.mkdir();output.mkdir();(work/'stdin.txt').write_bytes(b'')
    environment=dict(os.environ);environment.update({'AIRK_NATIVE_TRAIN_SOURCE_MANIFEST':str(manifest),'AIRK_NATIVE_TRAIN_SOURCE_MANIFEST_SHA256':pins[manifest],'AIRK_NATIVE_TRAIN_UNIT_DATA':str(output/'data'),'AIRK_NATIVE_TRAIN_UNIT_OUTPUT':str(output/'units.private.json'),'AIRK_NATIVE_ENCODER_INPUT':str(master),'OMP_NUM_THREADS':'1','MKL_NUM_THREADS':'1','OPENBLAS_NUM_THREADS':'1'})
    argv=[str(target),'--exact',SELECTOR,'--ignored','--nocapture','--test-threads=1'];started=time.monotonic()
    def suspended(api,job,info,deadline):
        members,active,total=owner.job_snapshot(api,job)
        if members!=[int(info.pid)] or active!=1 or total!=1:raise ValueError('native interpreter not alone while suspended')
        image=owner.image_proof(api,info.process,{'path':str(target),'sha256':args.binary_sha256})
        if time.monotonic()>=deadline:raise ValueError('original native prep deadline expired before resume')
        return {'members':members,'active':active,'total':total,'image':image}
    with (work/'stdout.log').open('wb') as log,(work/'stdin.txt').open('rb') as stdin:
        result=owner.run_windows_owned_training(argv,ROOT,log,60,environment,stdin,1073741824,before_resume=suspended)
    drift=[str(path) for path,pin in pins.items() if sha(path)!=pin]
    if census(manifest.parent/'sources')!=expected:drift.append('TRAIN source file census')
    receipt={'argv':argv,'native_source_group':args.source_group,'input_pins':{str(k):v for k,v in pins.items()},'environment':{k:v for k,v in environment.items() if k.startswith('AIRK_NATIVE_') or k.endswith('NUM_THREADS')},'owned':result,'post_input_drift':drift,'elapsed_seconds':time.monotonic()-started,'limits':{'whole_owned_seconds':60,'aggregate_owned_host_bytes':1073741824,'max_job_slots':2,'expected_processes':1},'model_family':row['family'],'purpose':'Native model initialization/tokenization only; no embedding forwards or gradients'}
    (work/'execution.json').write_text(json.dumps(receipt,indent=2)+'\n')
    if result['exit_code'] or result['timed_out'] or result['owned_job_forced_termination'] or not result['owned_descendants_settled'] or result['owned_total_processes']!=1 or drift:raise ValueError('native stage failed; retain diagnostics/no automatic retry')
    data=json.loads((output/'units.private.json').read_bytes())
    if data.get('complete') is not True or data['embedding_forwards']!=0 or data['training_gradients'] is not False or len(data['parents'])!=1789:raise ValueError('native output incomplete or unexpected forwards')
    print(json.dumps({'units_output_sha256':sha(output/'units.private.json'),'units':len(data['units']),'parents':len(data['parents']),'owned':result,'post_input_drift':drift}),flush=True)
if __name__=='__main__':main()
