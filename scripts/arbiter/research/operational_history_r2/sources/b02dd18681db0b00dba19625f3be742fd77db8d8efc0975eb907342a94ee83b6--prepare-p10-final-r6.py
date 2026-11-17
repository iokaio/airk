# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,copy,importlib.util,os
b=Path(__file__).resolve().parent;root=b.parents[1];sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
m=json.loads((b/'p10-final-qualification-command-map-r5.json').read_text())
def revise(value):
 if isinstance(value,str):
  return value.replace('-r5','-r6') if 'p10-final-' in value or 'p10-resource-final-' in value else value
 if isinstance(value,list):return [revise(v) for v in value]
 if isinstance(value,dict):return {k:revise(v) for k,v in value.items()}
 return value
m=revise(m);work=Path(m['cwd']);row=copy.deepcopy(next(c for c in m['commands'] if c['name']=='split-process-resources'))
row['name']='permanent-trim-native-regression';row['log']=str(b/'p10-final-permanent-trim-native-regression-r6.log');row['environment'].pop('AIRK_P10_RESOURCE_OUTPUT_DIR')
row['command']=['python',str(b/'p10-local-command.py'),row['log'],'cargo','test','-p','airk-cli','--test','combined_reads','--all-features','--locked','--timings','permanent_trim::permanent_trim_fences_retained_grants_and_preserves_native_cli_mcp_results','--','--exact','--test-threads=1','--nocapture']
row['expected_passing_tests']=1;row['precondition']='Fresh R6 source gates and NEW sealed runtime-final-r3 containing actual metadata policy preservation fix. Never use a745 runtime-final-r2 for this proof.';row['acceptance']='Exactly one actual canonical handler/CLI/MCP permanent-trim process regression passes using the newly built candidate. Stop on failure before resources or full workspace; no assertion or complete-response reduction.'
pos=next(i for i,c in enumerate(m['commands']) if c['name']=='release-build');m['commands'].insert(pos+1,row)
for proof in m['driver_parity']:
 proof['root_sha256']=sha(root/proof['path']);proof['worktree_sha256']=sha(work/proof['path']);proof['identical']=proof['root_sha256']==proof['worktree_sha256'];assert proof['identical'],proof['path']
assert all(sha(b/name)==digest for name,digest in m['unchanged_helper_sha256'].items())
assert sha(Path(m['private_base_proof']['receipt']))==m['private_base_proof']['receipt_sha256']
os.environ['GIT_CONFIG_GLOBAL']=str(b/'p10-owned-ignore-controls/global.gitconfig')
spec=importlib.util.spec_from_file_location('p10_r6_identity_read_only',work/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
m['source_expected']=builder.source_identity();assert m['source_expected']['sha256'].startswith('c713b469'),m['source_expected'];assert m['source_expected']['files']==945
m['qualification_change']='Fresh R6 all22 stages. Production metadata read adapters now preserve scoped source policy through snapshot/coverage/readiness observations; scoped deny/union guards remain authoritative. R5 was prepared only and is preserved unlaunched. Must build NEW runtime-final-r3; a745 runtime-final-r2 lacks the actual fix. One exact previously failing permanent-trim native process test runs immediately after fresh release sealing before expensive resources/workspace. All earlier histories remain unchanged; no adoption.'
m['status']='fresh_prepared_not_run';m['native_source']=None;m['reused_receipts']=[]
assert len(m['commands'])==22 and sum(c['name']=='release-build' for c in m['commands'])==1
assert m['commands'][pos+1]['name']=='permanent-trim-native-regression'
assert all(c['environment']['AIRK_PRIVATE_CENSUS_BASE']==m['private_base_proof']['path'] for c in m['commands'])
assert '--no-fail-fast' in next(c for c in m['commands'] if c['name']=='workspace-tests')['command']
assert not Path(m['runtime']).exists()
assert all(not Path(c['log']).exists() for c in m['commands'])
out=b/'p10-final-qualification-command-map-r6.json';assert not out.exists();out.write_text(json.dumps(m,indent=2)+'\n')
s=(b/'p10-final-queue-r5.py').read_text().replace('p10-final-qualification-command-map-r5.json','p10-final-qualification-command-map-r6.json').replace('p10-final-qualification-r5.json','p10-final-qualification-r6.json').replace('fresh r5 gates','fresh r6 gates').replace('R5 selected source drift','R6 selected source drift')
q=b/'p10-final-queue-r6.py';assert not q.exists();compile(s,str(q),'exec');q.write_text(s,encoding='utf-8',newline='\n')
print(json.dumps({'source_expected':m['source_expected'],'runner_sha256':sha(q),'map_sha256':sha(out),'stages':len(m['commands']),'runtime':m['runtime'],'driver_pairs':len(m['driver_parity']),'private_receipt_sha256':m['private_base_proof']['receipt_sha256']},indent=2))
