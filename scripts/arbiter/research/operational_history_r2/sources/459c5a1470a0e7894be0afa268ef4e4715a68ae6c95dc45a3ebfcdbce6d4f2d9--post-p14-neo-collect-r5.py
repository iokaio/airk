# SPDX-License-Identifier: Apache-2.0
"""Explicitly launch OR collect one already frozen post-P14 portable Neo batch."""
import hashlib,json,re,subprocess,sys,tempfile
from pathlib import Path
base=Path(__file__).resolve().parent
action,label=sys.argv[1:]
assert action in ['launch','collect'] and re.fullmatch(r'post-p14-native-[a-z0-9-]+',label)
connection=json.loads((base/'neo-connection.local.json').read_text(encoding='utf-8-sig'))
peer=connection['user']+'@'+connection['host']
ssh=['ssh','-i',connection['identity_path'],*connection['ssh_options'],peer]
scp=['scp','-i',connection['identity_path'],*connection['ssh_options']]
remote='/Users/tylerjensen/airk-builds/'+label
def download(name,local_name,optional=False):
    # Download independently before touching immutable local evidence. Repeat
    # collection may verify identical bytes, never replace a prior receipt.
    destination=base/local_name
    with tempfile.TemporaryDirectory(prefix='post-p14-neo-collect-',dir=base) as scratch:
        fresh=Path(scratch)/'download'
        result=subprocess.run([*scp,peer+':'+remote+'/'+name,str(fresh)])
        if result.returncode:
            assert optional, 'Required remote evidence unavailable: '+name
            return False
        content=fresh.read_bytes()
        if destination.exists():
            assert destination.read_bytes()==content, 'Different immutable local evidence: '+local_name
        else:
            with destination.open('xb') as output: output.write(content)
    return True
if action=='launch':
    prepared=json.loads((base/(label+'-prepared.json')).read_text(encoding='utf-8'))
    assert prepared['phase']=='PostP14Portable' and prepared['prepared_only'] and not prepared['ssh_launched']
    for path,expected in prepared['pins'].items(): assert hashlib.sha256(Path(path).read_bytes()).hexdigest()==expected,path
    for suffix in ['.tar.gz','-snapshot.json','-plan.json','-runner.py','-head.commit','-head.tree']:
        assert (base/(label+suffix)).is_file()
    subprocess.run([*ssh,'umask 077; mkdir '+remote],check=True)
    for path,name in [(base/(label+'.tar.gz'),'source.tar.gz'),(base/(label+'-snapshot.json'),'snapshot.json'),
                      (base/(label+'-plan.json'),'plan.json'),(base/(label+'-runner.py'),'runner.py'),(base/'post-p14-neo-owner-r3.py','owner.py'),(base/(label+'-head.commit'),'head.commit'),(base/(label+'-head.tree'),'head.tree')]:
        subprocess.run([*scp,str(path),peer+':'+remote+'/'+name],check=True)
    command='cd '+remote+' && nohup /Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/python3.13 owner.py > owner.log 2>&1 < /dev/null &'
    subprocess.run([*ssh,command],check=True)
    with (base/(label+'-active.json')).open('x',encoding='utf-8') as output:
        output.write(json.dumps({'remote':remote,'snapshot':label,'cleanup':'owner finally cargo clean + target absence verification'})+'\n')
