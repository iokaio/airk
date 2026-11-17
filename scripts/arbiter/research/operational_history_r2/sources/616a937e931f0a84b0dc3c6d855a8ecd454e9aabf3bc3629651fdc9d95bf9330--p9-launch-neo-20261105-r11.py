# SPDX-License-Identifier: Apache-2.0
import json, subprocess, sys, time
from pathlib import Path
root=Path.cwd()
base=root/'artifacts/v190'
label='p9-source-20261105-r11'
subprocess.run([sys.executable,str(base/'make-source-snapshot.py'),label],check=True)
connection=json.loads((base/'neo-connection.local.json').read_text(encoding='utf-8-sig'))
remote='/Users/tylerjensen/airk-builds/p9-native-20261105-r11'
ssh=['ssh','-i',connection['identity_path'],*connection['ssh_options'],f"{connection['user']}@{connection['host']}"]
scp=['scp','-i',connection['identity_path'],*connection['ssh_options']]
plan=json.loads((base/'p9-neo-plan-draft.json').read_text(encoding='utf-8-sig'))
plan.update(draft=False,pending=[],steps=[
 {'name':'fmt','command':['cargo','fmt','--all','--check']},
 {'name':'ingest','command':['cargo','test','-p','airk-ingest','--lib','--all-features','--locked','--offline','--','--test-threads=1']},
 {'name':'kernel-policy-inputs','command':['cargo','test','-p','airk-kernel','--test','source_policy','--test','administrative_capture','--all-features','--locked','--offline','--','--test-threads=1']},
 {'name':'store-policy','command':['cargo','test','-p','airk-store','--lib','--all-features','--locked','--offline','service_catalog_policy','--','--test-threads=1']},
 {'name':'store-calibration','command':['cargo','test','-p','airk-store','--lib','--all-features','--locked','--offline','service_calibration_files','--','--test-threads=1']},
 {'name':'native-permanent-trim','command':['cargo','test','-p','airk-cli','--test','combined_reads','--all-features','--locked','--offline','permanent_trim','--','--test-threads=1']},
 {'name':'clippy','command':['cargo','clippy','--workspace','--all-targets','--all-features','--locked','--offline','--','-D','warnings']}
],not_run=['Complete Mac workspace and fresh runtime/performance; earlier evidence retains original identities.','Installed editor, signing, notarization, Intel, Linux and publication.'])

plan_file=base/'p9-neo-plan-20261105-r11.json'
plan_file.write_text(json.dumps(plan,indent=2)+'\n',encoding='utf-8')
wrapper=base/'p9-neo-owner-20261105-r11.py'
wrapper.write_text('''import json, os, subprocess, sys
from pathlib import Path
run=Path(__file__).resolve().parent
assert run.parent==Path.home()/'airk-builds' and run.name=='p9-native-20261105-r11'
args=json.loads((run/'plan.json').read_text())['runner_arguments']
result=1
try:
    result=subprocess.run([sys.executable,str(run/'runner.py'),*args],cwd=run).returncode
finally:
    source=(run/'source').resolve()
    target=(run/'target').resolve()
    assert source.parent==run and target.parent==run and target.name=='target'
    clean_env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+':'+os.environ.get('PATH',''))
    with (run/'cargo-clean.log').open('x') as log:
        cleanup=subprocess.run(['cargo','clean','--manifest-path',str(source/'Cargo.toml'),'--target-dir',str(target)],env=clean_env,cwd=source,stdout=log,stderr=subprocess.STDOUT).returncode
    (run/'cleanup.json').write_text(json.dumps({'test_exit_code':result,'cargo_clean_exit_code':cleanup,'target':str(target),'source':str(source),'source_preserved':source.is_dir()})+'\\n')
    print('CARGO_CLEAN_EXIT',cleanup,flush=True)
sys.exit(result if result else cleanup)
''',encoding='utf-8')
subprocess.run([*ssh,f'umask 077; mkdir {remote}'],check=True)
for local,name in [(base/(label+'.tar.gz'),'source.tar.gz'),(base/(label+'-snapshot.json'),'snapshot.json'),(plan_file,'plan.json'),(base/'p9-neo-run.py','runner.py'),(wrapper,'owner.py')]:
    subprocess.run([*scp,str(local),f"{connection['user']}@{connection['host']}:{remote}/{name}"],check=True)
command=f'cd {remote} && nohup /Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/python3.13 owner.py > owner.log 2>&1 < /dev/null &'
print(subprocess.check_output([*ssh,command],text=True))
(base/'p9-neo-active-20261105-r11.json').write_text(json.dumps({'remote':remote,'snapshot':label,'plan':str(plan_file),'cleanup':'owner.py finally calls cargo clean on exact isolated target'},indent=2)+'\n',encoding='utf-8')
print('STARTED',remote,flush=True)
