# SPDX-License-Identifier: Apache-2.0
"""Own exactly one stdlib source stage; suspended assignment, no automatic retry."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import time
import types
HERE=Path(__file__).resolve().parent
def sha(raw):return hashlib.sha256(raw).hexdigest()
def suspended_identity(owner,api,job,info,expected,deadline):
    members,active,total=owner.job_snapshot(api,job)
    if members!=[int(info.pid)] or active!=1 or total!=1:raise ValueError('suspended source interpreter not assigned alone')
    image=owner.image_proof(api,info.process,expected)
    if time.monotonic()>=deadline:raise ValueError('original source deadline expired before resume')
    return {'members':members,'active':active,'total':total,'pinned_base_image':image,'source_stage_executed_from_captured_verified_bytes':True}
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--review',type=Path,required=True);parser.add_argument('--sha256',required=True);args=parser.parse_args()
    raw=args.review.read_bytes()
    if sha(raw)!=args.sha256:raise ValueError('review packet changed')
    review=json.loads(raw);pins={row['path']:row['sha256'] for row in review['inputs']}
    source_root=HERE.parents[3];owner_path=source_root/'tools/retrieval-train/training_execution.py';owner_raw=owner_path.read_bytes()
    if sha(owner_raw)!='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975':raise ValueError('qualified process owner changed')
    owner=types.ModuleType('_verified_owned');owner.__file__=str(owner_path);exec(compile(owner_raw,str(owner_path),'exec'),owner.__dict__)
    stage=HERE/'source_stage.py';stage_raw=stage.read_bytes()
    if sha(stage_raw)!=pins[str(stage)]:raise ValueError('source-stage entry changed')
    runtime_path=HERE/'source_runtime.py';runtime_raw=runtime_path.read_bytes()
    if sha(runtime_raw)!=pins[str(runtime_path)]:raise ValueError('parent runtime-verifier source changed')
    runtime=types.ModuleType('_verified_parent_source_runtime');runtime.__file__=str(runtime_path);exec(compile(runtime_raw,str(runtime_path),'exec'),runtime.__dict__)
    preflight_started=time.monotonic();preflight_files=runtime.verify(review,preflight_started);preflight_elapsed=time.monotonic()-preflight_started
    cache=HERE/'empty-bytecode-root'
    if cache.exists() and any(cache.rglob('*')):raise ValueError('nonempty executable bytecode cache refused')
    work=HERE/'source-execution-r1';work.mkdir(exist_ok=False);(work/'stdin.txt').write_bytes(b'')
    command=review['source_command']
    bootstrap='exec(compile('+repr(stage_raw)+','+repr(str(stage))+',"exec"),{"__name__":"__main__","__file__":'+repr(str(stage))+'})'
    argv=[*command[:6],'-c',bootstrap,str(args.review.resolve()),args.sha256]
    # command[:6] is interpreter,-I,-S,-B,-X,pycache_prefix=... .
    environment=dict(os.environ);environment['PYTHONDONTWRITEBYTECODE']='1';started=time.monotonic()
    def before_resume(api,job,info,deadline):
        return suspended_identity(owner,api,job,info,review['python_executable'],deadline)
    with (work/'stdout.log').open('wb') as log,(work/'stdin.txt').open('rb') as stdin:
        result=owner.run_windows_owned_training(argv,source_root,log,60,environment,stdin,268435456,before_resume=before_resume)
    receipt={'argv':argv,'review_sha256':args.sha256,'owned':result,'elapsed_seconds':time.monotonic()-started,'parent_preflight_verified_files':preflight_files,'parent_preflight_seconds':preflight_elapsed,'actual_model_or_native_execution':False}
    (work/'execution.json').write_text(json.dumps(receipt,indent=2)+'\n')
    if result['exit_code'] or result['timed_out'] or result['owned_job_forced_termination'] or not result['owned_descendants_settled'] or result['owned_total_processes']!=1:raise ValueError('bounded source stage refused; preserve evidence/no retry')
    print(json.dumps(receipt),flush=True)
if __name__=='__main__':main()
