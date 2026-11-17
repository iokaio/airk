# SPDX-License-Identifier: Apache-2.0
"""Seal all generated inputs and match tests to the complete live source catalog."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def observe(expected):
    source = ROOT / 'packages/vscode/src/test'
    out = ROOT / 'packages/vscode/out'
    for directory in (source, out):
        if (not directory.is_dir() or directory.is_symlink()
                or directory.resolve() != directory.absolute()):
            raise ValueError('Missing or redirected extension input root')
    sources = {}
    expected_paths = []
    for path in sorted(source.rglob('*')):
        if path.is_symlink() or path.resolve() != path.absolute():
            raise ValueError('Redirected test source input')
        if not path.is_file() or not path.name.endswith('.test.ts'):
            continue
        if path.parent != source:
            raise ValueError('Unexpected test source location')
        sources[path.relative_to(ROOT).as_posix()] = digest(path)
        expected_paths.append('packages/vscode/out/test/' + path.stem + '.js')
    if len(sources) != expected or expected <= 0:
        raise ValueError(f'Frozen test source count changed: {len(sources)} != {expected}')
    files = {}
    for path in sorted(out.rglob('*')):
        if path.is_symlink() or path.resolve() != path.absolute():
            raise ValueError('Redirected compiled extension input')
        if path.is_file():
            files[path.relative_to(ROOT).as_posix()] = {
                'bytes': path.stat().st_size, 'sha256': digest(path)}
            if (path.name.endswith('.test.js') and (out / 'test') in path.parents
                    and path.parent != out / 'test'):
                raise ValueError('Unexpected nested compiled test location')
    tests = sorted(name for name in files
                   if name.startswith('packages/vscode/out/test/')
                   and Path(name).parent.as_posix() == 'packages/vscode/out/test'
                   and name.endswith('.test.js'))
    if tests != sorted(expected_paths):
        raise ValueError('Compiled test catalog differs from complete frozen source catalog')
    return files, sources, [str(ROOT / name) for name in tests]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--before', type=Path)
    parser.add_argument('--expected-tests', type=int, required=True)
    args = parser.parse_args()
    files, sources, tests = observe(args.expected_tests)
    result = {'schema': 1, 'compiled_files': files, 'test_sources': sources,
              'test_paths': tests, 'selected_test_files': len(tests),
              'helper_sha256': digest(Path(__file__))}
    if args.before:
        prior = json.loads(args.before.read_text(encoding='utf-8'))
        result['inputs_unchanged'] = all(result[key] == prior[key] for key in (
            'compiled_files', 'test_sources', 'test_paths',
            'selected_test_files', 'helper_sha256'))
        result['before_receipt_sha256'] = digest(args.before)
    with args.output.open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print(json.dumps({'compiled_files': len(files),
                      'selected_test_files': len(tests),
                      'inputs_unchanged': result.get('inputs_unchanged'),
                      'receipt_sha256': digest(args.output),
                      'helper_sha256': result['helper_sha256']}))
    if result.get('inputs_unchanged') is False:
        raise ValueError('Consumed compiled extension inputs changed')


if __name__ == '__main__':
    main()
