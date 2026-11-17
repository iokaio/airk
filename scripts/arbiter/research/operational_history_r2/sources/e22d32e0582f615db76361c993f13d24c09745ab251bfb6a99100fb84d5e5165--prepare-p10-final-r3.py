# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import copy,hashlib,json
b=Path(__file__).resolve().parent
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
m=copy.deepcopy(json.loads((b/'p10-final-qualification-command-map-r2.json').read_text()))
original=json.loads((b/'p10-final-qualification-command-map-r1.json').read_text())
release=next(row for row in original['commands'] if row['name']=='release-build')
m['commands']=[row for row in m['commands'] if row['name']!='release-build']
for row in m['commands']:
    row['log']=row['log'].replace('-r2.log','-r3.log')
    row['command']=[x.replace('-r2','-r3') if ('p10-final-' in x or 'p10-resource-final-' in x) and 'p10-runtime-final-' not in x else x for x in row['command']]
    row['environment']={k:v.replace('p10-resource-final-r2','p10-resource-final-r3') for k,v in row.get('environment',{}).items()}
m['resource_summary_commands']=[[x.replace('p10-resource-final-r2','p10-resource-final-r3') for x in command] for command in m['resource_summary_commands']]
m['adopt_original_release']={
 'receipt':str(Path(release['log']).with_suffix('.receipt.json')),'log':release['log'],
 'expected_command':release['command'][3:],
 'expected_incremental':'0','expected_profile':'release',
 'source_expected':m['frozen_source_expected'],
 'runtime':m['runtime'],'seal':str(b/'p10-runtime-final-r1-seal.json'),
 'launch_precondition':'Original r1 build and wrapper have fully exited successfully and complete receipt/log retained; primary has collected r2 duplicate-waiter failure. Never start or retry any release build from r3.'}
m['race_disposition']='Original r1 release child started just before parent stop. The earlier before-build assumption was incorrect. r2 duplicated a builder waiting on the Cargo cache lock; primary terminated only that waiting duplicate. Original compilation is preserved. r3 adopts its one successful sealed result, with no release-build command.'
m['status']='prepared_adoption_not_run'
assert len(m['commands'])==16 and all(row['name']!='release-build' for row in m['commands'])
out=b/'p10-final-qualification-command-map-r3.json'
assert not out.exists()
out.write_text(json.dumps(m,indent=2)+'\n')
runner=(b/'p10-final-queue-r2.py').read_text()
runner=runner.replace("mapping = base / 'p10-final-qualification-command-map-r2.json'","mapping = base / 'p10-final-qualification-command-map-r3.json'")
runner=runner.replace("output = base / 'p10-final-qualification-r2.json'","output = base / 'p10-final-qualification-r3.json'")
needle="    record['source_gates_complete'] = True\n    save()\n    for row in plan['commands']:"
assert runner.count(needle)==1
replacement='''    record['source_gates_complete'] = True
    # Adopt the original release producer only after its wrapper has published
    # a complete successful receipt. No build command exists in this queue.
    adoption = plan['adopt_original_release']
    release_receipt_path, release_log = Path(adoption['receipt']), Path(adoption['log'])
    release_receipt = json.loads(release_receipt_path.read_text())
    assert release_receipt['actual_exit_code'] == release_receipt['exit_code'] == 0
    assert release_receipt['validation_error'] is None and release_receipt['source_unchanged']
    assert release_receipt['source_before'] == release_receipt['source_after'] == expected == adoption['source_expected']
    assert release_receipt['cargo_incremental'] == adoption['expected_incremental'] == '0'
    assert release_receipt['command'] == adoption['expected_command']
    assert Path(release_receipt['workdir']) == Path(plan['cwd']) and Path(release_receipt['target']) == Path(plan['persistent_target'])
    assert hashlib.sha256(release_log.read_bytes()).hexdigest() == release_receipt['log_sha256'], 'Original release log drift'
    assert source_builder.source_identity() == expected, 'Source changed before original release adoption'
    seal = sealer.seal_runtime(runtime, expected)
    assert seal['build']['profile'] == adoption['expected_profile'] == 'release'
    assert seal['build']['source'] == expected and seal['verified_inventory']
    seal_path = Path(adoption['seal'])
    if seal_path.exists():
        assert json.loads(seal_path.read_text()) == seal, 'Existing original release seal drift'
    else:
        with seal_path.open('x',encoding='utf-8') as handle:
            json.dump(seal,handle,indent=2)
            handle.write('\\n')
    assert source_builder.source_identity() == expected, 'Source changed during original release adoption'
    record['steps'].append({'name':'release-build','reused_original_r1':True,'exit_code':0,
        'receipt':str(release_receipt_path),'receipt_sha256':hashlib.sha256(release_receipt_path.read_bytes()).hexdigest(),
        'log_sha256':release_receipt['log_sha256'],'seal':str(seal_path),
        'seal_sha256':hashlib.sha256(seal_path.read_bytes()).hexdigest()})
    record['race_disposition'] = plan['race_disposition']
    save()
    for row in plan['commands']:'''
runner=runner.replace(needle,replacement)
path=b/'p10-final-queue-r3.py'
assert not path.exists()
compile(runner,str(path),'exec')
path.write_text(runner)
print(json.dumps({'runner_sha256':sha(path),'map_sha256':sha(out),'remaining_stages':len(m['commands']),'release_build_commands':0}))
