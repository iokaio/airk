# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, os, subprocess
from pathlib import Path
root = Path(__file__).resolve().parents[2]
base = Path(__file__).resolve().parent
previous = json.loads((base / 'source-freeze-r1.json').read_text(encoding='utf-8'))
spec = importlib.util.spec_from_file_location('builder', root / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
scope = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo', 'crates', 'scripts/build-service-runtime.py', 'scripts/provision-onnx-runtime.py', 'scripts/provision-local-model.py', 'scripts/int8-policy.py', 'scripts/macho-identity.py', 'scripts/license-notices.py', 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md', 'third_party']
raw_names = subprocess.check_output(['git', '-c', 'safe.directory=' + root.as_posix(), 'ls-files', '-z', '--cached', '--others', '--exclude-standard', '--', *scope], cwd=root)
inputs = {}
for raw in sorted(set(p for p in raw_names.split(b'\0') if p)):
    name = os.fsdecode(raw)
    path = root / name
    assert path.is_file() and not path.is_symlink()
    inputs[name] = hashlib.sha256(path.read_bytes()).hexdigest()
changed = sorted(name for name in set(inputs) | set(previous['native_inputs']) if inputs.get(name) != previous['native_inputs'].get(name))
assert changed == ['crates/airk-cli/tests/service_runtime.rs', 'crates/airk-store/src/service_catalog_upgrade_recovery.rs'], changed
for key, folder in [('extension_sources', 'src'), ('compiled_extension', 'out')]:
    actual = {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((root / 'packages/vscode' / folder).rglob('*')) if p.is_file()}
    assert actual == previous[key], key
native = builder.source_identity()
value = {**previous, 'native_source': native, 'native_inputs': inputs, 'previous_freeze': 'source-freeze-r1.json', 'changed_native_inputs': changed, 'qualification': 'Reviewed test-fixture corrections only; production and extension bytes unchanged from R1. Compile receipt retains its original R1 native identity; this is not a new extension compilation or a test pass.'}
target = base / 'source-freeze-r3.json'
with target.open('x', encoding='utf-8') as stream: json.dump(value, stream, indent=2)
print(json.dumps({'native_source': native, 'changed_native_inputs': changed, 'freeze_sha256': hashlib.sha256(target.read_bytes()).hexdigest()}))

