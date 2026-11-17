# SPDX-License-Identifier: Apache-2.0
"""Retain every generated extension input consumed by the native Node suite."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def observe():
    out = ROOT / 'packages/vscode/out'
    files = {}
    for path in sorted(out.rglob('*')):
        if path.is_symlink():
            raise ValueError('Redirected compiled extension input')
        if path.is_file():
            files[path.relative_to(ROOT).as_posix()] = {
                'bytes': path.stat().st_size, 'sha256': digest(path)}
    tests = sorted(name for name in files
                   if name.startswith('packages/vscode/out/test/')
                   and Path(name).parent.as_posix() == 'packages/vscode/out/test'
                   and name.endswith('.test.js'))
    if len(tests) != 104:
        raise ValueError(f'Complete compiled test selection changed: {len(tests)}')
    return files, [str(ROOT / name) for name in tests]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--before', type=Path)
    args = parser.parse_args()
    files, tests = observe()
    result = {'schema': 1, 'compiled_files': files, 'test_paths': tests,
              'selected_test_files': len(tests), 'helper_sha256': digest(Path(__file__))}
    if args.before:
        prior = json.loads(args.before.read_text(encoding='utf-8'))
        result['inputs_unchanged'] = all(result[key] == prior[key] for key in (
            'compiled_files', 'test_paths', 'selected_test_files', 'helper_sha256'))
        result['before_receipt_sha256'] = digest(args.before)
    with args.output.open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print(json.dumps({'compiled_files': len(files), 'selected_test_files': len(tests),
                      'inputs_unchanged': result.get('inputs_unchanged'),
                      'receipt_sha256': digest(args.output),
                      'helper_sha256': result['helper_sha256']}))
    if result.get('inputs_unchanged') is False:
        raise ValueError('Consumed compiled extension inputs changed')


if __name__ == '__main__':
    main()
