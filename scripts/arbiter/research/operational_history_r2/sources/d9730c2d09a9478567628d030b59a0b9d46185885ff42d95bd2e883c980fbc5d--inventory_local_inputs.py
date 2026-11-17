# SPDX-License-Identifier: Apache-2.0
"""Read-only input identities; static verification executes no build or model."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent
def row(path):
    return {'path': str(path), 'bytes': path.stat().st_size,
            'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
names = ['Cargo.lock', 'rust-toolchain.toml', 'scripts/signing-identity.json',
         'scripts/static-onnx-runtime-lock.json', 'scripts/static-onnx-tools-lock.json',
         'scripts/static-cmake-source-lock.json', 'crates/airk-retrieval/src/model_catalog.json',
         'crates/airk-retrieval/src/arbiter/publication.rs',
         'crates/airk-retrieval/src/arbiter/release-catalog.json',
         'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json',
         'artifacts/advanced-split/ort130-x64-owned-r5/build-receipt.json',
         'artifacts/advanced-split/ort130-x64-owned-r5/static-aggregate-receipt.json']
identities = {name: row(ROOT / name) for name in names if (ROOT / name).is_file()}
catalog = json.loads((ROOT / 'crates/airk-retrieval/src/model_catalog.json').read_bytes())
profiles = []
for profile in catalog['profiles']:
    directory = ROOT / 'artifacts/advanced-split/bundled-model-v2' / profile['id']
    files = {path.name: row(path) for path in directory.iterdir() if path.is_file()}
    assets_match = all(files[asset['name']]['bytes'] == asset['bytes'] and
                       files[asset['name']]['sha256'] == asset['sha256'] for asset in profile['assets'])
    manifest_match = json.loads((directory / 'model.json').read_bytes()) == profile['manifest']
    profiles.append({'id': profile['id'], 'directory': str(directory), 'files': files,
                     'catalog_assets_match': assets_match, 'catalog_manifest_match': manifest_match})
command = [sys.executable, 'scripts/static-runtime-environment.py', '--environment-file',
           'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json']
started = time.monotonic()
if '--verify-static' not in sys.argv:
    result = None
else:
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=60)
    (OUT / 'windows-static-verification.log').write_text(result.stdout + result.stderr, encoding='utf-8')
receipt = {'schema': 'airk.private-local-delivery-input-identities/1', 'read_only': True,
           'identities': identities, 'bundled_profiles': profiles,
           'static_verification': {'command': command, 'exit': result.returncode if result else None,
                                   'elapsed_seconds': time.monotonic() - started},
           'does_not_qualify_model_quality_or_product_promotion': True,
           'no_cargo_nn_signing_installation': True}
if result is None:
    receipt['static_verification'] = {'result': 'not_run_inventory_only', 'authority': 'root identifies current r5 owner as already verified; final canonical builder checks remain'}
elif result.returncode == 0:
    receipt['static_environment'] = json.loads(result.stdout)
else:
    receipt['static_verification']['error_tail'] = result.stderr[-2000:]
(OUT / 'windows-input-identities-current-r5.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
print(json.dumps({'static_verification': receipt['static_verification'],
                  'catalog_assets_and_manifests_match': all(p['catalog_assets_match'] and p['catalog_manifest_match'] for p in profiles)}))
raise SystemExit(result.returncode if result else 0)
