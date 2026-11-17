# SPDX-License-Identifier: Apache-2.0
"""Collect final VSIX verifier pins only after the reviewed freeze still matches.

No compilation, tests, native execution, packaging, downloading or automatic
acceptance of changed compiler outputs. Root owns the post-commit/package order.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import sys

sys.dont_write_bytecode = True
EXPECTED_NATIVE = {'sha256': '2fb291eada72a3161622ed20b6a0c4cd7c478b2a07843cec3d4019a1fddaf972', 'files': 973}
EXPECTED_FREEZE_SHA256 = '4d95a3ad1cc81e38415373f8ff8653e3b6b779dccc53efcb0e68e028e228b6d5'
FORBIDDEN = {'.git', '.codex', '.agents', '.aws', '.ssh', '.mcp.json', '.airk', 'temp', 'target', 'artifacts', '__pycache__'}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def plain(path, root):
    require(path.resolve(strict=True).is_relative_to(root), 'Source escapes reviewed root: ' + str(path))
    for candidate in (path, *path.parents):
        if candidate == root:
            break
        require(not candidate.is_symlink() and not (hasattr(candidate, 'is_junction') and candidate.is_junction()), 'Aliased source path: ' + str(candidate))
    metadata = path.lstat()
    require(stat.S_ISREG(metadata.st_mode) and metadata.st_nlink == 1
            and not getattr(metadata, 'st_file_attributes', 0) & 0x400, 'Source is not an unaliased regular file: ' + str(path))
    return path


def source_path(root, name):
    require(isinstance(name, str) and not Path(name).is_absolute() and '..' not in Path(name).parts
            and not (set(Path(name).parts) & FORBIDDEN), 'Unsafe/private source key')
    return plain(root / name, root)


def tree_hashes(root, relative):
    directory = root / relative
    require(directory.is_dir() and directory.resolve(strict=True).is_relative_to(root), 'Missing source/output directory')
    pending = [directory]
    result = {}
    nodes = 0
    while pending:
        current = pending.pop()
        require(not current.is_symlink() and not (hasattr(current, 'is_junction') and current.is_junction()), 'Aliased source/output directory')
        with os.scandir(current) as entries:
            for entry in entries:
                nodes += 1
                require(nodes <= 20000, 'Source/output tree exceeds reviewed bound')
                path = Path(entry.path)
                metadata = path.lstat()
                require(not stat.S_ISLNK(metadata.st_mode) and not getattr(metadata, 'st_file_attributes', 0) & 0x400, 'Aliased source/output entry')
                if stat.S_ISDIR(metadata.st_mode):
                    pending.append(path)
                else:
                    name = path.relative_to(root).as_posix()
                    result[name] = sha(source_path(root, name))
    return result


def unchanged(label, expected, actual):
    difference = sorted(name for name in expected.keys() | actual.keys() if expected.get(name) != actual.get(name))
    require(not difference, label + ' differs from source-freeze-r3; refusing automatic acceptance. Changed/missing/extra: ' + repr(difference[:20]))


def load(name, path):
    specification = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def collect(root, freeze_path):
    require(sha(freeze_path) == EXPECTED_FREEZE_SHA256, 'Exact reviewed source-freeze-r3 changed')
    require(freeze_path.stat().st_size <= 16 << 20, 'Freeze exceeds bound')
    freeze = json.loads(freeze_path.read_text(encoding='utf-8-sig'))
    require(freeze.get('status') == 'reviewed_integrated_source' and freeze.get('reviewed') is True
            and Path(freeze['source_root']).resolve(strict=True) == root
            and freeze.get('compiled_extension_reviewed') is True
            and freeze.get('native_source') == EXPECTED_NATIVE, 'Reviewed freeze identity differs')
    require(len(freeze['native_inputs']) == 973 and len(freeze['extension_sources']) == 248
            and len(freeze['compiled_extension']) == 178, 'Freeze scope/count differs')
    native = {name: sha(source_path(root, name)) for name in freeze['native_inputs']}
    unchanged('Native input map', freeze['native_inputs'], native)
    extension_sources = tree_hashes(root, 'packages/vscode/src')
    compiled = tree_hashes(root, 'packages/vscode/out')
    unchanged('All 248 extension source files', freeze['extension_sources'], extension_sources)
    unchanged('All 178 compiled output files (including test outputs)', freeze['compiled_extension'], compiled)
    # Only load the canonical owners after their current bytes have been read.
    builder = load('final_pin_runtime_builder', source_path(root, 'scripts/build-service-runtime.py'))
    require(builder.source_identity() == EXPECTED_NATIVE, 'Current native input SET or framed identity differs')
    packager = load('final_pin_extension_owner', source_path(root, 'scripts/package-extension.py'))
    files = {**native, **extension_sources, **compiled}
    required = {'Cargo.lock', 'rust-toolchain.toml', 'scripts/package-native.py', 'scripts/package-extension.py', 'scripts/build-service-runtime.py',
        'packages/vscode/package.json', 'packages/vscode/package-lock.json', 'packages/vscode/README.md',
        'packages/vscode/scripts/build-explorer.cjs', 'packages/vscode/scripts/package.cjs',
        'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md'}
    required.update(path.relative_to(root).as_posix() for path in (root / 'packages/vscode').glob('tsconfig*.json'))
    required.update('packages/vscode/out/' + name for name in (*packager.MODULES, *packager.BUNDLES))
    required.update('packages/vscode/node_modules/jsonc-parser/' + name for name in packager.JSONC_FILES)
    required.update('docs/images/' + name for name in packager.ILLUSTRATIONS)
    required.update('docs/' + component + '/images/' + name for name, component in packager.PRODUCT_ILLUSTRATIONS.items())
    required.update('packages/vscode/images/' + name for name in packager.ICONS)
    for name in sorted(required):
        files[name] = sha(source_path(root, name))
    require(json.loads((root / 'packages/vscode/package.json').read_text(encoding='utf-8'))['version'] == '1.8.8', 'Working package version differs')
    # The observation interval ends with exact map/closure checks again. Neither
    # a later package compile nor another source change may be silently adopted.
    require(builder.source_identity() == EXPECTED_NATIVE, 'Native inputs changed during pin collection')
    unchanged('Extension sources after collection', extension_sources, tree_hashes(root, 'packages/vscode/src'))
    unchanged('Compiled outputs after collection', compiled, tree_hashes(root, 'packages/vscode/out'))
    require(all(sha(source_path(root, name)) == digest for name, digest in files.items()), 'A collected input changed during observation')
    require(sha(freeze_path) == EXPECTED_FREEZE_SHA256, 'Freeze changed during observation')
    return {'schema': 'airk.branch-review.vsix-source-pins/1', 'status': 'observed_matching_reviewed_freeze',
        'source_root': str(root), 'native_source': EXPECTED_NATIVE, 'files': files,
        'freeze': {'path': str(freeze_path), 'sha256': EXPECTED_FREEZE_SHA256,
            'extension_sources_verified': 248, 'compiled_outputs_verified': 178,
            'original_compile_receipt_sha256': freeze['compile_receipt_sha256']},
        'compiled_production_files': len(packager.MODULES) + len(packager.BUNDLES),
        'required_packaging_inputs': sorted(required),
        'qualification': 'Observed byte identity only. Compiler execution and post-commit/canonical-package ordering are established by Root receipts, not inferred here. No tests, runtime or packaging operation was run by this collector. Final release/ledger docs outside the packaged input scope are not native/extension equivalence blockers.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--freeze', type=Path, default=Path(__file__).with_name('source-freeze-r3.json'))
    parser.add_argument('--output', type=Path, default=Path(__file__).with_name('final-vsix-source-pins-r1.json'))
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    require(args.output.parent.resolve(strict=True).is_relative_to(root / 'artifacts'), 'Fresh output must stay in owned artifacts/')
    with args.output.open('x', encoding='utf-8') as stream:
        try:
            result = collect(root, args.freeze)
        except BaseException as error:
            stream.write(json.dumps({'status': 'failed', 'failure_type': type(error).__name__, 'error': str(error)[:4096]}, indent=2) + '\n')
            raise
        stream.write(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'path': str(args.output), 'sha256': sha(args.output), 'native_source': EXPECTED_NATIVE,
        'extension_sources': 248, 'compiled_outputs': 178, 'files': len(result['files']),
        'scope': 'Matching source/output pins; no compiler or qualification pass inferred'}))


if __name__ == '__main__':
    main()
