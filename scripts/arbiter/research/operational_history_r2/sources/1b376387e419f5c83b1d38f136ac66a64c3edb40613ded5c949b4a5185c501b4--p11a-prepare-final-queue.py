# SPDX-License-Identifier: Apache-2.0
import ast, hashlib, json
from pathlib import Path
base = Path(__file__).resolve().parent
root = base.parents[1]
work = base/'p10-worktree'
old = json.loads((base/'p10-final-qualification-command-map-r6.json').read_text())
runtime = base/'p11a-runtime-final-r1'
def convert(value):
    if isinstance(value,str):
        return value.replace(str(base/'p10-runtime-final-r3'),str(runtime)).replace('p10-final-','p11a-final-').replace('-r6','-r1').replace('p10-local-command.py','p11-local-command.py').replace('p9-evaluate-gates.py','p11-evaluate-gates.py')
    if isinstance(value,list):return [convert(x) for x in value]
    if isinstance(value,dict):return {k:convert(v) for k,v in value.items()}
    return value
plan = convert(old)
plan['runtime'] = str(runtime)
plan['source_expected'] = {'sha256':'33e6df135da90839f409dfbba6dd2ce338af31675875e99f20d0b94af212f3f7','files':948}
rows = {r['name']:r for r in plan['commands']}
wrapper = str(base/'p11-local-command.py')
cargo = 'C:/Users/tdotn/.cargo/bin/cargo.exe'
def custom(name,args,expected=None):
    log=base/f'p11a-final-{name}-r1.log'
    value={'name':name,'cwd':str(work),'environment':dict(rows['workspace-tests']['environment']),
        'log':str(log),'command':['python',wrapper,str(log),*args]}
    if expected is not None:value['expected_passing_tests']=expected
    return value
transport=[]
pattern='profile_families::opted_file_family_report_keeps_full_native_cli_console_mcp_and_input_catalog_parity'
for suffix,feature in [('minimal','--no-default-features'),('helpers','--all-features')]:
    row=custom('profile-parity-'+suffix,[cargo,'test','-p','airk-cli','--test','combined_reads',feature,'--locked','--offline','--timings',pattern,'--','--exact','--test-threads=1','--nocapture'],1)
    row['environment']['AIRK_CATALOG_CANDIDATE_BINARY']=str(runtime/'airk.exe')
    transport.append(row)
nextest=custom('workspace-tests',[cargo,'nextest','run','--profile','ci','--workspace','--all-features','--locked','--offline','--timings','--no-tests','fail'])
nextest['environment']['PATH']=str(root/'artifacts/tools/nextest')+';'+__import__('os').environ['PATH']
doctests=custom('doctests',[cargo,'test','--workspace','--all-features','--doc','--locked','--offline','--timings'])
plan['commands']=[rows[n] for n in ['fmt','lightweight-clippy','workspace-clippy','lightweight-check']]+transport+[rows['workspace-build'],nextest,doctests]+[rows[n] for n in ['clients','publication-faults','ci-local','off','report','latency-gates','repo-check','license-notices','dependency-audit','python-tests','whitespace']]
plan['qualification_scope']='P11a: routine workspace nextest CI excludes exactly four documented long campaigns; full restoration and broader cumulative qualification remain P14. Doctests separate; no retry. Legacy replay gates unchanged, preceding reference explicitly P10 R6 rather than P8.'
for proof in plan['driver_parity']:
    for key,folder in [('root_sha256',root),('worktree_sha256',work)]:
        proof[key]=hashlib.sha256((folder/proof['path']).read_bytes()).hexdigest()
for name in ['p11-local-command.py','p11-evaluate-gates.py']:
    plan['unchanged_helper_sha256'][name]=hashlib.sha256((base/name).read_bytes()).hexdigest()
mapping=base/'p11a-final-qualification-command-map-r1.json'
assert not mapping.exists()
mapping.write_text(json.dumps(plan,indent=2)+'\n')
runner=(base/'p10-final-queue-r6.py').read_text()
runner=runner.replace("import atexit\n","import atexit\nimport time\n")
runner=runner.replace('p10-final-qualification-command-map-r6.json',mapping.name).replace('p10-final-qualification-r6.json','p11a-final-qualification-r1.json').replace('p10-final-qualification-owner.lock','p11a-final-qualification-owner.lock').replace('p10-runtime-final-r3-seal.json','p11a-runtime-final-r1-seal.json')
anchor="try:\n    assert source_builder.source_identity() == plan['source_expected'], 'Source changed before fresh r6 gates'"
assert anchor in runner
adopt="""try:
    release_receipt=base/'p11a-release-runtime-r1.receipt.json'
    record['waiting_for_release_receipt']=str(release_receipt)
    save()
    waiting=time.monotonic()
    while not release_receipt.exists():
        assert time.monotonic()-waiting<7200, 'Release prerequisite did not finish'
        time.sleep(3)
    release=json.loads(release_receipt.read_text())
    assert release['actual_exit_code']==release['exit_code']==0 and release['cargo_incremental']=='0'
    assert release['source_before']==release['source_after']==plan['source_expected'] and release['source_unchanged']
    assert hashlib.sha256((base/'p11a-release-runtime-r1.log').read_bytes()).hexdigest()==release['log_sha256']
    record['source']=plan['source_expected']
    record['adopted_release_receipt']=str(release_receipt)
    seal=sealer.seal_runtime(runtime,record['source'])
    with (base/'p11a-runtime-final-r1-seal.json').open('x',encoding='utf-8') as handle:
        json.dump(seal,handle,indent=2)
    assert source_builder.source_identity() == plan['source_expected'], 'Source changed before P11a gates'"""
runner=runner.replace(anchor,adopt)
runner=runner.replace("        if row['name'] == 'dependency-audit':", """        if row['name']=='workspace-tests':
            import xml.etree.ElementTree as ET
            junit=base/'p10-target/nextest/ci/junit.xml'
            cases=ET.parse(junit).getroot().findall('.//testcase')
            assert len(cases)>2000 and not any(c.find('failure') is not None or c.find('error') is not None for c in cases), 'Routine workspace results incomplete'
            record['steps'][-1]['nextest_cases']=len(cases)
            record['steps'][-1]['junit_sha256']=hashlib.sha256(junit.read_bytes()).hexdigest()
        if row['name'] == 'dependency-audit':""")
ast.parse(runner)
output=base/'p11a-final-queue-r1.py'
assert not output.exists()
output.write_text(runner,encoding='utf-8')
print(json.dumps({'prepared_only':True,'stages':len(plan['commands']),'source':plan['source_expected'],'release':'adopt existing in-progress build; never duplicate Cargo'}))
