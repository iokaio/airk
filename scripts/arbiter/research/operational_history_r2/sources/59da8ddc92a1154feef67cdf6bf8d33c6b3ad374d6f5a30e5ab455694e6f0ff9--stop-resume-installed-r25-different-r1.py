# SPDX-License-Identifier: Apache-2.0
"""One new instance-fenced Stop for the failed, isolated r25 DIFFERENT fixture."""
import hashlib, importlib.util, json, os, sqlite3, subprocess, time, uuid
from pathlib import Path

root = Path(__file__).resolve().parents[3]
base = Path(__file__).resolve().parent
fixture = (root / 'artifacts/editor-qualification/native-installed-XW0Bay/different').resolve(strict=True)
install = fixture / 'native-service'
runtime = base / 'windows-debug-r25/runtime'
cli = runtime / 'airk.exe'
out = base / 'resume-installed-r25-different-recovered-stop-r1.json'
assert not out.exists(), 'Never replay this cleanup script'
assert fixture.is_relative_to(root / 'artifacts/editor-qualification')
assert install.resolve(strict=True).is_relative_to(fixture)
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
manifest = json.loads((runtime / 'airk-install.json').read_text())
assert sha(cli) == manifest['files']['airk.exe'] == '430e9ed73cbdfcb29c4f7f3c53542b6ae97facf6bddceec4fb5693fa878b4a2f'
active = sha(runtime / 'airk-install.json')
assert active == '830bee5ef28e2d7761ced8c8f1aea04ed25e8d5bb76aa696ff105808e51342d9'
payload = install / 'runtimes' / active / 'airk.exe'
assert sha(payload) == sha(cli)
expected_namespace = 'f1e63462c36b44ecdb82c44d0df0bcfd'
expected_instance = None
records = []
receipt = dict(schema=1, fixture=str(fixture), install_dir=str(install),
               original_attempt='resume-installed-editor-r25.json',
               original_attempt_remains_failed=True, qualified=False,
               runtime_sha256=active, executable_sha256=sha(cli), records=records,
               stop_dispatches=0)

def save():
    out.write_text(json.dumps(receipt, indent=2) + '\n')

def invoke(args, value=None):
    start = time.monotonic()
    result = subprocess.run([str(cli), *map(str, args)],
                            input=None if value is None else json.dumps(value).encode(),
                            capture_output=True, timeout=180,
                            creationflags=subprocess.CREATE_NO_WINDOW)
    assert len(result.stdout) + len(result.stderr) <= 2 * 1024 * 1024
    record = dict(arguments=list(map(str, args)), exit_code=result.returncode,
                  elapsed_seconds=time.monotonic()-start,
                  stdout=result.stdout.decode('utf-8', errors='replace'),
                  stderr=result.stderr.decode('utf-8', errors='replace'))
    records.append(record); save()
    assert result.returncode == 0, record
    return json.loads(record['stdout'])

def rpc(method):
    return invoke(['service', 'rpc', '--runtime-dir', install / 'control', '--stdin'], method)

def canonical(path):
    return os.path.normcase(os.path.abspath(str(path).removeprefix('\\\\?\\')))

def sources():
    return {p.relative_to(fixture).as_posix(): sha(p)
            for name in ['workspace-a', 'workspace-b', 'shared-a', 'shared-b']
            for p in (fixture / name).rglob('*') if p.is_file()}

def editor_observation():
    script = "$p=Get-CimInstance Win32_Process | Where-Object {$_.Name -eq 'Code.exe' -and $_.CommandLine -like '*native-installed-XW0Bay*'}; @($p | Select-Object ProcessId,ExecutablePath,CommandLine) | ConvertTo-Json -Depth 3 -Compress"
    result = subprocess.run(['powershell', '-NoProfile', '-Command', script], capture_output=True,
                            text=True, timeout=30, creationflags=subprocess.CREATE_NO_WINDOW)
    assert result.returncode == 0, result.stderr
    value = json.loads(result.stdout or '[]')
    return value if isinstance(value, list) else [value]

