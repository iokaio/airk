# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import uuid

base = Path('D:/code/Github/airk/artifacts/v190')
leaf = sys.argv[1]
assert leaf == 'native-installed-QvepgF'
fixture = base / 'p10-worktree/artifacts/editor-qualification' / leaf / 'usage'
install = fixture / 'native-service'
control = install / 'control'
binary = base / 'p14-runtime-final-r2/airk.exe'
runtime = '45da6d3a10b34313dea53881ce95a930f89aa304f019ba095406eb40b852085c'
main_sha = 'b6b4ed967a13888ed238cf9111da4158821df5acff6993b37334ea0d5ef987d8'
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
    assert observed['build']['source_sha256'] == '5d8912b117b410306ac9a3b44af6d96d70bb018006c2d49a1efa365597fe3e11'
    assert observed['catalog_clock']['namespace'] == inspection['registry_namespace']
    epoch = parsed(['service', 'rpc', '--runtime-dir', control, '--stdin'], 'request_epoch')
    assert epoch['namespace'] == inspection['registry_namespace']
    request = {'caller': uuid.uuid4().hex,
        'key': {'namespace': epoch['namespace'], 'epoch': epoch['epoch'], 'nonce': uuid.uuid4().hex},
        'action': 'stop', 'expected_instance': observed['instance'], 'runtime': runtime, 'force': False}
    proof = parsed(['runtime', 'lifecycle', '--install-dir', install], request)
    assert proof['status'] == 'lifecycle' and proof['record']['phase'] == 'complete' and proof['record']['request'] == request
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
