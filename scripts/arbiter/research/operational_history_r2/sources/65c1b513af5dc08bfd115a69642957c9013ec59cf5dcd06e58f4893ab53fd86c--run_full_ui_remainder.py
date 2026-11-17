# SPDX-License-Identifier: Apache-2.0
"""Root-release-only test run: full Admin/VSCode catalogs with exact Help-pass reuse."""
import csv
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT=Path(__file__).resolve().parents[3]
HERE=Path(__file__).resolve().parent
OUT=ROOT/'artifacts/advanced-split/receipts/full-ui-remainder-r1'
NODE=ROOT/'artifacts/tools/node-v24.15.0-win-x64/node.exe'
NORMAL_SID='S-1-5-21-3678370870-3858642417-826968105-1002'

def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    if sys.argv[1:]!=['--root-resource-window-released']:
        raise SystemExit('Root must release the exclusive NN/resource window before tests launch')
    observed=subprocess.check_output(['whoami','/user','/fo','csv','/nh'],text=True)
    if next(csv.reader(io.StringIO(observed)))[-1]!=NORMAL_SID:
        raise SystemExit('Run under the reviewed normal human/browser account')
    assert digest(NODE)=='3331e1ffe19874215472217c5e94f5a0c6d8e18c4ac7111d3937aa0ad5e9b4a5'
    r3=ROOT/'artifacts/advanced-split/receipts/final-help-complete-r3/result.json'
    proof=json.loads(r3.read_bytes())
    admin_reused='packages/admin/test/helpSemantics.test.mjs'
    vscode_reused='packages/vscode/out/test/explorerHelp.test.js'
    assert digest(ROOT/admin_reused)==proof['final_source_files'][admin_reused]
    assert digest(ROOT/vscode_reused)==proof['extension_out'][vscode_reused]['sha256']
    admin=sorted(path.relative_to(ROOT).as_posix() for path in (ROOT/'packages/admin/test').glob('*.test.mjs'))
    vscode=sorted(path.relative_to(ROOT).as_posix() for path in (ROOT/'packages/vscode/out/test').glob('*.test.js'))
    assert admin_reused in admin and vscode_reused in vscode
    stages=[('admin-full-remainder',[name for name in admin if name!=admin_reused]),
            ('vscode-full-remainder',[name for name in vscode if name!=vscode_reused])]
    OUT.mkdir(parents=True,exist_ok=False)
    roster={'schema':'airk.private-full-ui-catalog-reuse/1','normal_windows_sid':NORMAL_SID,
        'current_catalogs':{'admin':admin,'vscode':vscode},'selected':dict(stages),
        'reused':{admin_reused:{'authored_cases':5,'source_sha256':digest(ROOT/admin_reused),'actual_pass':'final-help-resume-r2/shared-help-tests.json'},
            vscode_reused:{'authored_cases':3,'compiled_sha256':digest(ROOT/vscode_reused),'actual_pass':'final-help-r1/extension-help-tests.json'}},
        'complete_union_without_repeating_help':True,'r3_final_proof_sha256':digest(r3),
        'no_compile_build_reseal_native_model_or_cargo':True}
    (OUT/'roster.json').write_text(json.dumps(roster,indent=2)+'\n',encoding='utf-8')
    env={k:v for k,v in os.environ.items() if not k.startswith('AIRK_WRITE_')}
    cache=ROOT/'artifacts/advanced-split/npm-cache'
    env.update(NPM_CONFIG_CACHE=str(cache),NPM_CONFIG_LOGS_DIR=str(cache/'_logs'),NPM_CONFIG_UPDATE_NOTIFIER='false',PYTHONIOENCODING='utf-8')
    results=[]
    for name,files in stages:
        command=[sys.executable,'-B','scripts/dev/receipt_runner.py','--groups','native,harness,fixtures,frontend,extension',
            '--tier','T2','--out',str(OUT),'--node',str(NODE),'--target-dir',str(ROOT/'artifacts/v190/p10-target'),
            name,str(NODE),'--test',*files]
        start=time.monotonic()
        with (OUT/(name+'-wrapper.log')).open('x',encoding='utf-8') as log:
            result=subprocess.run(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
        row={'name':name,'command':command,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-start}
        results.append(row)
        (OUT/'results.json').write_text(json.dumps(results,indent=2)+'\n',encoding='utf-8')
        print(json.dumps(row),flush=True)
        if result.returncode:return result.returncode
    return 0

if __name__=='__main__':raise SystemExit(main())