try:
    before = invoke(['runtime', 'inspect', '--install-dir', install])
    receipt['before'] = before
    assert before['payload_verified'] is True and before['approval']['manifest_sha256'] == active
    assert before['selection']['active'] == active and before['registry_namespace'] == expected_namespace
    assert canonical(before['control_dir']) == canonical(install / 'control')
    assert before['lifecycle']['pending'] is None and before['lifecycle']['explicitly_stopped'] is False
    expected_instance = before['lifecycle']['recorded_instance']
    assert isinstance(expected_instance,str) and len(expected_instance)==32 and all(c in '0123456789abcdef' for c in expected_instance)
    receipt['frozen_expected_instance'] = expected_instance
    assert not (fixture / 'native-stop.json').exists()
    with sqlite3.connect((install / 'control/service-1.4.sqlite3').as_uri() + '?mode=ro', uri=True) as database:
        rows = database.execute('SELECT id,request,phase FROM service_lifecycle').fetchall()
    receipt['pre_stop_lifecycle_record_count'] = len(rows)
    assert not rows, 'A lifecycle action already exists; do not submit another Stop'
    receipt['editors_before'] = editor_observation()
    assert receipt['editors_before'] == [], 'An exact fixture editor is still running'
    receipt['original_editor_exit_records'] = {side: json.loads((fixture / (side + '-editor.log.forced-cleanup.json')).read_text()) for side in ['a', 'b']}
    assert all(record['closed'] is True for record in receipt['original_editor_exit_records'].values())
    receipt['source_hashes_before'] = sources()
    assert len(receipt['source_hashes_before']) == 6
    specification = importlib.util.spec_from_file_location('exact_census', base / 'native_process_image_census.py')
    census = importlib.util.module_from_spec(specification); specification.loader.exec_module(census)
    receipt['owned_processes_before'] = census.census([payload])
    health = rpc('health')
    assert health['status'] == 'health'
    observed = health['observation']
    assert observed['instance'] == expected_instance and observed['runtime_sha256'] == active
    assert observed['executable_sha256'] == sha(payload) and observed['policy']['accepting'] is True
    assert observed['catalog_clock']['namespace'] == expected_namespace
    epoch = rpc('request_epoch')
    assert epoch['namespace'] == expected_namespace
    request = dict(caller=uuid.uuid4().hex,
                   key=dict(namespace=epoch['namespace'], epoch=epoch['epoch'], nonce=uuid.uuid4().hex),
                   action='stop', expected_instance=expected_instance, runtime=active, force=False)
    receipt['stop_request'] = request
    receipt['stop_dispatches'] = 1
    receipt['stop_dispatch_started_unix_ms'] = int(time.time()*1000)
    save()
    stopped = invoke(['runtime', 'lifecycle', '--install-dir', install], request)
    receipt['stop_response'] = stopped
    expected = {key: value for key, value in request.items() if key != 'force'}
    expected['manager'] = False
    assert stopped['status'] == 'lifecycle' and stopped['record']['phase'] == 'complete'
    assert stopped['record']['request'] == expected
    after = invoke(['runtime', 'inspect', '--install-dir', install])
    receipt['after'] = after
    assert after['payload_verified'] is True and after['registry_namespace'] == expected_namespace
    assert after['selection']['active'] == active
    assert after['lifecycle']['explicitly_stopped'] is True and after['lifecycle']['pending'] is None
    receipt['owned_processes_after'] = census.census([payload])
    assert receipt['owned_processes_after']['live_count'] == 0
    receipt['source_hashes_after'] = sources()
    assert receipt['source_hashes_after'] == receipt['source_hashes_before']
    receipt['editors_after'] = editor_observation()
    assert receipt['editors_after'] == []
    receipt['qualified'] = True
    receipt['qualification'] = 'Separate recovered native Stop; original editor attempt and missing UI Stop remain failed. No uninstall, forced termination, source deletion or ambient receipt cleanup.'
    save()
    print(json.dumps(dict(qualified=True, stop_dispatches=1, source_files_unchanged=6,
                          owned_visible_processes_after=0, evidence=str(out))))
except BaseException as error:
    receipt['error'] = repr(error)
    receipt['uncertain_stop_requires_original_key_read_only_reconciliation'] = receipt['stop_dispatches'] == 1 and 'stop_response' not in receipt
    save()
    raise
