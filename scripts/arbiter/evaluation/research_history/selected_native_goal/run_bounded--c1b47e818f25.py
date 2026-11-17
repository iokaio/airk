# SPDX-License-Identifier: Apache-2.0
"""Bounded, reviewed no-neural source/rubric preparation entrypoint."""
import argparse
import hashlib
import os
from pathlib import Path
import runpy
import sys
import threading
import types
ROOT=Path(__file__).resolve().parents[5]
HERE=Path(__file__).resolve().parent

THREAD_KEYS=('OMP_NUM_THREADS','OPENBLAS_NUM_THREADS','MKL_NUM_THREADS','NUMEXPR_NUM_THREADS','VECLIB_MAXIMUM_THREADS')

def enforce_thread_environment():
    for key in THREAD_KEYS:os.environ[key]='1'

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--operation',choices=('materialize_sources','goal_rubric'),required=True)
    parser.add_argument('--script-sha256',required=True)
    parser.add_argument('arguments',nargs=argparse.REMAINDER)
    args=parser.parse_args();enforce_thread_environment();target=(HERE.parent/'source/materialize_sources.py') if args.operation=='materialize_sources' else (HERE.parent/'data-preparation-r2/goal_rubric_streaming.py');raw=target.read_bytes()
    if hashlib.sha256(raw).hexdigest()!=args.script_sha256:raise ValueError('reviewed data source changed')
    path=ROOT/'artifacts/advanced-split/models/a1-next-experiment-blind-metadata-r5/audit.py';source=path.read_bytes()
    if hashlib.sha256(source).hexdigest()!='9c540c08c2e56d868059ab8425193e52f0d9141f75d16df01f622c679032425b':raise ValueError('resource owner drift')
    owner=types.ModuleType('selected_data_resource_owner');owner.__file__=str(path);exec(compile(source,str(path),'exec'),owner.__dict__)
    owner.JOB_STATE=owner.enforce_budget();done=threading.Event()
    def expire():
        if not done.wait(60):os._exit(124)
    threading.Thread(target=expire,daemon=True).start()
    if args.operation=='goal_rubric':
        import pyarrow
        pyarrow.set_cpu_count(1);pyarrow.set_io_thread_count(1)
        if pyarrow.cpu_count()!=1 or pyarrow.io_thread_count()!=1:raise ValueError('Arrow thread cap refused')
    sys.path.insert(0,str(target.parent));sys.argv=[str(target),*(args.arguments[1:]if args.arguments[:1]==['--']else args.arguments)]
    runpy.run_path(str(target),run_name='__main__');done.set()
if __name__=='__main__':main()
