# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import uuid

base = Path('D:/code/Github/airk/artifacts/v190')
leaf = sys.argv[1]
assert leaf == 'native-installed-yRHlt6'
fixture = base / 'p10-worktree/artifacts/editor-qualification' / leaf / 'usage'
install = fixture / 'native-service'
control = install / 'control'
binary = base / 'p14-runtime-final-r3/airk.exe'
runtime = 'f52d263e96154a5b675d4575df300c5e20e17ab9ba6433b7c670fe0798fd4f98'
main_sha = 'ae4bddf9debb2d0811021ca7aeb623ca775b78db563c1e8737518653e731b997'
assert install.resolve(strict=True).is_relative_to(fixture.resolve(strict=True))
assert hashlib.sha256(binary.read_bytes()).hexdigest() == main_sha
prefix = base / ('p14-editor-cleanup-' + leaf + '-r1')
assert not prefix.with_suffix('.json').exists()
rows = []
def invoke(args, payload=None, required=True):
    ordinal = len(rows) + 1
    result = subprocess.run([str(binary), *map(str, args)],
        input=json.dumps(payload) if payload is not None else None,
        capture_output=True, text=True, encoding='utf-8', timeout=180, creationflags=0x08000000)
    for suffix, body in [('stdout', result.stdout), ('stderr', result.stderr)]:
        with Path(str(prefix) + f'-{ordinal:02d}.{suffix}').open('x', encoding='utf-8') as output:
            output.write(body)
    rows.append({'command': [str(binary), *map(str, args)], 'exit_code': result.returncode})
    if required:
        assert result.returncode == 0, result.stderr[-4096:]
    return result
def parsed(args, payload=None):
    return json.loads(invoke(args, payload).stdout)
inspection = parsed(['runtime', 'inspect', '--install-dir', install])
assert Path(inspection['control_dir']).samefile(control)
assert inspection['approval']['manifest_sha256'] == runtime and inspection['payload_verified'] is True
health_run = invoke(['service', 'health', '--runtime-dir', control], required=False)
if health_run.returncode == 0:
    health = json.loads(health_run.stdout)
    observed = health['observation']
    assert health['status'] == 'health' and observed['runtime_sha256'] == runtime and observed['executable_sha256'] == main_sha
    assert observed['build']['source_sha256'] == '237f896933ad59a4075f9395fd53fcb92e376946faf1fc01036c139fc0533c0f'
    assert observed['catalog_clock']['namespace'] == inspection['registry_namespace']
    epoch = parsed(['service', 'rpc', '--runtime-dir', control, '--stdin'], 'request_epoch')
    assert epoch['namespace'] == inspection['registry_namespace']
    request = {'caller': uuid.uuid4().hex,
        'key': {'namespace': epoch['namespace'], 'epoch': epoch['epoch'], 'nonce': uuid.uuid4().hex},
        'action': 'stop', 'expected_instance': observed['instance'], 'runtime': runtime, 'force': False}
    proof = parsed(['runtime', 'lifecycle', '--install-dir', install], request)
    assert proof['status'] == 'lifecycle' and proof['record']['phase'] == 'complete' and proof['record']['request'] == {**{k:v for k,v in request.items() if k!='force'}, 'manager':False}
    kind = 'authenticated exact-instance no-force lifecycle Stop'
else:
    proof = parsed(['runtime', 'repair-launcher', '--install-dir', install])
    assert proof == {'launcher_repaired': True, 'runtime': runtime}
    kind = 'native all-role quiescence leases after unavailable health; no restart or Stop claim'
after = parsed(['runtime', 'inspect', '--install-dir', install])
assert Path(after['control_dir']).samefile(control) and after['payload_verified'] is True
assert after['approval']['manifest_sha256'] == runtime and after['lifecycle']['pending'] is None
if health_run.returncode == 0:
    assert after['lifecycle']['explicitly_stopped'] is True
with prefix.with_suffix('.json').open('x') as output:
    json.dump({'qualified': True, 'fixture': str(fixture), 'proof_kind': kind,
        'proof': proof, 'inspection': after, 'commands': rows,
        'ui_test': 'failed, preserved; no UI receipt fabricated or prior action replayed'}, output, indent=2)
print(json.dumps({'qualified': True, 'proof_kind': kind}))
