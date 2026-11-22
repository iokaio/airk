# SPDX-License-Identifier: Apache-2.0
"""Write the pinned L5 phase A probe inputs (plan 12.5.2). Data only.

Each input is a CPU source profile plus a name->path asset map, copied byte
for byte (profiles) or projected from an authenticated source receipt (maps).
Every source is SHA-checked before use; outputs are fresh files.

  focus512   current-runtime regenerated Focus static-512 CPU profile
             (private-final-policy-v2-arbiter-focus-qwen3-reranker-06b).
  compact    resource-run input used by the earlier Compact FP16 metadata
             (fresh-cu15-current-compact-numeric-cpu-input-r1.json).
  large      resource-run input used by the earlier Large FP16 metadata.

Run: python -I l5a_inputs.py --campaign R --output-dir DIR
"""
import argparse, hashlib, json, pathlib

INPUTS = {
    'compact': ('fresh-cu15-current-compact-numeric-cpu-input-r1.json',
                '18722ef1b3a38777afff5a3ff4ddcb2a396f2730e4e5097a462092e584ce8c27'),
    'large': ('fp32-expanded-reference-preflight-r1/current-native-metadata-source-only-r2/'
              'arbiter-context-qwen3-4b/resource-run-input.private.json',
              'af9a46022b51994f9b5dbe620433a8868d1ad7b40fd223019167ae2ef90cac8b'),
    'focus512': ('fresh-cu15-current-cpu-metadata-r1/arbiter-focus-qwen3-reranker-06b/'
                 'resource-run-input.private.json',
                 'd03b7b4cfd640b23a869ce4ac88bf129443a50f6d4f495be6d6ea22c4dd1260f'),
}


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def pinned(path, expected):
    raw = pathlib.Path(path).read_bytes()
    if sha(raw) != expected:
        raise ValueError(f'{path} differs from its pin')
    return raw


def write(path, raw):
    with open(path, 'xb') as handle:
        handle.write(raw)
    return {'path': str(path), 'sha256': sha(raw)}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--campaign', type=pathlib.Path, required=True)
    p.add_argument('--output-dir', type=pathlib.Path, required=True)
    args = p.parse_args()
    args.output_dir.mkdir(exist_ok=False)
    receipt = {'schema': 'airk.private.l5a-inputs/1', 'outputs': {}}
    for key, (leaf, digest) in INPUTS.items():
        run = json.loads(pinned(args.campaign / leaf, digest))
        profile_raw = pinned(run['profile']['path'], run['profile']['sha256'])
        out_profile = write(args.output_dir / f'{key}-cpu-profile.json', profile_raw)
        out_map = write(args.output_dir / f'{key}-cpu-asset-map.json',
                        json.dumps(run['files'], indent=2, sort_keys=True).encode() + b'\n')
        receipt['outputs'][key] = {'source': {'path': str(args.campaign / leaf), 'sha256': digest},
                                   'profile': out_profile, 'asset_map': out_map}
    write(args.output_dir / 'inputs-receipt.json', json.dumps(receipt, indent=2).encode() + b'\n')
    print(json.dumps(receipt, indent=2))


if __name__ == '__main__':
    main()
