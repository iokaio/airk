# SPDX-License-Identifier: Apache-2.0
"""Bind the actual Cargo-produced fixture; metadata preparation only."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]

def pin(path):
    path = Path(path).resolve(strict=True)
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(block)
    return {'path': str(path), 'bytes': path.stat().st_size, 'sha256': digest.hexdigest()}

def cargo_image(path):
    matches = set()
    for line in Path(path).read_text(encoding='utf-8').splitlines():
        try:
            row = json.loads(line)
        except ValueError:
            continue
        if (row.get('reason') == 'compiler-artifact'
                and row.get('target', {}).get('name') == 'airk_service'
                and row.get('profile', {}).get('test') is True
                and row.get('executable')):
            matches.add(row['executable'])
    if len(matches) != 1:
        raise ValueError('expected exactly one actual Service test executable')
    return matches.pop()

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--cargo-json', required=True)
    parser.add_argument('--build-receipt', required=True)
    parser.add_argument('--inputs', required=True)
    parser.add_argument('--work', required=True)
    parser.add_argument('--seconds', type=int, required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--semantic-plan')
    args = parser.parse_args()
    if not 1 <= args.seconds <= 21000:
        raise ValueError('fixture deadline must leave owner closure time under 21600s')
    work = Path(args.work)
    if not work.is_absolute() or work.exists() or not work.parent.is_dir():
        raise ValueError('work must be a fresh absolute child of an existing private anchor')
    receipt = json.loads(Path(args.build_receipt).read_text(encoding='utf-8'))
    if receipt.get('exit_code') != 0:
        raise ValueError('actual successful build receipt required')
    staged_metadata = []
    for row in json.loads(Path(args.inputs).read_text(encoding='utf-8')):
        profile = pin(row['profile'])
        if profile['sha256'] != row['profile_sha256']:
            raise ValueError('actual staged native Profile bytes changed')
        staged_metadata.extend((profile, pin(row['fixtures'])))
    packet = {
        'schema': 'airk.private-selected-cal-fixture-launch/1',
        'evaluation': True, 'release_qualified': False,
        'cwd': str(ROOT), 'image': pin(cargo_image(args.cargo_json)),
        'cargo_json': pin(args.cargo_json), 'build_receipt': pin(args.build_receipt),
        'native_source_group': receipt['groups']['native']['sha256'],
        'inputs': pin(args.inputs), 'work': str(work),
        'staged_metadata': staged_metadata,
        'asset_authentication_owner': 'Native fixture Prepare verifies every declared Profile/asset/fixture, then copies through native Store custody before genuine Ready. This wrapper does not assert post-run source-payload immutability.',
        'fixture_seconds': args.seconds, 'owner_seconds': args.seconds + 600,
        'owner': pin(ROOT / 'scripts/provision-static-onnx-runtime.py'),
        'selector': 'service_runtime::evaluation_fixture_tests::actual_private_arbiter_fixture',
        'context_cal_observer': True, 'native_dispatch_observer': True,
        'approval_required_before_execution': True,
        'semantic_plan': pin(args.semantic_plan) if args.semantic_plan else None,
        'pipeline_pending_native_plan': not bool(args.semantic_plan),
    }
    with Path(args.output).open('x', encoding='utf-8', newline='\n') as out:
        json.dump(packet, out, indent=2)
        out.write('\n')

if __name__ == '__main__':
    main()
