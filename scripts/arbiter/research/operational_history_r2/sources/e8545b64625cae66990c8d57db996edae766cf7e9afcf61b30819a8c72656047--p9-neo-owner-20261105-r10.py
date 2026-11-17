# SPDX-License-Identifier: Apache-2.0
import json, os, subprocess, sys
from pathlib import Path
run=Path(__file__).resolve().parent
assert run.parent==Path.home()/'airk-builds' and run.name=='p9-native-20261105-r10'
args=json.loads((run/'plan.json').read_text())['runner_arguments']
result=1
try:
    result=subprocess.run([sys.executable,str(run/'runner.py'),*args],cwd=run).returncode
finally:
    source=(run/'source').resolve()
    target=(run/'target').resolve()
    assert source.parent==run and target.parent==run and target.name=='target'
    clean_env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+':'+os.environ.get('PATH',''))
    with (run/'cargo-clean.log').open('x') as log:
        cleanup=subprocess.run(['cargo','clean','--manifest-path',str(source/'Cargo.toml'),'--target-dir',str(target)],env=clean_env,cwd=source,stdout=log,stderr=subprocess.STDOUT).returncode
    (run/'cleanup.json').write_text(json.dumps({'test_exit_code':result,'cargo_clean_exit_code':cleanup,'target':str(target),'source':str(source),'source_preserved':source.is_dir()})+'\n')
    print('CARGO_CLEAN_EXIT',cleanup,flush=True)
sys.exit(result if result else cleanup)
