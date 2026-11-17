# SPDX-License-Identifier: Apache-2.0
"""Root-reviewed zero-NN graph regeneration, one owned process; never compiles."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import subprocess
import time
import types

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
PACKET = HERE / 'prepared-packet-r2.json'

def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def require(path, digest):
    path = Path(path)
    state = path.stat(follow_symlinks=False)
    if (not path.is_file() or path.is_symlink() or state.st_nlink != 1
            or getattr(state, 'st_file_attributes', 0) & 0x400 or sha(path) != digest):
        raise ValueError('reviewed source/artifact identity refused')

def natural(result):
    return (result.get('exit_code') == 0 and result.get('timed_out') is False
        and result.get('log_oversized') is False and result.get('owned_descendants_settled') is True
        and result.get('owned_job_forced_termination') is False
        and result.get('owned_job_final_active_processes') == 0
        and result.get('owned_job_final_member_pids') == [] and result.get('owned_total_processes') == 1
        and not result.get('unexpected_third_process') and not result.get('cleanup_refusal'))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--packet-sha256', required=True)
    parser.add_argument('--reviewed-runner-sha256', required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--binary-sha256', required=True)
    parser.add_argument('--compile-receipt', type=Path, required=True)
    parser.add_argument('--compile-receipt-sha256', required=True)
    parser.add_argument('--artifact-lock', type=Path, required=True,
                        help='Root-retained actual compiler dependency/dep-info/rlib/image closure')
    parser.add_argument('--artifact-lock-sha256', required=True)
    parser.add_argument('--execute-reviewed', action='store_true')
    args = parser.parse_args()
    if os.name != 'nt': raise ValueError('reviewed Windows owned-process stage only')
    require(PACKET,args.packet_sha256)
    packet=json.loads(PACKET.read_bytes())
    pins={Path(path):digest for path,digest in packet['source_and_small_input_seals'].items()}
    pins.update({Path(__file__).resolve():args.reviewed_runner_sha256,PACKET:args.packet_sha256,
        args.binary:args.binary_sha256,args.compile_receipt:args.compile_receipt_sha256,
        args.artifact_lock:args.artifact_lock_sha256})
    require(args.artifact_lock,args.artifact_lock_sha256)
    lock=json.loads(args.artifact_lock.read_bytes())
    if lock['binary']!={'path':str(args.binary),'sha256':args.binary_sha256}:
        raise ValueError('actual retained compiler image differs')
    if not lock['artifacts'] or not lock['kernel_depinfo'] or not lock['retrieval_depinfo']:
        raise ValueError('actual compiler transitive closure required, not binary path alone')
    for artifact in [*lock['artifacts'],lock['kernel_depinfo'],lock['retrieval_depinfo']]:
        pins[Path(artifact['path'])]=artifact['sha256']
    for path,digest in pins.items(): require(path,digest)
    for field,required in [('kernel_depinfo','crates/airk-kernel/src/inference_owner/evaluation_tests/ann_self/regenerate.rs'),
                           ('retrieval_depinfo','crates/airk-retrieval/src/vector/mutable/self_probe/regeneration_cache.rs')]:
        if required not in Path(lock[field]['path']).read_text().replace('\\','/'):
            raise ValueError('owning compiler did not consume the mounted helper')
    compiled=json.loads(args.compile_receipt.read_bytes())
    if compiled['exit_code']!=0: raise ValueError('compile receipt is not successful')
    native=compiled['groups']['native']['files']
    for path,digest in pins.items():
        if path.suffix=='.rs' and path.is_relative_to(ROOT/'crates'):
            if native.get(path.relative_to(ROOT).as_posix())!=digest:
                raise ValueError('actual compile source snapshot differs')
    selector=packet['selector']
    argv=[str(args.binary),'--exact',selector,'--ignored','--nocapture','--test-threads=1']
    listed=subprocess.run([str(args.binary),'--exact',selector,'--ignored','--list'],
                          cwd=ROOT,capture_output=True,text=True,timeout=10)
    if listed.returncode!=0 or listed.stdout.count(selector+': test')!=1:
        raise ValueError('expected exactly one actual retained native selector')
    if not args.execute_reviewed:
        print(json.dumps({'preflight_passed':True,'pins':len(pins),'selected':1,'native_stage_started':False}));return
    anchor=Path(packet['environment']['AIRK_ANN_REGENERATE_ANCHOR'])
    control=Path(packet['environment']['AIRK_ANN_REGENERATE_CONTROL'])
    if not anchor.is_dir() or not control.is_dir() or {p.name for p in anchor.iterdir()}!={'control'}:
        raise ValueError('parent-prepared exact private anchor/control required')
    output=Path(packet['environment']['AIRK_ANN_REGENERATE_OUTPUT'])
    if output.exists(): raise ValueError('never replay or overwrite a prior graph regeneration')
    owner_path=ROOT/'tools/retrieval-train/training_execution.py'
    owner_raw=owner_path.read_bytes()
    if hashlib.sha256(owner_raw).hexdigest()!=pins[owner_path]: raise ValueError('owner source drift')
    owner=types.ModuleType('_owned_zero_nn_regeneration');owner.__file__=str(owner_path)
    exec(compile(owner_raw,str(owner_path),'exec'),owner.__dict__)
    work=HERE/'execution-r1'
    if work.exists(): raise ValueError('retain previous attempt; no overwrite or hidden retry')
    work.mkdir();(work/'stdin.txt').write_bytes(b'')
    environment=dict(os.environ);environment.update(packet['environment'])
    before={str(path):digest for path,digest in pins.items()}
    packet_bound={'preparation':packet,'argv':argv,'binary_sha256':args.binary_sha256,
        'compile_receipt_sha256':args.compile_receipt_sha256,'artifact_lock_sha256':args.artifact_lock_sha256,
        'actual_selector_list':{'argv':listed.args,'stdout':listed.stdout,'stderr':listed.stderr,'exit_code':listed.returncode},
        'pre_input_seals':before,'limits':packet['limits']}
    (work/'execution-packet.json').write_text(json.dumps(packet_bound,indent=2)+'\n',encoding='utf-8')
    def suspended(api,job,info,deadline):
        members,active,total=owner.job_snapshot(api,job)
        if members!=[int(info.pid)] or active!=1 or total!=1:
            raise ValueError('single exact suspended native image required')
        image=owner.image_proof(api,info.process,{'path':str(args.binary),'sha256':args.binary_sha256})
        if time.monotonic()>=deadline: raise ValueError('original deadline expired before resume')
        return {'members':members,'active':active,'total':total,'image':image}
    started=time.monotonic()
    with (work/'native.log').open('xb') as log,(work/'stdin.txt').open('rb') as stdin:
        owned=owner.run_windows_owned_training(argv,ROOT,log,120,environment,stdin,2147483648,before_resume=suspended)
    drift=[str(path) for path,digest in pins.items() if sha(path)!=digest]
    result={'owned':owned,'elapsed_seconds':time.monotonic()-started,'post_input_drift':drift,
            'complete':False,'quality_or_release_qualified':False}
    receipt=output/'receipt.json'
    if receipt.is_file():
        native=json.loads(receipt.read_bytes());result['native_receipt_sha256']=sha(receipt)
        result['native_complete']=native.get('complete') is True
        actual=native['result']
        result['complete']=(natural(owned) and not drift and native['complete'] is True
            and actual['embedding_actual_forwards']==0 and actual['embedding_inferences']==0
            and actual['vectors']==378 and actual['sources']==366
            and actual['exact_original_vector_bits_and_order'] is True)
    (work/'execution.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(result))
    if not result['complete']: raise SystemExit(1)

if __name__=='__main__':main()
