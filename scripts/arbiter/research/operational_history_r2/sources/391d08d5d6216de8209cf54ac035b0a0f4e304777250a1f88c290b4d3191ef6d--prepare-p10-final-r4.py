# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import copy,hashlib,json,importlib.util
b=Path(__file__).resolve().parent
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
m=copy.deepcopy(json.loads((b/'p10-final-qualification-command-map-r1.json').read_text()))
r3=json.loads((b/'p10-final-qualification-command-map-r3.json').read_text())
work=Path(m['cwd']); root=b.parents[1]
for row in m['commands']:
    row['log']=row['log'].replace('-r1.log','-r4.log')
    row['command']=[x.replace('p10-runtime-final-r1','p10-runtime-final-r2') for x in row['command']]
    row['command']=[x.replace('-r1','-r4') if ('p10-final-' in x or 'p10-resource-final-' in x) and 'p10-runtime-final-' not in x else x for x in row['command']]
    row['environment']={k:v.replace('p10-runtime-final-r1','p10-runtime-final-r2').replace('p10-resource-final-r1','p10-resource-final-r4') for k,v in row.get('environment',{}).items()}
    if row['name']=='workspace-tests':
        assert row['command'][-2:]==['--','--test-threads=4']
        row['command'].insert(row['command'].index('--'),'--no-fail-fast')
    if row['name']=='dependency-audit':
        source=next(x for x in r3['commands'] if x['name']=='dependency-audit')
        row['environment']=copy.deepcopy(source['environment'])
        row['command']= [x.replace('p10-final-dependency-audit-r3','p10-final-dependency-audit-r4') for x in source['command']]
        row['expected_report']=copy.deepcopy(source['expected_report'])
        row['expected_report']['path']=row['expected_report']['path'].replace('-r3.json','-r4.json')
m['resource_summary_commands']=[[x.replace('p10-resource-final-r1','p10-resource-final-r4') for x in command] for command in m['resource_summary_commands']]
m['runtime']=m['runtime'].replace('p10-runtime-final-r1','p10-runtime-final-r2')
m['runtime_seal_gate']['call']=m['runtime_seal_gate']['call'].replace('p10-runtime-final-r1','p10-runtime-final-r2')
for proof in m['driver_parity']:
    proof['root_sha256']=sha(root/proof['path'])
    proof['worktree_sha256']=sha(work/proof['path'])
    proof['identical']=proof['root_sha256']==proof['worktree_sha256']
    assert proof['identical'],proof['path']
assert all(sha(b/name)==digest for name,digest in m['unchanged_helper_sha256'].items())
spec=importlib.util.spec_from_file_location('p10_r4_source_identity_only',work/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
m['source_expected']=builder.source_identity()
m['native_source']=None
m['status']='fresh_prepared_not_run'
m['reused_receipts']=[]
m['qualification_change']='New native source includes only test-fixture ownership correction and runtime builder --timings after prior source gates. Rust production behavior unchanged; activation/ACL requirements unchanged. All four source prerequisite gates must run fresh. No r1/r2/r3 adoption or evidence rewritten.'
m['environment'] += ' Live dependency-advisory gate only uses pinned existing deny/vet PATH and CARGO_NET_OFFLINE=false with --require-fresh/full saved report verification. Build/test stages stay offline. Full workspace uses --no-fail-fast to collect all targets plus --test-threads=4.'
assert len(m['commands'])==21 and sum(x['name']=='release-build' for x in m['commands'])==1
out=b/'p10-final-qualification-command-map-r4.json';assert not out.exists();out.write_text(json.dumps(m,indent=2)+'\n')
runner=(b/'p10-final-queue-r1.py').read_text()
runner=runner.replace('p10-final-qualification-command-map-r1.json','p10-final-qualification-command-map-r4.json')
runner=runner.replace('p10-final-qualification-r1.json','p10-final-qualification-r4.json')
runner=runner.replace('p10-runtime-final-r1-seal.json','p10-runtime-final-r2-seal.json')
needle="    for row in plan['commands']:\n        if record['source'] is not None:"
assert runner.count(needle)==1
runner=runner.replace(needle,"    assert source_builder.source_identity() == plan['source_expected'], 'Source changed before fresh r4 gates'\n    for row in plan['commands']:\n        if record['source'] is not None:")
needle="        assert receipt['source_after'] == record['source'], 'Source drift'"
extra='''
        assert receipt['source_after'] == plan['source_expected'], 'R4 selected source drift'
        if row['name'] == 'dependency-audit':
            report_path = Path(row['expected_report']['path'])
            report = json.loads(report_path.read_text())
            assert report['status'] == 'passed' and report['all_requested_checks_passed'] is True
            assert report['fresh_full_advisory_check'] is True
            assert report['network_mode'] == 'live advisories, locked vet imports'
            expected_checks = {'source_review', 'cargo_audit', 'npm_audit', 'cargo_deny',
                               'cargo_vet', 'advisory_dispositions', 'input_integrity'}
            assert set(report['checks']) == expected_checks
            assert all(value['status'] == 'passed' for value in report['checks'].values())
            record['steps'][-1]['dependency_report'] = str(report_path)
            record['steps'][-1]['dependency_report_sha256'] = hashlib.sha256(report_path.read_bytes()).hexdigest()
'''
assert runner.count(needle)==1
runner=runner.replace(needle,needle+extra)
runner=runner.replace("'AIRK_P10_RESOURCE_OUTPUT_DIR'] if key in environment", "'AIRK_P10_RESOURCE_OUTPUT_DIR', 'CARGO_NET_OFFLINE'] if key in environment")
path=b/'p10-final-queue-r4.py';assert not path.exists();compile(runner,str(path),'exec');path.write_text(runner)
print(json.dumps({'source_expected':m['source_expected'],'runner_sha256':sha(path),'map_sha256':sha(out),'commands':len(m['commands']),'drivers_identical':len(m['driver_parity'])}))
