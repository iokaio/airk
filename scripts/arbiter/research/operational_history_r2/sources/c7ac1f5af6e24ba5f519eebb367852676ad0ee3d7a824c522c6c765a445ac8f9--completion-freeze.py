# SPDX-License-Identifier: Apache-2.0
"""Record and verify separate public native, extension, harness and fixture inputs."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
BASE = Path(__file__).resolve().parent

def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def tracked(*paths):
    raw = subprocess.check_output(['git', '-c', 'safe.directory=' + ROOT.as_posix(),
        'ls-files', '-z', '--cached', '--others', '--exclude-standard', '--', *paths], cwd=ROOT)
    return [ROOT / name.decode() for name in sorted(set(raw.split(b'\0')) - {b''})]

def snapshot(paths):
    result = {}
    for path in sorted(set(paths)):
        relative = path.relative_to(ROOT)
        assert not any(part in ('temp', '.airk', '.git', '.aws', '.ssh') for part in relative.parts)
        assert path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(ROOT)
        result[relative.as_posix()] = digest(path)
    return result

def observe():
    specification = importlib.util.spec_from_file_location('completion_builder', ROOT / 'scripts/build-service-runtime.py')
    builder = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(builder)
    native = tracked('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo', 'crates',
        'third_party', 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md')
    extension = tracked('packages/vscode')
    extension += [path for path in (ROOT / 'packages/vscode/out').iterdir() if path.is_file()]
    package_specification = importlib.util.spec_from_file_location('completion_extension', ROOT / 'scripts/package-extension.py')
    package = importlib.util.module_from_spec(package_specification)
    package_specification.loader.exec_module(package)
    extension += [ROOT / 'packages/vscode/node_modules/jsonc-parser' / name for name in package.JSONC_FILES]
    harness = tracked('scripts', 'Dockerfile', '.dockerignore', '.config/nextest.toml')
    audit = BASE / 'client-protocol-audit-r1'
    harness += [path for path in audit.iterdir() if path.is_file() and path.suffix in ('.py', '.cjs')]
    harness += [audit / 'code-source-identity.json', BASE / 'run-qualification-gate.py', Path(__file__).resolve()]
    harness += [ROOT / 'packages/vscode/out/test' / name for name in (
        'installedEditor.js', 'installedControlTrace.js', 'installedEditorC8.js',
        'installedEditorContracts.js', 'installedEditorHttp.js', 'installedCompletion.js')]
    manifest = json.loads((ROOT / 'scripts/container-inputs.json').read_text())
    fixtures = tracked('examples', 'tools/parser-train')
    fixtures += [ROOT / name for name in manifest['files'] if name.startswith(('docs/', 'packages/'))]
    fixtures += [BASE / 'static-environment-supplement.json']
    groups = {name: snapshot(paths) for name, paths in (
        ('native', native), ('extension', extension), ('harness', harness), ('fixtures', fixtures))}
    identities = {name: {'sha256': hashlib.sha256(json.dumps(files, sort_keys=True,
        separators=(',', ':')).encode()).hexdigest(), 'files': len(files)} for name, files in groups.items()}
    return {'schema': 'airk-completion-freeze/1', 'version': '1.8.14',
        'compiled_native_source': builder.source_identity(), 'identities': identities, 'groups': groups}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--verify', action='store_true')
    args = parser.parse_args()
    current = observe()
    if args.verify:
        expected = json.loads(args.output.read_text())
        assert current == expected, 'Frozen consumed inputs changed; stop qualification'
        print(json.dumps({'frozen_inputs_unchanged': True, 'identities': current['identities']}))
    else:
        with args.output.open('x', encoding='utf-8') as output:
            json.dump(current, output, indent=2)
            output.write('\n')
        print(json.dumps({key: value for key, value in current.items() if key != 'groups'}))

if __name__ == '__main__':
    main()
