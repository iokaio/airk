# SPDX-License-Identifier: Apache-2.0
"""Freeze public isolated P10 source and prepare (never launch) one Neo batch."""
import hashlib, importlib.util, json, re, subprocess, sys, tarfile
from pathlib import Path

base=Path(__file__).resolve().parent
root=base/'p10-worktree'
label=sys.argv[1]
assert re.fullmatch(r'p10-native-[a-z0-9-]+',label)
archive=base/(label+'.tar.gz')
assert not archive.exists(), 'Use a new run label after every source correction'
spec=importlib.util.spec_from_file_location('builder',root/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec); spec.loader.exec_module(builder)
before=builder.source_identity()
git=['git','-c','safe.directory='+root.as_posix()]
names=subprocess.check_output([*git,'ls-files','-z','--cached','--others','--exclude-standard'],cwd=root).split(b'\0')
excluded={'.git','.codex','.agents','.aws','.mcp.json','.ssh','temp','target','artifacts','node_modules','__pycache__'}
files=sorted({name.decode() for name in names if name and not(set(Path(name.decode()).parts)&excluded)})
manifest={}
with tarfile.open(archive,'w:gz') as tar:
    for name in files:
        path=root/name
        assert path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(root.resolve()),name
        manifest[name]=hashlib.sha256(path.read_bytes()).hexdigest()
        tar.add(path,arcname=name,recursive=False)
assert builder.source_identity()==before
assert all(hashlib.sha256((root/name).read_bytes()).hexdigest()==digest for name,digest in manifest.items())
record={'entry_commit':subprocess.check_output([*git,'rev-parse','HEAD'],cwd=root).decode().strip(),
        'native_source':before,'files':manifest,'archive_sha256':hashlib.sha256(archive.read_bytes()).hexdigest()}
(base/(label+'-snapshot.json')).write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
plan=json.loads((base/'p9-neo-plan-20261105-r13.json').read_text(encoding='utf-8'))
plan.update(phase='P10',draft=False,pending=[],steps=[],not_run=[
    'Complete Mac workspace/runtime/performance campaign, semantic-ready split execution, installed editor, signing/notarization, Intel, Linux and publication.',
    'Windows-only HANDLE/rename tests are cfg-excluded on macOS; retain separate Windows native receipts.'])
def step(name,package,pattern,features=None,target=None,skip=None):
    command=['cargo','test','-p',package,*(target or ['--lib']),'--locked','--offline','--timings']
    if features: command+=features
    command += [pattern,'--','--test-threads=1']
    if skip: command += ['--skip',skip]
    plan['steps'].append({'name':name,'command':command})
plan['steps'].append({'name':'fmt','command':['cargo','fmt','--all','--check']})
step('core-partition','airk-core','source_policy::split')
step('store-funded-split','airk-store','split')
step('store-funding','airk-store','quota::funding')
step('store-funded-context','airk-store','funded')
happy_cutover='real_empty_native_owner_completes_witness_fence_remainder_membership_and_funding_ack'
for suffix,features in [('lightweight',['--no-default-features']),('helpers',['--all-features'])]:
    step('kernel-policy-positive-'+suffix,'airk-kernel','positive_partition_',features,['--test','source_policy'])
    step('kernel-policy-cleanup-pin-'+suffix,'airk-kernel','service_policy_cleanup_releases_only_its_own_pin_and_preserves_actual_error_reports',features,['--test','source_policy'])
    step('kernel-tokenizer-positive-'+suffix,'airk-kernel','shared_calibration',features)
    step('cli-control-'+suffix,'airk-cli','catalog_split_control',features,skip=happy_cutover if suffix=='helpers' else None)
    step('cli-claimed-worker-'+suffix,'airk-cli','project_worker_split',features,skip=happy_cutover if suffix=='helpers' else None)
# Run the embedding-worker-gated real cutover exactly once, separately from
# either broader CLI owner filter above. Lightweight builds cannot run it.
step('cli-native-happy-cutover-helpers','airk-cli',happy_cutover,['--all-features'])
(base/(label+'-plan.json')).write_text(json.dumps(plan,indent=2)+'\n',encoding='utf-8')
runner=(base/'p9-neo-run.py').read_text(encoding='utf-8')
runner=runner.replace("run.name.startswith('p9-native-')","run.name.startswith('p10-native-')")
runner=runner.replace("plan['phase'] == 'P9'","plan['phase'] == 'P10'")
runner=runner.replace("'phase': 'P9'","'phase': 'P10'").replace('airk.p9.neo-qualification/1','airk.p10.neo-qualification/1')
runner=runner.replace('import time\n','import time\nimport tempfile\nimport re\n')
runner=runner.replace("environment.pop('AIRK_TEST_BINARY', None)","environment.pop('AIRK_TEST_BINARY', None)\n    environment['TMPDIR'] = str(Path(tempfile.gettempdir()).resolve(strict=True))")
runner=runner.replace("('CARGO_TARGET_DIR','CARGO_BUILD_JOBS'", "('TMPDIR','CARGO_TARGET_DIR','CARGO_BUILD_JOBS'")
runner=runner.replace("steps.append({'name': row['name']", "actual_exit_code = result.returncode\n        passing_tests = None\n        validation_error = None\n        if command[:2] == ['cargo', 'test']:\n            observed = (run / (row['name'] + '.log')).read_text(encoding='utf-8')\n            passing_tests = sum(map(int, re.findall(r'test result: (?:ok|FAILED)[.] (\\d+) passed', observed)))\n            if result.returncode == 0 and passing_tests == 0:\n                validation_error = 'Cargo test filter matched no passing tests'\n                result.returncode = 1\n        steps.append({'actual_exit_code': actual_exit_code, 'passing_tests': passing_tests, 'validation_error': validation_error, 'name': row['name']")
# Original runner JSON/log writes already specify or run under UTF-8 on Mac;
# make every unqualified read/write explicit for portable inspection as well.
runner=runner.replace(".write_text(row['name'])", ".write_text(row['name'], encoding='utf-8')")
(base/(label+'-runner.py')).write_text(runner,encoding='utf-8')
print(json.dumps({'label':label,'native_source':before,'files':len(files),'archive_sha256':record['archive_sha256'],'prepared_only':True}))