else:
    subprocess.run([*ssh,'test -f '+remote+'/cleanup.json'],check=True) # Completion receipt only; failed targets must remain collectable.
    plan=json.loads((base/(label+'-plan.json')).read_text(encoding='utf-8'))
    planned={row['name']:row for row in plan['steps']}
    assert len(planned)==len(plan['steps'])
    assert all(re.fullmatch(r'[A-Za-z0-9_-]+',name) for name in planned)
    for name in ['cleanup.json','cargo-clean.log','owner.log','timings.json']:
        download(name,label+'-'+name)
    available=[]
    for name in ['qualification.json','steps.json']:
        if download(name,label+'-'+name,optional=True):
            available.append(name)
    steps=json.loads((base/(label+'-steps.json')).read_text(encoding='utf-8')) if 'steps.json' in available else []
    assert [row['name'] for row in steps]==list(planned)[:len(steps)], 'Remote steps differ from frozen local plan'
    for row in steps:
        assert row['name'] in planned
        expected=[item.replace('{run}',remote).replace('{source}',remote+'/source') for item in planned[row['name']]['command']]
        assert row['command']==expected, 'Remote command differs from frozen plan'
        download(row['name']+'.log',label+'-'+row['name']+'.log')
    timings=json.loads((base/(label+'-timings.json')).read_text(encoding='utf-8'))
    assert len(timings['files'])<=256
    seen=set()
    for row in timings['files']:
        name=row['name']
        assert re.fullmatch(r'[A-Za-z0-9._-]+[.]html',name) and name not in seen
        seen.add(name)
        assert re.fullmatch(r'[a-f0-9]{64}',row['sha256']) and 0<=row['bytes']<=64<<20
        local=label+'-timing-'+name
        download('cargo-timings/'+name,local)
        content=(base/local).read_bytes()
        assert len(content)==row['bytes'] and hashlib.sha256(content).hexdigest()==row['sha256']
    cleanup=json.loads((base/(label+'-cleanup.json')).read_text(encoding='utf-8'))
    # All available logs/reports/timings have been retained before refusing a run.
    # A failed cleanup/containment must never block collection of its own evidence.
    actual_target_absence=subprocess.run([*ssh,'test ! -e '+remote+'/target']).returncode
    fresh=base/(label+'-collection-target-absence.json')
    observation={'remote':remote,'target_absence_exit_code':actual_target_absence,'evidence_downloaded_before_probe':True}
    content=(json.dumps(observation,sort_keys=True)+'\n').encode()
    if fresh.exists():assert fresh.read_bytes()==content,'Changed immutable collection target observation'
    else:
        with fresh.open('xb') as output:output.write(content)
    assert actual_target_absence==0, 'Evidence preserved; exact owned target remains/absence unknown'
    assert cleanup['test_exit_code']==0 and cleanup['execution_error'] is None
    assert cleanup['group_absent'] is True and cleanup['drain_error'] is None
    assert cleanup['cleanup_attempted'] is True
    assert timings['error'] is None and cleanup['timing_copy_error'] is None
    assert cleanup['cargo_clean_exit_code']==0 and cleanup['target_removed']
    assert 'qualification.json' in available, 'Evidence collected; runner failed before a complete qualification report'
    qualified=json.loads((base/(label+'-qualification.json')).read_text(encoding='utf-8'))
    snapshot=json.loads((base/(label+'-snapshot.json')).read_text(encoding='utf-8'))
    assert qualified['phase']=='PostP14Portable' and qualified['native_source']==snapshot['native_source']
    assert qualified['plan_sha256']==hashlib.sha256((base/(label+'-plan.json')).read_bytes()).hexdigest()
    assert qualified['archive_sha256']==snapshot['archive_sha256']
    assert qualified['environment']['CARGO_BUILD_JOBS']=='1' and qualified['environment']['CARGO_INCREMENTAL']=='0'
    assert qualified['environment']['CARGO_NET_OFFLINE']=='true'
    assert qualified['source_before']['unchanged'] and qualified['source_after']['unchanged']
    assert qualified['status']=='passed' and len(steps)==len(planned)
    # Reconcile R7 selection:33new portable+8existing related+1P14 related.
    expected_total=sum(row.get('expected_passing_tests',0) for row in planned.values())
    assert expected_total==42
    assert sum(row.get('passing_tests') or 0 for row in steps)==expected_total
    for row in steps:
        assert row['actual_exit_code']==row['exit_code']==0 and row['validation_error'] is None
        if 'expected_passing_tests' in planned[row['name']]:assert row['passing_tests']==planned[row['name']]['expected_passing_tests']
print(action,label)
