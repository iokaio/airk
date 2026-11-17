# SPDX-License-Identifier: Apache-2.0
"""Owned P14 Mac build/sign/install qualification. No keychain changes or notarization."""
import hashlib, importlib.util, json, os, plistlib, re, shutil, signal, subprocess, sys, time, uuid
from pathlib import Path

RUN = Path(__file__).resolve().parent
SOURCE = RUN / 'source'
TARGET = RUN / 'target'
UNSIGNED = SOURCE / 'artifacts/p14-mac-release-r1'
SIGNED = SOURCE / 'artifacts/p14-mac-developerid-r1'
EXPECTED = {'sha256': '5d8912b117b410306ac9a3b44af6d96d70bb018006c2d49a1efa365597fe3e11', 'files': 960}
IDENTITY = 'Developer ID Application: Ioka LLC (5PX7D99PXA)'
TEAM = '5PX7D99PXA'

def read(p): return json.loads(Path(p).read_text(encoding='utf-8'))
def sha(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def save(p, value): Path(p).write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')
def module(name, p):
    spec = importlib.util.spec_from_file_location(name, p)
    value = importlib.util.module_from_spec(spec); spec.loader.exec_module(value); return value

def execute(command, log, env, timeout):
    """Kill only this owned child process group on timeout, before cleaning its target."""
    with Path(log).open('x', encoding='utf-8') as output:
        child = subprocess.Popen(command, cwd=SOURCE if SOURCE.is_dir() else RUN, env=env,
                                 stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
        try: code = child.wait(timeout=timeout)
        except BaseException:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                try: child.wait(timeout=10)
                except subprocess.TimeoutExpired: os.killpg(child.pid, signal.SIGKILL); child.wait(timeout=10)
            raise
    assert code == 0, f'Command failed({code}): {command}; log={log}'
    return {'command': command, 'exit_code': code, 'log': str(log), 'log_sha256': sha(log)}

def source_check():
    snapshot = read(RUN / 'snapshot.json')
    assert snapshot['native_source'] == EXPECTED
    for name, digest in snapshot['files'].items():
        path = SOURCE / name
        assert not path.is_symlink() and path.resolve().is_relative_to(SOURCE.resolve()) and sha(path) == digest, name
    builder = module('p14_signed_builder', SOURCE / 'scripts/build-service-runtime.py')
    assert builder.source_identity() == EXPECTED
    return snapshot

def seal(runtime):
    package = module('p14_signed_package', SOURCE / 'scripts/package-native.py')
    files, images = package.validate_runtime(runtime, '1.8.8', 'aarch64-apple-darwin', EXPECTED)
    build = read(runtime / 'airk-build.json')
    assert build['profile'] == 'release' and build['source'] == EXPECTED
    return {'path': str(runtime), 'files': files, 'images': list(images), 'build': build,
            'install_manifest_sha256': sha(runtime / 'airk-install.json')}

def gui_execute():
    assert os.getuid() == 501 == Path('/dev/console').stat().st_uid
    config = read(RUN / 'sign-config.json'); source_check()
    assert seal(UNSIGNED) == read(RUN / 'unsigned-seal.json')
    helper = Path(config['keychain_helper'])
    assert helper.is_file() and not helper.is_symlink() and helper.stat().st_uid == os.getuid()
    assert sha(helper) == config['keychain_helper_sha256']
    keys = [str(Path.home() / 'Library/Keychains/airk-signing.keychain-db'),
            str(Path.home() / 'Library/Keychains/login.keychain-db')]
    status = subprocess.run([str(helper), *keys], capture_output=True, text=True, timeout=15)
    (RUN / 'gui-keychain-metadata.log').write_text(status.stdout + status.stderr, encoding='utf-8')
    assert status.returncode == 0
    selected = next((key for key in keys if any(line.startswith('path=' + key + ' ') and 'unlocked=1' in line
                                                for line in status.stdout.splitlines())), None)
    assert selected, 'No existing keychain is already unlocked in GUI context; never unlock/change access'
    command = [sys.executable, str(SOURCE / 'scripts/sign-native.py'), '--runtime-dir', str(UNSIGNED),
               '--target', 'aarch64-apple-darwin', '--identity', IDENTITY, '--team-id', TEAM,
               '--keychain', selected, '--output', str(SIGNED)]
    record = {'status': 'failed', 'source': EXPECTED, 'selected_keychain': selected,
              'notarization_requested': False, 'command': command}
    try:
        record['sign'] = execute(command, RUN / 'canonical-sign.log', os.environ.copy(), 300)
        signer = module('p14_gui_signer', SOURCE / 'scripts/sign-native.py')
        signed = read(SIGNED / 'signing.json')
        signer.read_verified_pair(SIGNED, UNSIGNED, 'aarch64-apple-darwin', signed['commit'], sha(SOURCE / 'Cargo.lock'))
        assert all(c['notarization'] is None and c['identity'] == IDENTITY and c['team_id'] == TEAM
                   for c in signed['components'].values())
        assert signed['copied_launcher']['strict_signature'] and signed['copied_launcher']['standalone_version_probe']
        assert signed['copied_launcher']['version'] == '1.8.8'
        source_check(); assert seal(UNSIGNED) == read(RUN / 'unsigned-seal.json')
        record.update(status='passed', signing_receipt_sha256=sha(SIGNED / 'signing.json'), copied_launcher=signed['copied_launcher'])
    except BaseException as error:
        record['failure'] = repr(error); raise
    finally: save(RUN / 'gui-signing.json', record)

def gui_controller(env):
    uid = os.getuid(); assert uid == 501 == Path('/dev/console').stat().st_uid
    label = 'io.iokaio.airk.p14-test-sign-' + uuid.uuid4().hex
    service = f'gui/{uid}/{label}'
    plist = RUN / 'sign-job.plist'
    payload = {'Label': label, 'ProgramArguments': [sys.executable, str(Path(__file__).resolve()), 'gui-execute'],
               'WorkingDirectory': str(SOURCE), 'RunAtLoad': True, 'KeepAlive': False,
               'LimitLoadToSessionType': 'Aqua', 'ProcessType': 'Background',
               'StandardOutPath': str(RUN / 'gui-sign.stdout'), 'StandardErrorPath': str(RUN / 'gui-sign.stderr'),
               'EnvironmentVariables': {key: value for key, value in env.items() if key in
                   ['PATH', 'HOME', 'TMPDIR', 'PYTHONUTF8', 'CARGO_BUILD_JOBS', 'CARGO_INCREMENTAL',
                    'CARGO_NET_OFFLINE', 'CARGO_TARGET_DIR', 'MACOSX_DEPLOYMENT_TARGET', 'GIT_CONFIG_GLOBAL',
                    'AIRK_BUNDLED_MODEL_DIR', 'AIRK_BUNDLED_RUNTIME_DIR', 'AIRK_TEST_MODEL_DIR', 'AIRK_PRIVATE_CENSUS_BASE']}}
    payload['EnvironmentVariables']['HOME'] = str(Path.home())
    with os.fdopen(os.open(plist, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'wb') as output: plistlib.dump(payload, output)
    record = {'service': service, 'label': label, 'uid': uid, 'cleanup_verified': False}
    save(RUN / 'gui-job.json', record)
    try:
        result = subprocess.run(['/bin/launchctl', 'bootstrap', f'gui/{uid}', str(plist)], capture_output=True, text=True, timeout=15)
        record.update(bootstrap_exit=result.returncode, bootstrap_stdout=result.stdout, bootstrap_stderr=result.stderr)
        assert result.returncode == 0
        deadline = time.monotonic() + 330
        while not (RUN / 'gui-signing.json').exists():
            assert time.monotonic() < deadline, 'Owned GUI signing deadline exceeded'
            time.sleep(.5)
        signed = read(RUN / 'gui-signing.json'); assert signed['status'] == 'passed', signed
    finally:
        stopped = subprocess.run(['/bin/launchctl', 'bootout', service], capture_output=True, text=True, timeout=15)
        remaining = subprocess.run(['/bin/launchctl', 'print', service], capture_output=True, text=True, timeout=15)
        record.update(bootout_exit=stopped.returncode, remaining_job_exit=remaining.returncode,
                      cleanup_verified=stopped.returncode == 0 and remaining.returncode != 0)
        save(RUN / 'gui-job.json', record)
        assert record['cleanup_verified'], 'Exact GUI job cleanup incomplete; preserve plist'
        plist.unlink()

def cleanup(phase, env):
    assert RUN.parent == Path.home() / 'airk-builds' and re.fullmatch(r'p14-native-signed-quota-[a-z0-9-]+', RUN.name)
    assert SOURCE.resolve().parent == RUN and TARGET.resolve().parent == RUN and not TARGET.is_symlink()
    timing_error = None; timings = []
    try:
        saved = RUN / 'cargo-timings'; saved.mkdir(mode=0o700, exist_ok=True)
        for path in sorted((TARGET / 'cargo-timings').glob('*.html')):
            assert path.is_file() and not path.is_symlink() and path.stat().st_size <= 64 << 20
            dest = saved / path.name
            if dest.exists(): assert sha(dest) == sha(path)
            else:
                with dest.open('xb') as output: output.write(path.read_bytes())
            timings.append({'name': path.name, 'sha256': sha(path), 'bytes': path.stat().st_size})
    except BaseException as error: timing_error = repr(error)
    finally:
        # Disk-full inability to write logs/metadata must never skip Cargo cleanup.
        log_error = None
        try: stream = (RUN / ('cargo-clean-' + phase + '.log')).open('x', encoding='utf-8')
        except OSError as error: stream = None; log_error = repr(error)
        try:
            result = subprocess.run(['cargo', 'clean', '--manifest-path', str(SOURCE / 'Cargo.toml'), '--target-dir', str(TARGET)],
                                    cwd=RUN, env=env, stdout=stream or sys.stderr, stderr=subprocess.STDOUT, timeout=180)
        finally:
            if stream: stream.close()
        record = {'phase': phase, 'cargo_clean_exit_code': result.returncode, 'target': str(TARGET),
                  'target_removed': not TARGET.exists(), 'source_preserved': SOURCE.is_dir(),
                  'timing_copy_error': timing_error, 'clean_log_error': log_error, 'timings': timings,
                  'disk_free_bytes_after': shutil.disk_usage(RUN).free}
        save(RUN / ('cleanup-' + phase + '.json'), record)
        assert result.returncode == 0 and record['target_removed'] and timing_error is None, record
        return record

def main(mode='full'):
    os.umask(0o077)
    assert RUN.parent == Path.home() / 'airk-builds' and RUN.name.startswith('p14-native-signed-quota-')
    config = read(RUN / 'sign-config.json')
    for name, digest in config['transferred_hashes'].items(): assert sha(RUN / name) == digest
    env = {**os.environ, **config['environment']}
    env['PATH'] = ':'.join(config['path_prefix'] + [os.environ.get('PATH', '')])
    for key in ['AIRK_TEST_BINARY', 'AIRK_CATALOG_CANDIDATE_BINARY', 'AIRK_P10_RESOURCE_OUTPUT_DIR', 'AIRK_P11B_RESOURCE_OUTPUT_DIR']:
        env.pop(key, None)
    os.environ.update(env)
    record = {'status': 'failed', 'source': EXPECTED, 'notarization_requested': False, 'steps': [],
              'disk_free_bytes_before': shutil.disk_usage(RUN).free}
    result = 1
    def deadline(_signum, _frame): raise TimeoutError('Whole owned signed batch exceeded 7200 seconds')
    signal.signal(signal.SIGALRM, deadline); signal.alarm(7200)
    try:
        assert record['disk_free_bytes_before'] >= config['minimum_free_bytes']
        assert not TARGET.exists()
        if mode == 'sign-existing':
            prior = read(RUN / 'unsigned-qualification.json')
            assert prior['status'] == 'passed' and prior['source'] == EXPECTED and prior['target_removed']
        else:
            assert not SOURCE.exists()
            plan = read(RUN / 'plan.json')
            try:
                command = [sys.executable, str(RUN / 'runner.py'), *plan['runner_arguments']]
                record['steps'].append(execute(command, RUN / 'build-owner.log', env, 5450))
            finally: record['after_build_cleanup'] = cleanup('after-build', env)
        os.environ['GIT_CONFIG_GLOBAL'] = str(RUN / 'private-census/owned-ignore-controls/global.gitconfig')
        source_check(); unsigned = seal(UNSIGNED); save(RUN / 'unsigned-seal.json', unsigned)
        if mode == 'build-only':
            record.update(status='passed', qualification_scope='Current candidate five native tests + unsigned release producer ONLY; no signing/installed proof',
                          unsigned_install_manifest_sha256=unsigned['install_manifest_sha256'])
            result = 0
        else:
            assert shutil.disk_usage(RUN).free >= max(1 << 30, 3 * sum((UNSIGNED / n).stat().st_size for n in unsigned['files']) + (256 << 20))
            gui_controller(env)
            source_check(); assert seal(UNSIGNED) == unsigned
            signer = module('p14_final_signer', SOURCE / 'scripts/sign-native.py'); signing = read(SIGNED / 'signing.json')
            signer.read_verified_pair(SIGNED, UNSIGNED, 'aarch64-apple-darwin', signing['commit'], sha(SOURCE / 'Cargo.lock'))
            signed = seal(SIGNED / 'runtime'); save(RUN / 'signed-seal.json', signed)
            receipt_hash = sha(SIGNED / 'signing.json')
            command = [sys.executable, str(SOURCE / 'scripts/check-service-install.py'), '--runtime', str(SIGNED / 'runtime')]
            record['steps'].append(execute(command, RUN / 'signed-install.log', env, 600))
            assert 'Offline runtime staging, election, exact payload health, upgrade, rollback, stable launcher repair, and resumable uninstall passed.' in (RUN / 'signed-install.log').read_text()
            source_check(); assert seal(UNSIGNED) == unsigned and seal(SIGNED / 'runtime') == signed
            assert sha(SIGNED / 'signing.json') == receipt_hash
            record.update(status='passed', signing_receipt_sha256=receipt_hash, copied_launcher=signing['copied_launcher'],
                          signed_install_manifest_sha256=signed['install_manifest_sha256'], original_commit=read(RUN / 'snapshot.json')['entry_commit'])
            result = 0
    except BaseException as error: record['failure'] = repr(error)
    finally:
        signal.alarm(0)  # Cleanup always gets its own bounded budget, even after the batch deadline.
        try: record['final_cleanup'] = cleanup('build-final' if mode == 'build-only' else 'sign-final', env)
        except BaseException as error: result = 1; record.update(status='failed', cleanup_failure=repr(error))
        record['source_preserved'] = SOURCE.is_dir()
        record['target_removed'] = not TARGET.exists()
        save(RUN / ('unsigned-qualification.json' if mode == 'build-only' else 'signed-qualification.json'), record)
        print(json.dumps(record), flush=True)
    return result

if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == 'gui-execute': gui_execute()
    else: raise SystemExit(main(sys.argv[1] if len(sys.argv) > 1 else 'full'))
