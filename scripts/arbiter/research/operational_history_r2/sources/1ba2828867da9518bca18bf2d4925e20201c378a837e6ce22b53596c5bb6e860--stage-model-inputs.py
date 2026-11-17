# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path
import subprocess

gh = 'C:/Program Files/GitHub CLI/gh.exe'
repo = 'iokaio/airk'
tag = 'advanced-split-build-inputs-20261114'
archive = Path('artifacts/advanced-split/models/build-inputs-1.9.0/airk-minilm-trained-v2-build-inputs.zip')
expected_sha = '18f19f92653d98c01f54cce1dacd482de3ece91a635eefa20696de63da8599ea'
assert archive.stat().st_size == 139684472
assert hashlib.sha256(archive.read_bytes()).hexdigest() == expected_sha
def call(*args):
    result = subprocess.run([gh, *args], capture_output=True, text=True, timeout=300)
    if result.returncode:
        raise RuntimeError('GitHub operation failed: ' + result.stderr[:2000])
    return result.stdout

releases = json.loads(call('api', f'repos/{repo}/releases?per_page=100'))
found = [release for release in releases if release['tag_name'] == tag]
assert len(found) <= 1
notes = Path('artifacts/advanced-split/receipts/model-input-draft-notes.txt')
notes.write_text('Build-only candidate inputs for the authorized advanced-split campaign.\n'
                 'Unpublished draft; this is not a qualified product release.\n'
                 'The archive contains exactly two trained model.onnx, tokenizer.json and LICENSE sets.\n'
                 'It contains no training/evaluation data, private fixtures, receipts, credentials or live state.\n')
if not found:
    call('release', 'create', tag, '--repo', repo, '--draft', '--target', 'feat/advanced-split',
         '--title', 'Advanced split 1.9.0 build inputs (unpublished)', '--notes-file', str(notes))
    found = [json.loads(call('api', f'repos/{repo}/releases/tags/{tag}'))]
release = found[0]
assert release['draft'] is True
assets = [asset for asset in release['assets'] if asset['name'] == archive.name]
assert len(assets) <= 1
if not assets:
    call('release', 'upload', tag, str(archive), '--repo', repo)
    release = json.loads(call('api', f'repos/{repo}/releases/{release["id"]}'))
    assets = [asset for asset in release['assets'] if asset['name'] == archive.name]
assert len(assets) == 1
asset = assets[0]
assert asset['size'] == archive.stat().st_size and asset['digest'] == 'sha256:' + expected_sha
assert asset['state'] == 'uploaded' and release['draft'] is True
receipt = {'schema': 'airk.model-build-input-staging/1', 'repo': repo,
           'release_id': release['id'], 'tag': tag, 'draft': True, 'asset_id': asset['id'],
           'asset_name': archive.name, 'bytes': asset['size'], 'sha256': expected_sha,
           'qualification': False, 'product_publication': False}
Path('artifacts/advanced-split/receipts/delivery-02-model-input-staging.json').write_text(json.dumps(receipt, indent=2))
print(json.dumps(receipt, indent=2))
