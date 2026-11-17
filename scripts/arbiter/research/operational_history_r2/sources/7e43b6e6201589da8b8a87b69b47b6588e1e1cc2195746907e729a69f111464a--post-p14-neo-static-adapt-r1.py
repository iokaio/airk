# SPDX-License-Identifier: Apache-2.0
"""One-time ignored static owner/launcher adaptation. No native/SSH execution."""
from pathlib import Path
import ast
a=Path(__file__).parent
s=(a/'p14-neo-owner.py').read_text(encoding='utf-8')
s=s.replace('import hashlib,json,os,re,subprocess,sys','import hashlib,json,os,re,subprocess,sys,signal,shutil')
s=s.replace("startswith('p14-native-')","startswith('post-p14-native-')")
s=s.replace('result=1; cleanup=1; timing_error=None','result=1; cleanup=1; timing_error=None; execution_error=None')
old="    result=subprocess.run([sys.executable,str(run/'runner.py'),*args],cwd=run).returncode"
new="""    assert shutil.disk_usage(run).free >= 8<<30, 'Require at least8GiB free; not a peak-disk claim'
    def deadline(signum,frame): raise TimeoutError('Owned post-P14 batch interrupted/deadline')
    signal.signal(signal.SIGTERM,deadline);signal.signal(signal.SIGALRM,deadline)
    child=subprocess.Popen([sys.executable,str(run/'runner.py'),*args],cwd=run,start_new_session=True)
    try: result=child.wait(timeout=7200)
    except BaseException as error:
        execution_error=repr(error)
        if child.poll() is None:
            os.killpg(child.pid,signal.SIGTERM)
            try: child.wait(timeout=10)
            except subprocess.TimeoutExpired: os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=10)
        result=1"""
assert old in s;s=s.replace(old,new)
s=s.replace("'test_exit_code':result,","'test_exit_code':result,'execution_error':execution_error,")
ast.parse(s);(a/'post-p14-neo-owner-r1.py').write_bytes(s.encode())
s=(a/'p14-neo-launch.py').read_text(encoding='utf-8')
s=s.replace('P14 Neo','post-P14 portable Neo').replace('p14-native-','post-p14-native-')
s=s.replace('p14-neo-collect-','post-p14-neo-collect-').replace('p14-neo-owner.py','post-p14-neo-owner-r1.py')
s=s.replace("['.tar.gz','-snapshot.json','-plan.json','-runner.py']", "['.tar.gz','-snapshot.json','-plan.json','-runner.py','-head.commit','-head.tree']")
needle="(base/'post-p14-neo-owner-r1.py','owner.py')]"
assert needle in s
s=s.replace(needle,"(base/'post-p14-neo-owner-r1.py','owner.py'),(base/(label+'-head.commit'),'head.commit'),(base/(label+'-head.tree'),'head.tree')]")
old="if action=='launch':"
new="""if action=='launch':
    prepared=json.loads((base/(label+'-prepared.json')).read_text(encoding='utf-8'))
    assert prepared['phase']=='PostP14Portable' and prepared['prepared_only'] and not prepared['ssh_launched']
    for path,expected in prepared['pins'].items(): assert hashlib.sha256(Path(path).read_bytes()).hexdigest()==expected,path"""
assert old in s;s=s.replace(old,new)
old="    assert cleanup['cargo_clean_exit_code']==0 and cleanup['target_removed']"
new="""    assert cleanup['cargo_clean_exit_code']==0 and cleanup['target_removed']
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
    assert sum(row.get('passing_tests') or 0 for row in steps)==30
    for row in steps:
        assert row['actual_exit_code']==row['exit_code']==0 and row['validation_error'] is None
        if 'expected_passing_tests' in planned[row['name']]:assert row['passing_tests']==planned[row['name']]['expected_passing_tests']"""
assert old in s;s=s.replace(old,new)
ast.parse(s);(a/'post-p14-neo-launch-r1.py').write_bytes(s.encode())
print('owner/launcher AST parsed; no native or SSH execution')
