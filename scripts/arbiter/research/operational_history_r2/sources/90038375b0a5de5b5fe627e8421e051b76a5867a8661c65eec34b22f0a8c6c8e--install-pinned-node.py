# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path
import re
import subprocess
import urllib.request
import zipfile

root = Path(__file__).resolve().parents[2]
pins = set(re.findall(r"node-version:\s*['\"]([0-9.]+)['\"]", (root / '.github/workflows/ci.yml').read_text(encoding='utf-8')))
if len(pins) != 1:
    raise RuntimeError('Expected one repository Node pin')
version = pins.pop()
name = f'node-v{version}-win-x64'
archive_name = name + '.zip'
tools = root / 'artifacts/tools'
tools.mkdir(parents=True, exist_ok=True)
archive = tools / archive_name
base = f'https://nodejs.org/dist/v{version}/'
sums = urllib.request.urlopen(base + 'SHASUMS256.txt', timeout=60).read(1024 * 1024).decode('ascii')
matches = [line.split()[0] for line in sums.splitlines() if len(line.split()) == 2 and line.split()[1] == archive_name]
if len(matches) != 1 or not re.fullmatch('[0-9a-f]{64}', matches[0]):
    raise RuntimeError('Missing official archive checksum')
expected = matches[0]
if not archive.exists():
    partial = archive.with_suffix('.zip.partial')
    with urllib.request.urlopen(base + archive_name, timeout=60) as response, partial.open('xb') as output:
        total = 0
        while block := response.read(1024 * 1024):
            total += len(block)
            if total > 128 * 1024 * 1024:
                raise RuntimeError('Archive download exceeded bound')
            output.write(block)
    partial.rename(archive)
actual = hashlib.file_digest(archive.open('rb'), 'sha256').hexdigest()
if actual != expected:
    raise RuntimeError('Official archive checksum mismatch')
destination = tools / name
if destination.exists():
    raise RuntimeError('Existing portable Node directory preserved; inspect it before reuse')
with zipfile.ZipFile(archive) as package:
    size = 0
    for entry in package.infolist():
        parts = Path(entry.filename).parts
        if not parts or parts[0] != name or '..' in parts or Path(entry.filename).is_absolute():
            raise RuntimeError('Unexpected archive member path')
        size += entry.file_size
        if size > 512 * 1024 * 1024:
            raise RuntimeError('Expanded archive exceeded bound')
    package.extractall(tools)
binary = destination / 'node.exe'
observed = subprocess.run([str(binary), '--version'], capture_output=True, text=True, check=True).stdout.strip()
if observed != 'v' + version:
    raise RuntimeError('Portable Node reports the wrong version')
receipt = {'version': version, 'source': base + archive_name, 'archive_sha256': actual,
           'checksum_source': base + 'SHASUMS256.txt', 'node_sha256': hashlib.file_digest(binary.open('rb'), 'sha256').hexdigest(),
           'node': str(binary), 'npm_cli': str(destination / 'node_modules/npm/bin/npm-cli.js'),
           'global_settings_changed': False}
(tools / f'{name}.receipt.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
print(json.dumps(receipt))
