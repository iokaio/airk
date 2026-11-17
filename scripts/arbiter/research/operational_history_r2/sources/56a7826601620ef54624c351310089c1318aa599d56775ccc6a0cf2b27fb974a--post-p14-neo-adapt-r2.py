# SPDX-License-Identifier: Apache-2.0
"""Static correction only; preserve R1 and never launch archive/native/SSH work."""
from pathlib import Path
import ast
A=Path(__file__).parent
s=(A/'post-p14-neo-owner-r1.py').read_text(encoding='utf-8')
s=s.replace('subprocess,sys,signal,shutil','subprocess,sys,signal,shutil,time')
s=s.replace('result=1; cleanup=1; timing_error=None; execution_error=None', '''result=1; cleanup=1; timing_error=None; execution_error=None
group=None; group_absent=True; drain_error=None; cleanup_attempted=False
def group_exists():
    if group is None:return False
    try:os.killpg(group,0);return True
    except ProcessLookupError:return False
def wait_group_absent(child,seconds):
    until=time.monotonic()+seconds
    while True:
        # Do not reap the leader before the final possible group signal: its
        # unreaped PID pins the created PGID against reuse while draining.
        if not group_exists():return True
        if time.monotonic()>=until:return False
        time.sleep(0.05)
def drain_group(child):
    if not group_exists():return True
    if child.returncode is not None:return False # Reaped leader: no later group signals.
    try:os.killpg(group,signal.SIGTERM)
    except ProcessLookupError:return True
    if not wait_group_absent(child,10):
        # A zombie leader still pins this owned PGID. KILL never depends on poll().
        try:os.killpg(group,signal.SIGKILL)
        except ProcessLookupError:pass
    child.wait(timeout=10) # Reap only AFTER the final possible group signal.
    # Absence observation only; never signal a group after losing the PID pin.
    return wait_group_absent(child,20)''')
old='''    child=subprocess.Popen([sys.executable,str(run/'runner.py'),*args],cwd=run,start_new_session=True)
    try: result=child.wait(timeout=7200)
    except BaseException as error:
        execution_error=repr(error)
        if child.poll() is None:
            os.killpg(child.pid,signal.SIGTERM)
            try: child.wait(timeout=10)
            except subprocess.TimeoutExpired: os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=10)
        result=1'''
new='''    child=subprocess.Popen([sys.executable,str(run/'runner.py'),*args],cwd=run,start_new_session=True)
    group=child.pid;group_absent=False
    try: result=child.wait(timeout=7200)
    except BaseException as error:
        execution_error=repr(error);result=1
    finally:
        # Preserve cleanup even if an operator repeats interruption during drain.
        signal.signal(signal.SIGTERM,signal.SIG_IGN);signal.signal(signal.SIGALRM,signal.SIG_IGN)
        try:
            group_absent=not group_exists()
            if not group_absent:
                if result==0:
                    execution_error='Runner exited but its owned process group remained';result=1
                if child.returncode is not None:
                    drain_error='Leader already reaped and group still present; ownership/absence unknown, no later signals'
                else:group_absent=drain_group(child)
            if not group_absent:
                drain_error=drain_error or 'Exact owned process group remained after TERM/KILL deadlines';result=1
        except BaseException as error:
            group_absent=False;drain_error=repr(error);result=1'''
assert old in s;s=s.replace(old,new)
old="                cleanup=subprocess.run(['cargo','clean','--manifest-path',str(source/'Cargo.toml'),'--target-dir',str(target)],env=env,cwd=run,stdout=log if log is not None else sys.stderr,stderr=subprocess.STDOUT).returncode"
new="""                if group_absent:
                    cleanup_attempted=True
                    cleanup=subprocess.run(['cargo','clean','--manifest-path',str(source/'Cargo.toml'),'--target-dir',str(target)],env=env,cwd=run,stdout=log if log is not None else sys.stderr,stderr=subprocess.STDOUT).returncode
                else:
                    # Do not race Cargo with an unproven live owned group. This is
                    # failed containment/unknown cleanup, never an invented exit.
                    cleanup=None
                    print('CARGO_CLEAN_NOT_ATTEMPTED_OWNED_GROUP_NOT_ABSENT',file=sys.stderr,flush=True)"""
assert old in s;s=s.replace(old,new)
s=s.replace("'execution_error':execution_error,", "'execution_error':execution_error,'process_group':group,'group_absent':group_absent,'drain_error':drain_error,'cleanup_attempted':cleanup_attempted,")
s=s.replace("'source_preserved':source.is_dir(),", "'source_preserved':source.is_dir(),'containment_scope':'Exact created runner/Cargo group only; native fixture roles may own separate groups','fixture_teardown':'unknown_after_interruption' if execution_error is not None else 'ordinary test return, no global process census',")
ast.parse(s);(A/'post-p14-neo-owner-r2.py').write_bytes(s.encode())
s=(A/'post-p14-neo-prepare-r1.py').read_text(encoding='utf-8')
s=s.replace('post-p14-neo-owner-r1.py','post-p14-neo-owner-r2.py').replace('post-p14-neo-launch-r1.py','post-p14-neo-launch-r2.py')
ast.parse(s);(A/'post-p14-neo-prepare-r2.py').write_bytes(s.encode())
s=(A/'post-p14-neo-launch-r1.py').read_text(encoding='utf-8')
s=s.replace('post-p14-neo-owner-r1.py','post-p14-neo-owner-r2.py')
old="    assert cleanup['cargo_clean_exit_code']==0 and cleanup['target_removed']"
new="""    # All available logs/reports/timings have been retained before refusing a run.
    assert cleanup['test_exit_code']==0 and cleanup['execution_error'] is None
    assert cleanup['group_absent'] is True and cleanup['drain_error'] is None
    assert cleanup['cleanup_attempted'] is True
    assert timings['error'] is None and cleanup['timing_copy_error'] is None
    assert cleanup['cargo_clean_exit_code']==0 and cleanup['target_removed']"""
assert old in s;s=s.replace(old,new)
ast.parse(s);(A/'post-p14-neo-launch-r2.py').write_bytes(s.encode())
print('R2 AST parsed; R1 preserved; no native/SSH/archive execution')
