# SPDX-License-Identifier: Apache-2.0
"""Remote owner: always clean the exact batch target, including runner failures."""
import hashlib,json,os,re,subprocess,sys
from pathlib import Path
run=Path(__file__).resolve().parent
assert run.parent==Path.home()/'airk-builds' and run.name.startswith('p12-native-')
source=run/'source'; target=run/'target'
result=1; cleanup=1; timing_error=None
try:
    args=json.loads((run/'plan.json').read_text(encoding='utf-8'))['runner_arguments']
    result=subprocess.run([sys.executable,str(run/'runner.py'),*args],cwd=run).returncode
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
                cleanup=subprocess.run(['cargo','clean','--manifest-path',str(source/'Cargo.toml'),'--target-dir',str(target)],env=env,cwd=run,stdout=log if log is not None else sys.stderr,stderr=subprocess.STDOUT).returncode
            finally:
                if log is not None: log.close()
            if clean_log_error is not None:
                # After successful cleanup there may now be room to preserve it.
                try:
                    (run/'cargo-clean.log').write_text('Log creation failed before cleanup; Cargo output used owner stderr. '+clean_log_error+'\n',encoding='utf-8')
                except OSError:
                    pass
    removed=not target.exists()
    (run/'cleanup.json').write_text(json.dumps({'test_exit_code':result,'cargo_clean_exit_code':cleanup,'target':str(target),
        'target_removed':removed,'source_preserved':source.is_dir(),'clean_log_error':clean_log_error,'timing_copy_error':timing_error,'reports_preserved':sorted(p.name for p in run.glob('*.json'))})+'\n',encoding='utf-8')
    print('CARGO_CLEAN_EXIT',cleanup,'TARGET_REMOVED',removed,flush=True)
sys.exit(result or cleanup or (0 if removed and timing_error is None else 1))
