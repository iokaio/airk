# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,subprocess,sys,os
b=Path('D:/code/Github/airk/artifacts/v190');w=b/'p10-worktree';v=json.loads((b/'post-p14-first-pass-qualification-r5/command-catalog.json').read_text());original=next(x['command'] for x in v['windows_steps'] if x['name']=='focused-list');expression=original[original.index('-E')+1];command=['C:/Users/tdotn/.cargo/bin/cargo.exe','nextest','list','--locked','--offline','--timings','-p','airk-core','-p','airk-store','-p','airk-cli','--lib','--test','combined_reads','--all-features','--profile','ci','--message-format','json','-E',expression];out=b/'post-p14-focused-list-r1.json';assert not out.exists();env=os.environ.copy();env['PATH']=str(b.parent/'tools/nextest')+os.pathsep+env['PATH'];
for k in ['AIRK_CATALOG_CANDIDATE_BINARY','AIRK_TEST_BINARY','AIRK_P10_RESOURCE_OUTPUT_DIR','AIRK_P11B_RESOURCE_OUTPUT_DIR']:env.pop(k,None)
print(json.dumps({'command':command,'scope':'Same exact37 functions, only owning Core/Store/CLI library and combined_reads targets.'}),flush=True)
with out.open('xb') as f:r=subprocess.run(command,cwd=w,env=env,stdout=f)
sys.exit(r.returncode)
