# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,copy,importlib.util,os
b=Path('D:/code/Github/airk/artifacts/v190'); root=b.parents[1]
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
m=json.loads((b/'p10-final-qualification-command-map-r4.json').read_text())
def revise(x):
    if isinstance(x,str): return x.replace('p10-runtime-final-r2','p10-runtime-final-r3').replace('p10-resource-final-r4','p10-resource-final-r5').replace('p10-final-','p10-final-').replace('-r4','-r5')
    if isinstance(x,list):return [revise(y) for y in x]
    if isinstance(x,dict):return {k:revise(v) for k,v in x.items()}
    return x
m=revise(m);work=Path(m['cwd'])
rpath=b/'p10-private-base-r5.json'; r=json.loads(rpath.read_text(encoding='utf-8-sig')); rhash=sha(rpath)
m['private_base_proof']={'receipt':str(rpath),'receipt_sha256':rhash,'path':r['path'],'owner_sid':r['owner_sid'],'sddl':r['sddl'],'verifier':str(b/'p10-verify-private-base-r5.ps1'),'validation':'Read-only exact creation receipt hash, current process SID and actual owner, protected single owner FullControl OI/CI ACE, exact original SDDL and real directory parent identity before every stage. Contents need not be empty; no ACL repair.'}
for c in m['commands']:c.setdefault('environment',{})['AIRK_PRIVATE_CENSUS_BASE']=r['path']
for d in m['driver_parity']:
    d['root_sha256']=sha(root/d['path']);d['worktree_sha256']=sha(work/d['path']);d['identical']=d['root_sha256']==d['worktree_sha256'];assert d['identical'],d['path']
m['unchanged_helper_sha256']['p10-local-command.py']=sha(b/'p10-local-command.py')
assert m['unchanged_helper_sha256']['p10-local-command.py'].startswith('99f837e2')
m['unchanged_helper_sha256']['p10-verify-private-base-r5.ps1']=sha(b/'p10-verify-private-base-r5.ps1')
assert all(sha(b/n)==h for n,h in m['unchanged_helper_sha256'].items())
os.environ['GIT_CONFIG_GLOBAL']=str(b/'p10-owned-ignore-controls/global.gitconfig')
spec=importlib.util.spec_from_file_location('p10_r5_source_identity_only',work/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
m['source_expected']=builder.source_identity();m['status']='fresh_prepared_not_run';m['native_source']=None;m['reused_receipts']=[]
m['qualification_change']='Fresh R5 all21 stages after test-only catalog fixture ownership, diagnostic then Windows fixture open-descendant ordering correction and helper/docs changes. Production native safety policy and assertions unchanged. Explicit newly owned private base replaces broad inherited test environment. No earlier receipts adopted or histories rewritten.'
m['environment']+=' R5 binds the exact creation-receipt private root in EVERY stage environment and validates original receipt hash/current actual owner/DACL before every command without mutating ACL or requiring post-test emptiness.'
p=b/'p10-final-qualification-command-map-r5.json';assert not p.exists();p.write_text(json.dumps(m,indent=2)+'\n')
s=(b/'p10-final-queue-r4.py').read_text().replace('p10-final-qualification-command-map-r4.json','p10-final-qualification-command-map-r5.json').replace('p10-final-qualification-r4.json','p10-final-qualification-r5.json').replace('p10-runtime-final-r2-seal.json','p10-runtime-final-r3-seal.json').replace('fresh r4 gates','fresh r5 gates').replace('R4 selected source drift','R5 selected source drift')
needle="        if seal is not None:\n            assert sealer.seal_runtime(runtime, record['source']) == seal, 'Runtime drift'\n        environment = os.environ.copy()"
extra="""        proof = plan['private_base_proof']
        assert hashlib.sha256(Path(proof['receipt']).read_bytes()).hexdigest() == proof['receipt_sha256'], 'Private-base creation receipt drift'
        checked = subprocess.run(['pwsh', '-NoProfile', '-File', proof['verifier'],
                                  '-ExpectedReceiptSha256', proof['receipt_sha256']],
                                 capture_output=True, text=True, check=False)
        assert checked.returncode == 0, 'Private-base ACL proof refused: ' + checked.stdout + checked.stderr
        observation = json.loads(checked.stdout.lstrip('\\ufeff'))
        assert all(observation[key] == proof[key] for key in ['path', 'owner_sid', 'sddl']), 'Private base identity drift'
        assert observation['acl_mutated'] is False
        record.setdefault('private_base_observations', []).append({'stage': row['name'], **observation})
        save()
"""
assert s.count(needle)==1;s=s.replace(needle,needle.replace('        environment = os.environ.copy()',extra+'        environment = os.environ.copy()'))
s=s.replace("        if row['name'] == 'release-build':\n            assert record['source_gates_complete']", "        assert environment['AIRK_PRIVATE_CENSUS_BASE'] == proof['path'], 'Explicit owned fixture root required in every stage'\n        if row['name'] == 'release-build':\n            assert record['source_gates_complete']")
s=s.replace("'AIRK_P10_RESOURCE_OUTPUT_DIR', 'CARGO_NET_OFFLINE']", "'AIRK_P10_RESOURCE_OUTPUT_DIR', 'CARGO_NET_OFFLINE', 'AIRK_PRIVATE_CENSUS_BASE']")
q=b/'p10-final-queue-r5.py';assert not q.exists();compile(s,str(q),'exec');q.write_text(s,encoding='utf-8',newline='\n')
assert len(m['commands'])==21 and sum(c['name']=='release-build' for c in m['commands'])==1
assert '--no-fail-fast' in next(c for c in m['commands'] if c['name']=='workspace-tests')['command']
assert all(c['environment']['AIRK_PRIVATE_CENSUS_BASE']==r['path'] for c in m['commands'])
print(json.dumps({'source_expected':m['source_expected'],'map_sha256':sha(p),'runner_sha256':sha(q),'receipt_sha256':rhash,'verifier_sha256':sha(b/'p10-verify-private-base-r5.ps1'),'drivers_identical':len(m['driver_parity']),'stages':len(m['commands'])},indent=2))
