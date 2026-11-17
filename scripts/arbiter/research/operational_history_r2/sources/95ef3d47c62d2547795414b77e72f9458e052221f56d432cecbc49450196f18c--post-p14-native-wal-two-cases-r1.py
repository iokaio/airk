# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,os,subprocess,sys
B=Path(__file__).resolve().parent;W=B/'p10-worktree'
names=['post_p14_completed_shared_root_current_owned_destination_can_retire_without_changing_original','post_p14_interrupted_final_funding_retirement_preserves_ordinary_current_progress']
expression=' | '.join('(package(airk-cli) & test('+n+'))' for n in names)
common=['--locked','--offline','--timings','-p','airk-cli','--test','combined_reads','--all-features','--profile','ci','-E',expression]
env=os.environ.copy();env['PATH']=str(B.parent/'tools/nextest')+os.pathsep+env['PATH']
for key in ['AIRK_CATALOG_CANDIDATE_BINARY','AIRK_TEST_BINARY','AIRK_P10_RESOURCE_OUTPUT_DIR','AIRK_P11B_RESOURCE_OUTPUT_DIR']:env.pop(key,None)
out=B/'post-p14-native-wal-two-cases-list-r1.json'
with out.open('xb') as f:r=subprocess.run(['cargo','nextest','list',*common,'--message-format','json'],cwd=W,env=env,stdout=f)
if r.returncode:sys.exit(r.returncode)
v=json.loads(out.read_text());cases=[n for suite in v['rust-suites'].values() for n,t in suite['testcases'].items() if t['filter-match']['status']=='matches' and not t['ignored']]
assert len(cases)==2 and all(sum(c.endswith('::'+n) or c==n for c in cases)==1 for n in names),cases
print(json.dumps({'matched':cases,'expected':2}),flush=True)
sys.exit(subprocess.run(['cargo','nextest','run',*common,'--no-fail-fast','--retries','0','--test-threads','2'],cwd=W,env=env).returncode)
