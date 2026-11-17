# SPDX-License-Identifier: Apache-2.0
"""Prepare concrete package pins for review; never authorize or launch a batch."""
import hashlib
import json
from pathlib import Path

base = Path(__file__).resolve().parent
package = base / 'neo-frozen-transfer-r32-clean-break'

def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

manifest = json.loads((package / 'source-manifest.json').read_text())
assert manifest['status'] == 'FROZEN_PUBLIC_SOURCE'
check = base.parent / 'clean-break-r32-native-focused-r3.json'
result = json.loads(check.read_text())
assert result['exit_code'] == 0 and result['native_inputs_unchanged']
pins = {
    'schema': 1,
    'status': 'DRAFT_AWAITING_FINAL_PIN_REVIEW',
    'primary_reviewed': True,
    'independent_reviewed': False,
    'inputs_frozen': True,
    'windows_checks_passed': True,
    'windows_check_receipt_sha256': sha(check),
    'windows_check_receipt': str(check),
    'reviewers': {'primary': '/root', 'independent': None},
    'package': str(package),
    'transfer_receipt_sha256': sha(package / 'transfer-receipt.json'),
    'restorer_sha256': sha(package / 'restore-neo-source.py'),
    'native_source': manifest['native_source'],
    'source_roster_sha256': manifest['source_roster_sha256'],
    'source_file_count': manifest['file_count'],
    'public_models_roster_sha256': manifest['public_models_roster_sha256'],
    'launcher_sha256': sha(base / 'transfer-and-launch.py'),
    'runners': {'neo-scoped-owner.py': sha(base / 'neo-scoped-owner.py')},
}
with (base / 'final-pins-draft.json').open('x', encoding='utf-8') as stream:
    json.dump(pins, stream, indent=2)
    stream.write('\n')
print(json.dumps(pins))
