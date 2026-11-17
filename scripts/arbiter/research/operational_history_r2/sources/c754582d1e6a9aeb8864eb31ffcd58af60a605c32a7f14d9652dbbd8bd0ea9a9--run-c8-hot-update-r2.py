# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,time
from pathlib import Path
root=Path(__file__).resolve().parents[3];os.chdir(root);evidence=Path(__file__).resolve().parent
private=json.loads((evidence/'user-private-fixture-base.json').read_text())['path'];os.environ.update(TEMP=private,TMP=private,AIRK_PRIVATE_CENSUS_BASE=private)
def gate(name,args):
    start=time.monotonic()
    with (evidence/(name+'.log')).open('x') as log:result=subprocess.run(args,stdout=log,stderr=subprocess.STDOUT)
    receipt={'arguments':args,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-start,'qualification':'same sealed local debug VSIX; actual installed Code; native execution disabled for UI update gate'}
    with (evidence/(name+'.json')).open('x') as out:json.dump(receipt,out,indent=2)
    print(json.dumps(receipt),flush=True)
    if result.returncode:raise SystemExit(result.returncode)
gate('windows-debug-c8-explorer-update-contracts-r2',['node','--test','scripts/tests/check-explorer-hot-update.test.cjs','scripts/tests/check-vsix-upgrade-editor.test.cjs'])
gate('windows-debug-c8-installed-explorer-update-r2',json.loads((evidence/'windows-debug-c8-installed-explorer-update-r1.json').read_text())['arguments'])
