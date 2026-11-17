# SPDX-License-Identifier: Apache-2.0
"""One-off, read-only keychain status probe in the existing GUI user's context."""
import json
import os
from pathlib import Path
import plistlib
import subprocess
import time
import uuid

root = Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh').resolve(strict=True)
os.chdir(root)
uid = os.getuid()
assert uid == 501 == Path('/dev/console').stat().st_uid
helper = (root / 'artifacts/p6a-keychain-status').resolve(strict=True)
assert helper.is_relative_to(root) and helper.stat().st_uid == uid
nonce = uuid.uuid4().hex
label = f'io.iokaio.airk.test-sign-{nonce}'
domain = f'gui/{uid}'
service = f'{domain}/{label}'
plist = root / f'artifacts/p6a-gui-metadata-{nonce}.plist'
output = root / f'artifacts/p6a-gui-metadata-{nonce}.log'
error = root / f'artifacts/p6a-gui-metadata-{nonce}.stderr'
payload = dict(Label=label, ProgramArguments=[str(helper),
    '/Users/tylerjensen/Library/Keychains/login.keychain-db',
    '/Users/tylerjensen/Library/Keychains/airk-signing.keychain-db'],
    RunAtLoad=True, KeepAlive=False, LimitLoadToSessionType='Aqua',
    StandardOutPath=str(output), StandardErrorPath=str(error), ProcessType='Background')
with os.fdopen(os.open(plist, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'wb') as target:
    plistlib.dump(payload, target)
record = dict(label=label, domain=domain, uid=uid, output=str(output), error=str(error))
try:
    admitted = subprocess.run(['/bin/launchctl', 'bootstrap', domain, str(plist)],
                              capture_output=True, text=True, timeout=15)
    record['bootstrap_exit'] = admitted.returncode
    record['bootstrap_stderr'] = admitted.stderr[-2048:]
    if admitted.returncode:
        raise RuntimeError(f'GUI probe bootstrap failed: {record}')
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if output.exists() and output.stat().st_size:
            value = output.read_text()
            if value.count('get_status=') == 2:
                record['metadata'] = value[-4096:]
                break
        time.sleep(.1)
    else:
        raise RuntimeError('GUI metadata probe did not finish within 10 seconds')
finally:
    cleanup = subprocess.run(['/bin/launchctl', 'bootout', service], capture_output=True, text=True, timeout=15)
    remaining = subprocess.run(['/bin/launchctl', 'print', service], capture_output=True, text=True, timeout=15)
    record['bootout_exit'] = cleanup.returncode
    record['remaining_job_exit'] = remaining.returncode
    record['cleanup_verified'] = cleanup.returncode == 0 and remaining.returncode != 0
    receipt = root / f'artifacts/p6a-gui-metadata-{nonce}.json'
    receipt.write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record, indent=2))
    if not record['cleanup_verified']:
        raise RuntimeError('Exact metadata probe job cleanup failed; retain its plist for inspection')
    plist.unlink()
