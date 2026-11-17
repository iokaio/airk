# SPDX-License-Identifier: Apache-2.0
"""Verify a future local Windows 1.8.8 VSIX and manual copy; never build or package."""
import argparse
import hashlib
import importlib.util
import json
import re
import stat
import subprocess
from pathlib import Path
import zipfile

VERSION = '1.8.8'
TARGET = 'x86_64-pc-windows-msvc'
PLATFORM = 'win32-x64'
FORBIDDEN = {'.git', '.codex', '.agents', '.aws', '.ssh', '.mcp.json', '.airk', 'temp', 'target', 'artifacts', '__pycache__'}


def require(value, message):
    if not value:
        raise ValueError(message)


def plain(path, root=None):
    path = Path(path)
    metadata = path.lstat()
    require(stat.S_ISREG(metadata.st_mode) and metadata.st_nlink == 1
            and not getattr(metadata, 'st_file_attributes', 0) & 0x400, 'Linked/special input: ' + str(path))
    if root is not None:
        require(path.resolve(strict=True).is_relative_to(root), 'Input escapes reviewed root')
        for parent in path.parents:
            if parent == root:
                break
            require(not parent.is_symlink() and not (hasattr(parent, 'is_junction') and parent.is_junction()), 'Aliased input ancestor')
    return path


def sha(path):
    with plain(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def read_json(path, maximum=4 << 20):
    plain(path)
    with Path(path).open('rb') as stream:
        data = stream.read(maximum + 1)
    require(len(data) <= maximum, 'JSON input exceeds bound')
    return json.loads(data)


def canonical(value):
    return json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=False,allow_nan=False).encode('utf-8')


