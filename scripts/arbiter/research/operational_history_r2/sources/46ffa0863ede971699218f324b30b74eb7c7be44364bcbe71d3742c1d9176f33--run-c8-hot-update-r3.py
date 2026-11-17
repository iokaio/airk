# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,time
from pathlib import Path
base=Path(__file__).resolve().parent;root=base.parents[2];os.chdir(root)
private=json.loads((base/'user-private-fixture-base.json').read_text())['path'];os.environ.update(TEMP=private,TMP=private,AIRK_PRIVATE_CENSUS_BASE=private)
args=json.loads((base/'windows-debug-c8-installed-explorer-update-r2.json').read_text())['arguments'];manifest=json.loads((base/'windows-debug-c8-vsix-r4/publish/manifest.json').read_text());args[-2]=str(base/'windows-debug-c8-vsix-r4/publish'/manifest['archive']);args[-1]=manifest['archive_sha256']
start=time.monotonic()
with (base/'windows-debug-c8-installed-explorer-update-r3.log').open('x') as f:run=subprocess.run(args,stdout=f,stderr=subprocess.STDOUT)
receipt={'arguments':args,'exit_code':run.returncode,'elapsed_seconds':time.monotonic()-start,'qualification':'debug VSIX r4; actual same-window Explorer update; native execution disabled'}
with (base/'windows-debug-c8-installed-explorer-update-r3.json').open('x') as f:json.dump(receipt,f,indent=2)
print(json.dumps(receipt),flush=True);raise SystemExit(run.returncode)
