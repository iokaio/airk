# SPDX-License-Identifier: Apache-2.0
"""T2 probe: fresh install registers login startup, runs without a console
window, survives the on-demand idle window, and replacement removes it.
Installs only into its owned fixture folder; isolated state via --state-dir."""
import argparse, ctypes, ctypes.wintypes as w, hashlib, json, os, subprocess, sys, time, uuid
from pathlib import Path

p = argparse.ArgumentParser(); p.add_argument('--runtime', type=Path, required=True); p.add_argument('--idle-wait', type=int, default=200)
a = p.parse_args()
payload = a.runtime.resolve(strict=True); binary = payload / 'airk.exe'
identity = hashlib.sha256((payload / 'airk-install.json').read_bytes()).hexdigest()
fixture = Path(__file__).resolve().parent / f'login-fixture-{uuid.uuid4().hex[:8]}'; fixture.mkdir()
install, state = fixture / 'install', fixture / 'service-state'
env = dict(os.environ, HTTP_PROXY='http://127.0.0.1:1', HTTPS_PROXY='http://127.0.0.1:1', ALL_PROXY='http://127.0.0.1:1')
steps = []
def log(name, value): steps.append({'step': name, 'value': value}); print(json.dumps({'step': name, 'value': value}), flush=True)
def run(*args, timeout=600, ok=True):
    r = subprocess.run([str(binary), 'runtime', *map(str, args)], capture_output=True, encoding='utf-8', env=env, timeout=timeout, creationflags=0x08000000)
    if ok and r.returncode: raise RuntimeError(f'{args[0]} failed: {r.stderr[-4000:]}')
    return json.loads(r.stdout) if r.stdout.strip() else {'returncode': r.returncode, 'stderr': r.stderr[-2000:]}
def task_query(task):
    r = subprocess.run(['schtasks', '/Query', '/TN', task, '/FO', 'LIST', '/V'], capture_output=True, encoding='utf-8', errors='replace', creationflags=0x08000000)
    return r.returncode, r.stdout
def serve_processes():
    out = subprocess.run(['powershell', '-NoProfile', '-Command', "Get-CimInstance Win32_Process -Filter \"Name='airk.exe'\" | ForEach-Object { '{0}|{1}' -f $_.ProcessId,$_.CommandLine }"], capture_output=True, encoding='utf-8', creationflags=0x08000000).stdout
    rows = [line.split('|', 1) for line in out.splitlines() if '|' in line]
    return [int(pid) for pid, cmd in rows if 'managed-serve' in cmd and str(install) in cmd]
def visible_windows_mentioning(text):
    user32 = ctypes.windll.user32; found = []
    @ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
    def each(hwnd, _):
        if user32.IsWindowVisible(hwnd):
            buf = ctypes.create_unicode_buffer(1024); user32.GetWindowTextW(hwnd, buf, 1024)
            if text.lower() in buf.value.lower() or 'managed-serve' in buf.value: found.append(buf.value)
        return True
    user32.EnumWindows(each, 0); return found
result = {'schema': 'airk-login-probe/1', 'runtime_identity': identity, 'fixture': str(fixture)}
try:
    run('stage', '--source', payload, '--install-dir', install, '--approved-sha256', identity, '--verification-basis', 'trusted-distribution', '--state-dir', state)
    run('activate', '--install-dir', install, '--runtime-sha256', identity)
    status = run('login', '--install-dir', install, 'install'); log('login_install', status)
    task = status['id']; assert status['installed'] is True, 'registration not owned'
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline and not serve_processes(): time.sleep(0.5)
    pids = serve_processes(); log('managed_serve_pids', pids); assert len(pids) == 1, 'task did not start exactly one managed-serve'
    time.sleep(3)
    windows = visible_windows_mentioning(str(binary.name)) + visible_windows_mentioning(str(install)); log('visible_windows', windows)
    assert not [x for x in windows if 'managed-serve' in x or str(install).lower() in x.lower()], 'sign-in service has a visible console window'
    health = run('ensure', '--install-dir', install); log('ensure', {k: health.get(k) for k in ('instance', 'runtime_sha256', 'uptime_ms')})
    assert health['runtime_sha256'] == identity
    snap = run('snapshot', '--install-dir', install); log('startup_manager', snap.get('startup_manager'))
    assert snap['startup_manager']['installed'] is True and snap['startup_manager']['running'] is True
    log('idle_wait_seconds', a.idle_wait); time.sleep(a.idle_wait)
    after = run('ensure', '--install-dir', install); log('ensure_after_idle', {k: after.get(k) for k in ('instance', 'uptime_ms')})
    assert after['instance'] == health['instance'], 'service exited while idle under the login manager'
    assert serve_processes() == pids, 'managed-serve process changed'
    events = [r.get('kind') for r in snap.get('logs', {}).get('records', [])]
    events = [r.get('kind') for r in run('snapshot', '--install-dir', install).get('logs', {}).get('records', [])]; log('service_events', events)
    assert 'idle_exit' not in events
    rc, text = task_query(task); log('task_present_before_replacement', rc == 0)
    replaced = run('replace-installation', '--install-dir', install, '--candidate-sha256', 'b' * 64, '--apply'); log('replacement', {k: replaced.get(k) for k in ('replaced', 'removed', 'residue', 'startup_registration_removed')})
    assert replaced['replaced'] is True and replaced['startup_registration_removed'] is not None
    rc, _ = task_query(task); log('task_present_after_replacement', rc == 0); assert rc != 0, 'replacement left the login task registered'
    assert not install.exists() and not state.exists(), 'replacement left fixture folders'
    definition = Path(status['definition_path']); log('definition_present_after', definition.exists()); assert not definition.exists()
    result['outcome'] = 'pass'
except Exception as error:
    result['outcome'] = 'fail'; result['error'] = f'{type(error).__name__}: {error}'
    # Owned cleanup only: remove this fixture's registration if it still exists.
    try:
        if install.exists(): log('cleanup_uninstall', run('uninstall', '--install-dir', install, ok=False))
    except Exception as cleanup: log('cleanup_error', str(cleanup))
result['steps'] = steps
(Path(__file__).resolve().parent / 'login-probe-result.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({'outcome': result['outcome'], 'error': result.get('error')}))
sys.exit(0 if result['outcome'] == 'pass' else 1)