def load(name, path):
    specification = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def verify(args):
    root = args.root.resolve(strict=True)
    artifacts = root / 'artifacts'
    paths = [args.runtime, args.native_manifest, args.extension_manifest, args.manual_copy]
    require(all(path.resolve(strict=True).is_relative_to(artifacts) for path in paths), 'Candidate inputs must stay in owned artifacts/')
    require(sha(args.source_pins) == args.source_pins_sha256, 'Reviewed source pins changed')
    pins = read_json(args.source_pins, 16 << 20)
    files = pins['files']
    source = pins['native_source']
    require(isinstance(files, dict) and 0 < len(files) <= 20000, 'Complete reviewed source/output file map required')
    require(set(source) == {'sha256', 'files'} and re.fullmatch('[0-9a-f]{64}', source['sha256'])
            and type(source['files']) is int and source['files'] > 0, 'Native source identity invalid')
    checked = {}

    def pinned(name):
        require(not Path(name).is_absolute() and '..' not in Path(name).parts
                and not (set(Path(name).parts) & FORBIDDEN), 'Private/unsafe source input')
        require(name in files and re.fullmatch('[0-9a-f]{64}', files[name]), 'Required source/output pin missing: ' + name)
        path = plain(root / name, root)
        require(sha(path) == files[name], 'Source/output differs from reviewed pins: ' + name)
        checked[name] = files[name]
        return path.read_bytes()

    for name in ['scripts/package-native.py', 'scripts/package-extension.py', 'scripts/build-service-runtime.py']:
        pinned(name)
    native = load('branch_native_verification', root / 'scripts/package-native.py')
    extension = load('branch_extension_verification', root / 'scripts/package-extension.py')
    builder = native.runtime_builder()
    require(builder.source_identity() == source, 'Current native inputs differ from reviewed source')
    require(sha(args.runtime / 'airk-build.json') == args.runtime_build_sha256, 'Fresh runtime build seal differs')
    build = read_json(args.runtime / 'airk-build.json', 65536)
    require(build.get('profile') == 'release' and build.get('source') == source, 'Reviewed release build identity required')
    runtime_files, executables = native.validate_runtime(args.runtime, VERSION, TARGET, source)
    require(sha(args.native_manifest) == args.native_manifest_sha256, 'Native package receipt changed')
    native_receipt = read_json(args.native_manifest)
    require(native_receipt.get('schema') == 'airk-native-candidate/2'
            and native_receipt.get('version') == VERSION and native_receipt.get('target') == TARGET
            and native_receipt.get('vsix_target') == PLATFORM and native_receipt.get('source') == source
            and native_receipt.get('payload') == runtime_files
            and native_receipt.get('commit') == args.commit and native_receipt.get('tree') == args.tree
            and native_receipt.get('signed_distribution') is False and native_receipt.get('signing') is None,
            'Exact unsigned local native package lineage differs')
    require(subprocess.check_output(['git', '-c', 'safe.directory=' + root.as_posix(), 'rev-parse', 'HEAD'], cwd=root, text=True).strip() == args.commit, 'Current package metadata commit differs')
    require(extension.source_tree(args.commit) == args.tree, 'Current metadata tree differs')
    require(native_receipt.get('cargo_lock_sha256') == hashlib.sha256(pinned('Cargo.lock')).hexdigest()
            and native_receipt.get('toolchain_file_sha256') == hashlib.sha256(pinned('rust-toolchain.toml')).hexdigest(), 'Native lock/toolchain differs')
    builder.load_script('int8-policy').verify_embedded(native_receipt.get('embedded_models', {}))
    extension.safe_name(native_receipt['archive'])
    require(native_receipt['archive'] == 'airk-1.8.8-x86_64-pc-windows-msvc.zip', 'Unexpected native artifact name')
    archive = plain(args.native_manifest.parent / native_receipt['archive'], root)
    require(sha(archive) == native_receipt['archive_sha256'], 'Native ZIP digest differs')
    native.verify_archive(archive, runtime_files, executables)
    require(native_receipt['install_manifest_sha256'] == runtime_files['airk-install.json'], 'Native installer seal differs')

    require(sha(args.extension_manifest) == args.extension_manifest_sha256, 'Extension package receipt changed')
    receipt = read_json(args.extension_manifest)
    require(receipt.get('schema') == 'airk-extension-candidate/1' and receipt.get('version') == VERSION
            and receipt.get('platform') == PLATFORM and receipt.get('extension_id') == 'iokaio.airk'
            and receipt.get('commit') == args.commit and receipt.get('tree') == args.tree
            and receipt.get('pre_release') is False
            and receipt.get('native_manifest_sha256') == args.native_manifest_sha256
            and receipt.get('native_archive_sha256') == native_receipt['archive_sha256'], 'Extension/native lineage differs')
    extension.safe_name(receipt['archive'])
    require(receipt['archive'] == 'airk-1.8.8-win32-x64.vsix', 'Unexpected manual-test artifact name')
    vsix = plain(args.extension_manifest.parent / receipt['archive'], root)
    require(sha(vsix) == receipt['archive_sha256'], 'VSIX digest differs')
    package = json.loads(pinned('packages/vscode/package.json'))
    require(package['version'] == VERSION and package['name'] == 'airk' and package['publisher'] == 'iokaio', 'Current package version/identity differs')
    expected_package = extension.runtime_package(package)
    require(receipt.get('display_name') == expected_package['displayName'], 'Package display name differs')
    require(receipt.get('npm_lock_sha256') == hashlib.sha256(pinned('packages/vscode/package-lock.json')).hexdigest(), 'npm lock differs')
    # Bind every current production TypeScript/webview/help/compile input as well
    # as compiled outputs. This checks reviewed bytes, not a new compiler run.
    for path in (root / 'packages/vscode/src').rglob('*'):
        if path.is_file() and 'test' not in path.relative_to(root / 'packages/vscode/src').parts:
            pinned(path.relative_to(root).as_posix())
    for path in sorted((root / 'packages/vscode').glob('tsconfig*.json')):
        pinned(path.relative_to(root).as_posix())
    pinned('packages/vscode/scripts/build-explorer.cjs')
    pinned('packages/vscode/scripts/package.cjs')
    expected = {}
    def include(destination, data):
        require(destination not in expected, 'Duplicate expected payload')
        expected[destination] = hashlib.sha256(data).hexdigest()
    for name in (*extension.MODULES, *extension.BUNDLES):
        include('out/' + name, pinned('packages/vscode/out/' + name))
    for name in extension.JSONC_FILES:
        include('node_modules/jsonc-parser/' + name, pinned('packages/vscode/node_modules/jsonc-parser/' + name))
    include('README.md', pinned('packages/vscode/README.md'))
    for name in ['LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md']:
        include(name, pinned(name))
    for name in extension.ILLUSTRATIONS:
        include('images/' + name, pinned('docs/images/' + name))
    for name, component in extension.PRODUCT_ILLUSTRATIONS.items():
        include('images/' + name, pinned('docs/' + component + '/images/' + name))
    for name in extension.ICONS:
        include('images/' + name, pinned('packages/vscode/images/' + name))
    expected.update({'native/runtime/' + name: digest for name, digest in runtime_files.items()})
    include('native/candidate.json', args.native_manifest.read_bytes())
    with zipfile.ZipFile(vsix) as zipped:
        require(canonical(json.loads(zipped.read('extension/package.json'))) == canonical(expected_package), 'Staged package transform differs')
        wrapper_bytes = zipped.read('extension/native/manifest.json')
        require(canonical(json.loads(wrapper_bytes)) == canonical({'schema': 2, 'version': VERSION, 'vsix_target': PLATFORM,
            'runtime_dir': 'runtime', 'install_manifest_sha256': runtime_files['airk-install.json'],
            'payload': {name: runtime_files[name] for name in ('airk.exe', 'airk-embed.exe')}}), 'VSIX runtime wrapper differs')
        include('native/manifest.json', wrapper_bytes)
        include('package.json', zipped.read('extension/package.json'))
        allowed_directories=set()
        for name in expected:
            normalized='readme.md' if name=='README.md' else 'LICENSE.txt' if name=='LICENSE' else name
            parts=('extension/'+normalized).split('/')
            allowed_directories.update('/'.join(parts[:end]) for end in range(1,len(parts)))
        for item in zipped.infolist():
            parts = Path(item.filename).parts
            require(not (set(parts) & FORBIDDEN) and '/out/test/' not in item.filename, 'Private/test state in VSIX')
            if item.is_dir():
                require(item.filename.rstrip('/') in allowed_directories, 'Unknown empty VSIX directory')
    require(receipt.get('payload') == expected, 'Complete allowlisted stage differs from source/runtime outputs')
    extension.verify_vsix(vsix, expected, 'airk.exe', PLATFORM, VERSION, False, expected_package['displayName'])
    plain(args.manual_copy, root)
    require(args.manual_copy.name == vsix.name and args.manual_copy.resolve() != vsix.resolve()
            and sha(args.manual_copy) == receipt['archive_sha256'], 'Manual-test copy is not the exact verified VSIX')
    # Close the observation interval without changing any candidate or source.
    require(native.inventory(args.runtime) == runtime_files and builder.source_identity() == source, 'Runtime/native source changed during verification')
    require(all(sha(root / name) == digest for name, digest in checked.items()), 'Source/compiled output changed during verification')
    require(sha(args.source_pins) == args.source_pins_sha256 and sha(args.native_manifest) == args.native_manifest_sha256
            and sha(args.extension_manifest) == args.extension_manifest_sha256 and sha(vsix) == sha(args.manual_copy) == receipt['archive_sha256'], 'Receipts/artifacts changed during verification')
    return {'status': 'passed', 'version': VERSION, 'platform': PLATFORM, 'native_source': source,
        'runtime_build_sha256': args.runtime_build_sha256, 'source_pins_sha256': args.source_pins_sha256,
        'package_metadata_commit': args.commit, 'package_metadata_tree': args.tree,
        'native_manifest_sha256': args.native_manifest_sha256, 'extension_manifest_sha256': args.extension_manifest_sha256,
        'vsix': str(vsix), 'manual_copy': str(args.manual_copy), 'vsix_sha256': receipt['archive_sha256'],
        'runtime_files_verified': len(runtime_files), 'compiled_production_files_verified': len(extension.MODULES) + len(extension.BUNDLES),
        'complete_payload_files_verified': len(expected), 'source_and_output_pins_verified': len(checked),
        'package_transform': 'canonical runtime_package; devDependencies removed and only reviewed uninstall script retained',
        'readme': 'exact pinned source bytes', 'private_test_state': 'excluded by complete stage equality',
        'limits': 'Read-only byte/receipt verification; no compilation, packaging, native execution, OS signature, installed-editor, cross-platform or publication qualification.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['root', 'runtime', 'native-manifest', 'extension-manifest', 'manual-copy', 'source-pins', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    for name in ['source-pins-sha256', 'runtime-build-sha256', 'native-manifest-sha256', 'extension-manifest-sha256', 'commit', 'tree']:
        parser.add_argument('--' + name, required=True)
    args = parser.parse_args()
    for name in ['source_pins_sha256', 'runtime_build_sha256', 'native_manifest_sha256', 'extension_manifest_sha256']:
        require(re.fullmatch('[0-9a-f]{64}', getattr(args, name)), 'Explicit final SHA-256 pin required')
    require(all(re.fullmatch('[0-9a-f]{40}', value) for value in [args.commit, args.tree]), 'Explicit final Git metadata pins required')
    require(args.output.parent.resolve(strict=True).is_relative_to(args.root.resolve(strict=True) / 'artifacts'), 'Report must be in owned artifacts/')
    # Reserve the receipt first. Never overwrite prior verification evidence.
    with args.output.open('x', encoding='utf-8') as output:
        try:
            report = verify(args)
        except BaseException as error:
            output.write(json.dumps({'status': 'failed', 'failure_type': type(error).__name__, 'error': str(error)[:4096]}, indent=2) + '\n')
            raise
        output.write(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
