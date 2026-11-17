# SPDX-License-Identifier: Apache-2.0
"""Actual cheap synthetic children only; no FINAL alignment or source mutation."""
import hashlib,importlib.util,json,sys,time
from pathlib import Path
root=Path.cwd();source=root/'scripts/provision-static-onnx-runtime.py';raw=source.read_bytes();assert hashlib.sha256(raw).hexdigest()=='3778ba85759d02f61ead1de94817abd2670b059e4207e2bd0ecbe7700d7efa84'
spec=importlib.util.spec_from_file_location('prospective_owner',source);owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
out=root/'artifacts/advanced-split/models/qasper-owned-job-draft-controls-r1';out.mkdir(exist_ok=False)
handshake="import subprocess,sys; child=subprocess.Popen([sys.executable,'-I','-B','-c',\"import threading;print('ready',flush=True);threading.Event().wait()\"],stdout=subprocess.PIPE); assert child.stdout.readline().strip()==b'ready'; print('synthetic leader complete',flush=True)"
cases=[('normal_exit',"print('synthetic complete')",10),('waited_observer',"import subprocess;subprocess.run(['pwsh','-NoProfile','-NonInteractive','-Command',\"Write-Output 'synthetic'\"],capture_output=True,check=True);print('synthetic observed complete')",10),('blocked_orphan',handshake,10),('timeout_tree',handshake+";import threading;threading.Event().wait()",1)]
results=[]
for name,code,timeout in cases:
 value=owner.run_owned([sys.executable,'-I','-B','-c',code],root,out/(name+'.log'),timeout);results.append({'name':name,'result':value})
 if name in ('normal_exit','waited_observer'):assert value['exit_code']==0 and not value['owned_job_forced_termination'] and value['owned_descendants_settled'] and value['owned_job_final_active_processes']==0 and value['owned_job_final_member_pids']==[]
 if name=='blocked_orphan':assert value['exit_code']==0 and value['owned_job_forced_termination'] and not value['timed_out']
 if name=='timeout_tree':assert value['timed_out'] and value['owned_job_forced_termination'] and value['owned_descendants_settled']
assert source.read_bytes()==raw
receipt={'schema':'airk.private-owned-job-draft-controls/1','owner_sha256':hashlib.sha256(raw).hexdigest(),'results':results,'successful_guard_still_pending':True,'public_source_written_by_probe':False,'gold_or_alignment_access':False}
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt,indent=2))
