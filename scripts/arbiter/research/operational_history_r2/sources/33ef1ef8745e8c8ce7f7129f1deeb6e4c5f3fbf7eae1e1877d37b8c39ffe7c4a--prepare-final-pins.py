# SPDX-License-Identifier: Apache-2.0
"""Prepare concrete package pins for review; never authorize or launch a batch."""
import argparse
import hashlib
import json
import re
from pathlib import Path

BASE = Path(__file__).resolve().parent
# Match the Windows qualification wrapper's recorded script input catalog.
SCRIPT_SUFFIXES = ('.py', '.cjs', '.ps1', '.json')

def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def draft_pins(package, check):
    if package.is_symlink() or check.is_symlink():
        raise ValueError('Package and Windows receipt must not be symlinks')
    package, check = package.resolve(strict=True), check.resolve(strict=True)
    if package.parent != BASE or not package.name.startswith('neo-frozen-transfer-r35-'):
        raise ValueError('Require a new r35 frozen package immediately inside this preparation directory')
    revision = re.fullmatch(r'clean-break-r([1-9][0-9]*)-.+\.json', check.name)
    if check.parent != BASE.parent or revision is None or int(revision[1]) < 35 or check.suffix != '.json':
        raise ValueError('Require an explicit Windows receipt with canonical numeric revision at least r35')
    manifest = json.loads((package / 'source-manifest.json').read_text(encoding='utf-8'))
    if manifest.get('status') != 'FROZEN_PUBLIC_SOURCE' or manifest.get('final_freeze_required') is not False:
        raise ValueError('Require a finalized source package')
    result = json.loads(check.read_text(encoding='utf-8'))
    if result.get('exit_code') != 0 or result.get('native_inputs_unchanged') is not True:
        raise ValueError('Require successful unchanged Windows inputs')
    rows = manifest.get('files')
    if not isinstance(rows, list) or not rows:
        raise ValueError('Require the complete frozen public source roster')
    roster = {}
    for row in rows:
        if not isinstance(row, dict):
            raise ValueError('Invalid frozen source row')
        name, digest = row.get('path'), row.get('sha256')
        if not isinstance(name, str) or name in roster or not isinstance(digest, str) or not re.fullmatch('[0-9a-f]{64}', digest):
            raise ValueError('Invalid or duplicate frozen source identity')
        roster[name] = digest
    native = {name: digest for name, digest in roster.items()
              if name in ('Cargo.toml', 'Cargo.lock') or name.startswith('crates/')}
    scripts = {name: digest for name, digest in roster.items()
               if name.startswith('scripts/') and Path(name).suffix in SCRIPT_SUFFIXES}
    if not {'Cargo.toml', 'Cargo.lock'} <= native.keys() or not any(name.startswith('crates/') for name in native) or not scripts:
        raise ValueError('Frozen roster omitted native or script inputs')
    if not isinstance(result.get('source_files'), dict) or result['source_files'] != native:
        raise ValueError('Windows native input roster differs from the frozen package')
    if not isinstance(result.get('script_sources'), dict) or result['script_sources'] != scripts:
        raise ValueError('Windows script input roster differs from the frozen package')
    return {
        'schema': 1,
        'status': 'DRAFT_AWAITING_FINAL_PIN_REVIEW',
        'primary_reviewed': False,
        'independent_reviewed': False,
        'inputs_frozen': True,
        'windows_checks_passed': True,
        'windows_check_receipt_sha256': sha(check),
        'windows_check_receipt': str(check),
        'windows_script_source_suffixes': list(SCRIPT_SUFFIXES),
        'reviewers': {'primary': None, 'independent': None},
        'package': str(package),
        'transfer_receipt_sha256': sha(package / 'transfer-receipt.json'),
        'restorer_sha256': sha(package / 'restore-neo-source.py'),
        'native_source': manifest['native_source'],
        'source_roster_sha256': manifest['source_roster_sha256'],
        'source_file_count': manifest['file_count'],
        'test_source_files': sum(name.startswith('packages/vscode/src/test/') and name.endswith('.test.ts') for name in roster),
        'public_models_roster_sha256': manifest['public_models_roster_sha256'],
        'launcher_sha256': sha(BASE / 'transfer-and-launch.py'),
        'runners': {'neo-scoped-owner.py': sha(BASE / 'neo-scoped-owner.py')},
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', required=True, type=Path)
    parser.add_argument('--windows-check-receipt', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args(argv)
    output = args.output.resolve()
    if output.parent != BASE or args.output.is_symlink():
        raise ValueError('Draft output must be a fresh immediate child of this directory')
    pins = draft_pins(args.package, args.windows_check_receipt)
    with output.open('x', encoding='utf-8') as stream:
        json.dump(pins, stream, indent=2)
        stream.write('\n')
    print(json.dumps(pins))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
