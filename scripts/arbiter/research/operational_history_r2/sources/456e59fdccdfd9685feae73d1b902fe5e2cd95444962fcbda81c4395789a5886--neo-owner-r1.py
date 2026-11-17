# SPDX-License-Identifier: Apache-2.0
"""Remote owner: always clean the exact batch target, including runner failures."""
import hashlib,json,os,re,subprocess,sys,signal,shutil,time
from pathlib import Path
run=Path(__file__).resolve().parent
assert run.parent==Path.home()/'airk-builds' and run.name.startswith('branch-review-native-')
source=run/'source'; target=run/'target'
result=1; cleanup=1; timing_error=None; execution_error=None
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
    return wait_group_absent(child,20)
try:
    args=json.loads((run/'plan.json').read_text(encoding='utf-8'))['runner_arguments']
    assert shutil.disk_usage(run).free >= 8<<30, 'Require at least8GiB free; not a peak-disk claim'
    def deadline(signum,frame): raise TimeoutError('Owned branch-review batch interrupted/deadline')
    signal.signal(signal.SIGTERM,deadline);signal.signal(signal.SIGALRM,deadline)
    child=subprocess.Popen([sys.executable,str(run/'runner.py'),*args],cwd=run,start_new_session=True)
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
            group_absent=False;drain_error=repr(error);result=1
finally:
    assert source.resolve().parent==run and target.resolve().parent==run
    env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+':'+os.environ.get('PATH',''))
    timings=[]
    try:
        saved=run/'cargo-timings'; saved.mkdir(mode=0o700)
        timing_source=target/'cargo-timings'
        assert not timing_source.is_symlink() and timing_source.resolve().parent==target
        for path in sorted(timing_source.glob('*.html')):
            assert not path.is_symlink() and path.is_file() and re.fullmatch(r'[A-Za-z0-9._-]+[.]html',path.name)
            assert path.stat().st_size <= 64<<20
            content=path.read_bytes()
            with (saved/path.name).open('xb') as output: output.write(content)
            timings.append({'name':path.name,'sha256':hashlib.sha256(content).hexdigest(),'bytes':len(content)})
    except Exception as error:
        timing_error=repr(error)
    finally:
        try:
            (run/'timings.json').write_text(json.dumps({'files':timings,'error':timing_error})+'\n',encoding='utf-8')
        finally:
            # The drive can be full precisely when cleanup is needed. Failure
            # to create another log must not prevent Cargo from reclaiming space.
            clean_log_error=None
            try:
                log=(run/'cargo-clean.log').open('x',encoding='utf-8')
            except OSError as error:
                clean_log_error=repr(error)
                log=None
            try:
                if group_absent:
                    cleanup_attempted=True
                    cleanup=subprocess.run(['cargo','clean','--manifest-path',str(source/'Cargo.toml'),'--target-dir',str(target)],env=env,cwd=run,stdout=log if log is not None else sys.stderr,stderr=subprocess.STDOUT).returncode
                else:
                    # Do not race Cargo with an unproven live owned group. This is
                    # failed containment/unknown cleanup, never an invented exit.
                    cleanup=None
                    print('CARGO_CLEAN_NOT_ATTEMPTED_OWNED_GROUP_NOT_ABSENT',file=sys.stderr,flush=True)
            finally:
                if log is not None: log.close()
            if clean_log_error is not None:
                # After successful cleanup there may now be room to preserve it.
                try:
                    (run/'cargo-clean.log').write_text('Log creation failed before cleanup; Cargo output used owner stderr. '+clean_log_error+'\n',encoding='utf-8')
                except OSError:
                    pass
    removed=not target.exists()
    (run/'cleanup.json').write_text(json.dumps({'test_exit_code':result,'execution_error':execution_error,'process_group':group,'group_absent':group_absent,'drain_error':drain_error,'cleanup_attempted':cleanup_attempted,'cargo_clean_exit_code':cleanup,'target':str(target),
        'target_removed':removed,'source_preserved':source.is_dir(),'containment_scope':'Exact created runner/Cargo group only; native fixture roles may own separate groups','fixture_teardown':'unknown_after_interruption' if execution_error is not None else 'ordinary test return, no global process census','clean_log_error':clean_log_error,'timing_copy_error':timing_error,'reports_preserved':sorted(p.name for p in run.glob('*.json'))})+'\n',encoding='utf-8')
    print('CARGO_CLEAN_EXIT',cleanup,'TARGET_REMOVED',removed,flush=True)
sys.exit(result or cleanup or (0 if removed and timing_error is None else 1))
