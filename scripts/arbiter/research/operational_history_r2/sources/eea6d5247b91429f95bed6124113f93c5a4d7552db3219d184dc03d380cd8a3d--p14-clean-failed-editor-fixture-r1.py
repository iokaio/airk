# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path
import subprocess
import uuid

base = Path('D:/code/Github/airk/artifacts/v190')
fixture = base / 'p10-worktree/artifacts/editor-qualification/native-installed-z38fCa/same'
install = fixture / 'native-service'
control = install / 'control'
binary = base / 'p14-runtime-final-r2/airk.exe'
runtime = '45da6d3a10b34313dea53881ce95a930f89aa304f019ba095406eb40b852085c'
instance = 'ba6b8cdfd1831cbe7e6c4040cccb21f6'
assert install.resolve(strict=True).is_relative_to(fixture.resolve(strict=True))
assert hashlib.sha256(binary.read_bytes()).hexdigest() == 'b6b4ed967a13888ed238cf9111da4158821df5acff6993b37334ea0d5ef987d8'
assert not (base / 'p14-editor-failed-fixture-cleanup-r1.json').exists()
rows = []
def command(*args, payload=None):
    number = len(rows) + 1
    result = subprocess.run([str(binary), *map(str, args)],
        input=json.dumps(payload) if payload is not None else None,
        capture_output=True, text=True, encoding='utf-8', timeout=180,
        creationflags=0x08000000)
    for suffix, content in [('stdout', result.stdout), ('stderr', result.stderr)]:
        with (base / f'p14-editor-failed-cleanup-{number:02d}.{suffix}').open('x', encoding='utf-8') as output:
            output.write(content)
    rows.append({'command': [str(binary), *map(str, args)], 'exit_code': result.returncode})
    assert result.returncode == 0, result.stderr[-4096:]
    return json.loads(result.stdout)
health = command('service', 'health', '--runtime-dir', control)
observation = health['observation']
assert health['status'] == 'health' and observation['instance'] == instance
assert observation['runtime_sha256'] == runtime
assert observation['executable_sha256'] == hashlib.sha256(binary.read_bytes()).hexdigest()
assert observation['build']['source_sha256'] == '5d8912b117b410306ac9a3b44af6d96d70bb018006c2d49a1efa365597fe3e11'
epoch = command('service', 'rpc', '--runtime-dir', control, '--stdin', payload='request_epoch')
assert epoch['namespace'] == 'a007ecd0e1b3cba776ff5d9d021adabf'
request = {'caller': uuid.uuid4().hex,
           'key': {'namespace': epoch['namespace'], 'epoch': epoch['epoch'], 'nonce': uuid.uuid4().hex},
           'action': 'stop', 'expected_instance': instance, 'runtime': runtime, 'force': False}
stop = command('runtime', 'lifecycle', '--install-dir', install, payload=request)
assert stop['status'] == 'lifecycle' and stop['record']['phase'] == 'complete'
assert stop['record']['request'] == request
inspection = command('runtime', 'inspect', '--install-dir', install)
assert Path(inspection['control_dir']).samefile(control)
assert inspection['approval']['manifest_sha256'] == runtime and inspection['payload_verified'] is True
assert inspection['lifecycle']['explicitly_stopped'] is True and inspection['lifecycle']['pending'] is None
result = {'qualified': True, 'scope': 'separate native cleanup of exact failed synthetic editor fixture',
          'fixture': str(fixture), 'health': health, 'request': request, 'stop': stop,
          'inspection': inspection, 'commands': rows,
          'ui_test_outcome': 'failed, preserved; no fabricated UI native-stop.json or mutation replay'}
with (base / 'p14-editor-failed-fixture-cleanup-r1.json').open('x') as output:
    json.dump(result, output, indent=2)
print(json.dumps({'qualified': True, 'stop_phase': stop['record']['phase'],
                  'explicitly_stopped': inspection['lifecycle']['explicitly_stopped']}))
