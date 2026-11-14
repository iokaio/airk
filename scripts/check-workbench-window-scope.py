# SPDX-License-Identifier: Apache-2.0
"""Qualify ordinary Workbench window-scope reads against one sealed native pair.

Starts only a disposable, owner-private fixture via the existing client harness.
The Node peer uses compiled production Workbench/WindowScope/adapter/session code;
its VS Code host is injected, so this is not installed-editor/DOM qualification.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


def sha256(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def input_inventory():
    """Seal the compiled tree and harness owners, including newly added helpers."""
    compiled = ROOT / 'packages/vscode/out'
    if not compiled.is_dir() or not any(compiled.iterdir()):
        raise ValueError('Compile the extension before native Workbench qualification')
    paths = [path for path in (ROOT / 'scripts').rglob('*')
             if path.is_file() and path.suffix in ('.py', '.cjs', '.mjs', '.json')]
    paths.extend(path for path in compiled.rglob('*') if path.is_file())
    paths.extend(ROOT / name for name in ('packages/vscode/package.json', 'packages/vscode/package-lock.json'))
    return {path.relative_to(ROOT).as_posix(): sha256(path) for path in sorted(paths)}


def qualify(runtime, node, report, target):
    clients = module('workbench_native_clients', ROOT / 'scripts/check-service-clients.py')
    package = module('workbench_native_package', ROOT / 'scripts/package-native.py')
    runtime = runtime.resolve(strict=True)
    metadata = json.loads((runtime / 'airk-runtime.json').read_bytes())
    if metadata.get('schema') != 3:
        raise ValueError('A matched schema-3 airk/airkctl pair is required')
    build = json.loads((runtime / 'airk-build.json').read_bytes())
    # Exact original-source identity: reuse is never relabeled as a current build.
    files, _ = package.validate_runtime(runtime, metadata['version'], target, build['source'])
    expected_runtime = dict(files)
    before = input_inventory()
    with clients.owned_client_fixture() as directory:
        owner = Path(directory).resolve(strict=True)
        (owner / 'data').mkdir(mode=0o700)
        roots, source_hashes = [], {}
        for ordinal in ('a', 'b'):
            source = owner / ('source-' + ordinal)
            source.mkdir(mode=0o700)
            content = ('def workbench_fixture():\n    return "workbenchneedle-' + ordinal + '"\n').encode()
            file = source / 'fixture.py'
            file.write_bytes(content)
            source_hashes[str(file)] = sha256(file)
            roots.append(source)
        with clients.Fixture(runtime, owner) as fixture:
            assert fixture.cli is not None and fixture.catalog_authority
            for name, source in zip(('workbench-a', 'workbench-b'), roots):
                fixture.register(name, source, owner / 'data')
                fixture.job(name, 'index')
                fixture.published_status(name)
            output = owner / 'workbench-observations.json'
            request = {
                'runtime': str(runtime), 'owner': str(owner), 'cli': str(fixture.cli),
                'cli_sha256': sha256(fixture.cli), 'version': metadata['version'],
                'control': str(fixture.control.resolve(strict=True)), 'project': fixture.projects['workbench-a'],
                'selection': fixture.scopes['workbench-a'], 'other_selection': fixture.scopes['workbench-b'],
                'source_hashes': source_hashes, 'output': str(output),
            }
            request_path = owner / 'workbench-request.json'
            request_path.write_text(json.dumps(request, ensure_ascii=False) + '\n', encoding='utf-8')
            run = subprocess.run([str(node), str(ROOT / 'scripts/workbench-window-scope.cjs'), str(request_path)],
                                 cwd=ROOT, env=fixture.environment, text=True, capture_output=True,
                                 timeout=240, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
            print(run.stdout, end='')
            if run.returncode:
                raise AssertionError('Real Workbench peer failed: ' + run.stderr[-8192:])
            observations = json.loads(output.read_bytes())
            assert observations['exactComparisons'] >= 10, 'Zero/partial native comparison selection'
            assert all(sha256(file) == expected for file, expected in source_hashes.items()), 'Indexed source was modified'
            fixture.assert_alive()
    # Both the native owner and its temporary directory must close successfully
    # before any pass receipt is written. Failed cleanup retains owned evidence.
    after, _ = package.validate_runtime(runtime, metadata['version'], target, build['source'])
    assert after == expected_runtime, 'Matched runtime changed during native qualification'
    assert before == input_inventory(), 'Compiled adapter/harness changed during qualification'
    report.parent.mkdir(parents=True, exist_ok=True)
    with report.open('x', encoding='utf-8') as destination:
        json.dump({'schema': 1, 'runtime': {'version': metadata['version'], 'source': build['source'],
            'files': expected_runtime}, 'inputs': before, 'node': {'version': subprocess.check_output(
            [str(node), '--version'], text=True).strip(), 'sha256': sha256(node)},
            'source_preserved': True, 'fixture_cleanup': 'completed', 'observations': observations}, destination, indent=2)
        destination.write('\n')
    print('running 1 test\ntest result: ok. 1 passed; 0 failed')


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--runtime-dir', type=Path, required=True)
    parser.add_argument('--node', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--target', default='x86_64-pc-windows-msvc')
    args = parser.parse_args()
    node = args.node.resolve(strict=True)
    if subprocess.check_output([str(node), '--version'], text=True).strip() != 'v24.15.0':
        parser.error('Pinned Node 24.15.0 is required')
    qualify(args.runtime_dir, node, args.report.resolve(), args.target)


if __name__ == '__main__':
    main()
