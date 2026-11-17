# SPDX-License-Identifier: Apache-2.0
"""Explicit one-off Developer ID test; no keychain modification or notarization."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import subprocess
import sys
import time
import uuid

parent = Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh').resolve(strict=True)
checkout = parent / 'source-r2'
source = checkout / 'artifacts/p6a-r4-runtime'
nonce = sys.argv[2] if len(sys.argv) > 2 else uuid.uuid4().hex
assert len(nonce) == 32 and all(c in '0123456789abcdef' for c in nonce)
receipt_path = parent / f'artifacts/p6a-gui-sign-{nonce}.json'
output = checkout / f'artifacts/p6a-r4-gui-signed-{nonce}'

def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def execute():
    assert os.getuid() == 501 == Path('/dev/console').stat().st_uid
    record = dict(schema='airk.private-signing-test/1', source=str(source), output=str(output),
                  notarization_requested=False, source_freshness='R4 retained actual Rust pair; not R5 cache qualification')
    tool_paths = [checkout / 'scripts' / name for name in ['sign-native.py', 'package-native.py',
        'build-service-runtime.py', 'provision-onnx-runtime.py', 'int8-policy.py', 'macho-identity.py']]
    tools_before = {str(path.relative_to(checkout)): sha(path) for path in tool_paths}
    try:
        build = json.loads((source / 'airk-build.json').read_text())
        assert build['source'] == dict(sha256='7880013e0fd55b6cc9bb5505c16e052f123f72ee64caf8893081bff6c46db6cd', files=784)
        assert sha(source / 'AIRK.app/Contents/MacOS/airk') == '0f2d61b9be93d2c9dc6957a1cc9dd5f5fbf0d1a24df57700b9cd2a0fa2e7bdca'
        record['source_build'] = build['source']
        keys = ['/Users/tylerjensen/Library/Keychains/airk-signing.keychain-db',
                '/Users/tylerjensen/Library/Keychains/login.keychain-db']
        status = subprocess.run([str(parent / 'artifacts/p6a-keychain-status'), *keys], capture_output=True, text=True, timeout=15)
        assert status.returncode == 0
        record['keychain_metadata'] = status.stdout[-4096:]
        selected = next((path for path in keys if any(line.startswith('path=' + path + ' ') and 'unlocked=1' in line
                        for line in status.stdout.splitlines())), None)
        assert selected, 'No authorized existing keychain is unlocked in GUI context'
        record['selected_keychain'] = selected
        command = [sys.executable, str(checkout / 'scripts/sign-native.py'),
            '--runtime-dir', str(source), '--target', 'aarch64-apple-darwin',
            '--identity', 'Developer ID Application: Ioka LLC (5PX7D99PXA)',
            '--team-id', '5PX7D99PXA', '--keychain', selected, '--output', str(output)]
        record['command'] = command
        result = subprocess.run(command, cwd=checkout, capture_output=True, text=True, timeout=300)
        record.update(sign_exit=result.returncode, stdout=result.stdout[-8192:], stderr=result.stderr[-8192:])
        assert result.returncode == 0, 'Canonical signing failed; retain exact diagnostics'
        specification = importlib.util.spec_from_file_location('sign_r4_verifier', checkout / 'scripts/sign-native.py')
        signer = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(signer)
        signed = json.loads((output / 'signing.json').read_text())
        signer.read_verified_pair(output, source, 'aarch64-apple-darwin', signed['commit'], sha(checkout / 'Cargo.lock'))
        record['signing_receipt_sha256'] = sha(output / 'signing.json')
        record['copied_launcher'] = signed['copied_launcher']
        record['components'] = {name: dict(binary_sha256=value['binary_sha256'], verification=value['verification'],
                                          notarization=value['notarization']) for name, value in signed['components'].items()}
        record['status'] = 'passed'
    except BaseException as error:
        record.update(status='failed', failure=f'{type(error).__name__}: {error}')
        raise
    finally:
        record['tools_before'] = tools_before
        record['tools_unchanged'] = tools_before == {str(path.relative_to(checkout)): sha(path) for path in tool_paths}
        receipt_path.write_text(json.dumps(record, indent=2) + '\n')
        print(json.dumps(record, indent=2), flush=True)

def controller():
    assert os.getuid() == 501 == Path('/dev/console').stat().st_uid
    label = 'io.iokaio.airk.test-sign-' + nonce
    service = 'gui/501/' + label
    plist = parent / f'artifacts/p6a-gui-sign-{nonce}.plist'
    stdout = parent / f'artifacts/p6a-gui-sign-{nonce}.stdout'
    stderr = parent / f'artifacts/p6a-gui-sign-{nonce}.stderr'
    payload = dict(Label=label, ProgramArguments=[sys.executable, str(Path(__file__).resolve()), 'execute', nonce],
        WorkingDirectory=str(checkout), RunAtLoad=True, KeepAlive=False, LimitLoadToSessionType='Aqua',
        StandardOutPath=str(stdout), StandardErrorPath=str(stderr), ProcessType='Background',
        EnvironmentVariables=dict(PATH='/usr/bin:/bin:/usr/sbin:/sbin', HOME='/Users/tylerjensen'))
    with os.fdopen(os.open(plist, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'wb') as target:
        plistlib.dump(payload, target)
    control = dict(label=label, service=service)
    try:
        admission = subprocess.run(['/bin/launchctl', 'bootstrap', 'gui/501', str(plist)], capture_output=True, text=True, timeout=15)
        control.update(bootstrap_exit=admission.returncode, bootstrap_stderr=admission.stderr[-2048:])
        assert admission.returncode == 0
        deadline = time.monotonic() + 330
        while time.monotonic() < deadline:
            if receipt_path.exists():
                record = json.loads(receipt_path.read_text())
                print(json.dumps(record, indent=2))
                assert record['status'] == 'passed' and record['tools_unchanged']
                break
            time.sleep(.5)
        else:
            raise RuntimeError('GUI signing job exceeded owned test deadline')
    finally:
        stopped = subprocess.run(['/bin/launchctl', 'bootout', service], capture_output=True, text=True, timeout=15)
        remaining = subprocess.run(['/bin/launchctl', 'print', service], capture_output=True, text=True, timeout=15)
        control.update(bootout_exit=stopped.returncode, remaining_job_exit=remaining.returncode,
                       cleanup_verified=stopped.returncode == 0 and remaining.returncode != 0)
        (parent / f'artifacts/p6a-gui-sign-{nonce}.control.json').write_text(json.dumps(control, indent=2) + '\n')
        print(json.dumps(control, indent=2))
        assert control['cleanup_verified'], 'Retain exact plist if job cleanup failed'
        plist.unlink()

if len(sys.argv) > 1 and sys.argv[1] == 'execute':
    execute()
else:
    controller()
