# SPDX-License-Identifier: Apache-2.0
"""Record exact runtime reuse only for audited external CLI test corrections."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
RUNTIME = OUT / 'runtime-release'
ALLOWED = {'crates/airk-cli/tests/common/read_retry.rs', 'crates/airk-cli/tests/native_read_retry.rs'}

def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'scripts' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def inputs():
    paths = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo', 'crates', 'scripts/build-service-runtime.py', 'scripts/provision-onnx-runtime.py', 'scripts/provision-local-model.py', 'scripts/int8-policy.py', 'scripts/macho-identity.py', 'scripts/license-notices.py', 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md', 'third_party']
    raw = subprocess.check_output(['git', '-c', f'safe.directory={ROOT.as_posix()}', 'ls-files', '-z', '--cached', '--others', '--exclude-standard', '--', *paths], cwd=ROOT)
    return {os.fsdecode(name): digest(ROOT / os.fsdecode(name)) for name in sorted(set(raw.split(b'\0'))) if name}

builder = load('build-service-runtime')
package = load('package-native')
baseline_path = OUT / 'release-reuse-baseline.json'
if sys.argv[1] == 'capture':
    assert not baseline_path.exists()
    original = builder.source_identity()
    payload, _ = package.validate_runtime(RUNTIME, '1.8.9', 'x86_64-pc-windows-msvc', original)
    value = {'compiled_source': original, 'inputs': inputs(), 'complete_payload': payload}
    assert builder.source_identity() == original
    baseline_path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'captured': original, 'complete_payload_files': len(payload)}))
else:
    previous = json.loads(baseline_path.read_text(encoding='utf-8'))
    now = inputs()
    assert set(now) == set(previous['inputs'])
    changed = sorted(name for name, sha in now.items() if sha != previous['inputs'][name])
    assert set(changed).issubset(ALLOWED), changed
    payload, _ = package.validate_runtime(RUNTIME, '1.8.9', 'x86_64-pc-windows-msvc', previous['compiled_source'])
    assert payload == previous['complete_payload']
    receipt = {'runtime': str(RUNTIME), 'reused_not_fresh_build': True, 'compiled_source_retained': previous['compiled_source'], 'current_test_source': builder.source_identity(), 'changed_external_tests_only': changed, 'production_inputs_unchanged': True, 'complete_artifact_seals_unchanged': True}
    (OUT / 'release-reuse-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(receipt))
