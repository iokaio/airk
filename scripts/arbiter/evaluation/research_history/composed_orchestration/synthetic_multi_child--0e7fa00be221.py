# SPDX-License-Identifier: Apache-2.0
"""Three simultaneous pipe-gated children; real SID proof, no native/model calls."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time
SID='S-1-5-21-3678370870-3858642417-826968105-1002'

def identity(helper):
    spec=importlib.util.spec_from_file_location('readonly_current_user_identity',helper)
    m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
    sid=m._Windows().user
    if sid!=SID:raise ValueError('actual process user differs from verified human')
    return {'pid':os.getpid(),'sid':sid}

def main(args):
    actual=identity(args.identity_helper)
    if args.child:
        print(json.dumps({'event':'ready',**actual}),flush=True)
        if sys.stdin.readline()!='GO\n':raise ValueError('observed parent gate absent')
        print(json.dumps({'event':'completed',**actual}),flush=True)
        return
    deadline=time.monotonic()+4
    def remaining():
        result=deadline-time.monotonic()
        if result<=0:raise TimeoutError('original synthetic gate cutoff expired')
        return result
    children=[]
    for _ in range(3):
        children.append(subprocess.Popen([sys.executable,'-B',str(Path(__file__).resolve()),
            '--identity-helper',str(args.identity_helper),'--child'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,text=True,creationflags=subprocess.CREATE_NO_WINDOW))
    ready=[]
    for process in children:
        remaining();row=json.loads(process.stdout.readline());remaining()
        if row!={'event':'ready','pid':process.pid,'sid':SID}:raise ValueError('actual child gate identity differs')
        ready.append(row)
    # All three report readiness before ANY one-use release is sent.
    print(json.dumps({'event':'all-three-simultaneously-gated','leader':actual,'children':ready}),flush=True)
    for process in children:
        process.stdin.write('GO\n');process.stdin.flush();process.stdin.close();process.stdin=None
    completed=[]
    for process in children:
        stdout,stderr=process.communicate(timeout=remaining())
        row=json.loads(stdout)
        if process.returncode!=0 or stderr or row!={'event':'completed','pid':process.pid,'sid':SID}:
            raise ValueError('actual synthetic child exit/identity differs')
        completed.append({'pid':process.pid,'exit_code':process.returncode,'completion':row})
    print(json.dumps({'event':'all-three-exited-naturally','leader':actual,'children':completed}),flush=True)

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--identity-helper',type=Path,required=True);p.add_argument('--child',action='store_true')
    main(p.parse_args())