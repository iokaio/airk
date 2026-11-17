# SPDX-License-Identifier: Apache-2.0
"""Reuse an unchanged sealed runtime after an independently reviewed cfg(test)-only correction."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[2]
TEST_FILES = {
    'crates/airk-cli/src/project_supervisor_guarded_qualification_tests.rs',
    'crates/airk-cli/src/project_supervisor_modern_checkpoint_tests.rs',
}

def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--freeze', type=Path, required=True)
    parser.add_argument('--verify-only', action='store_true')
    parser.add_argument('name')
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    assert args.name and '/' not in args.name and '\\' not in args.name and (args.command or args.verify_only)
    expected = json.loads(args.freeze.read_text())
    specification = importlib.util.spec_from_file_location('reuse_freeze', BASE / 'completion-freeze.py')
    freeze = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(freeze)
    before = freeze.observe()
    differences = {}
    for group, files in expected['groups'].items():
        current = before['groups'][group]
        assert files.keys() == current.keys(), 'Changed frozen input roster'
        for name, digest in files.items():
            if current[name] != digest:
                assert group == 'native' and name in TEST_FILES, 'Production/harness input changed: ' + name
                differences[name] = {'compiled': digest, 'current_test': current[name]}
    assert differences, 'This owner is only for an explicit test-source correction'
    # These external modules are below an unchanged production cfg(test) gate;
    # the reviewer verifies this exact delta before the second component run.
    parent = (ROOT / 'crates/airk-cli/src/project_supervisor.rs').read_text()
    assert '#[cfg(all(test, any(feature = "embedding-worker", feature = "local-inference")))]\n#[path = "project_supervisor_guarded_qualification_tests.rs"]' in parent
    runtime = args.runtime.resolve(strict=True)
    manifest_file = runtime / 'airk-install.json'
    manifest_digest = sha(manifest_file)
    assert manifest_digest == '2069c2d6e3acc48bba07a808fae4b160c46def14b76b03cb12a768357cf2138a'
    manifest = json.loads(manifest_file.read_text())
    build = json.loads((runtime / 'airk-build.json').read_text())
    assert build['source'] == expected['compiled_native_source']
    def verify_runtime():
        assert sha(manifest_file) == manifest_digest
        actual = set()
        for path in runtime.rglob('*'):
            assert not path.is_symlink() and path.resolve().is_relative_to(runtime)
            if path.is_file():
                actual.add(path.relative_to(runtime).as_posix())
        assert actual == set(manifest['files']) | {'airk-install.json'}
        for name, digest in manifest['files'].items():
            assert sha(runtime / name) == digest, 'Runtime seal changed: ' + name
    verify_runtime()
    proof_file = BASE / (args.name + '-reuse-proof.json')
    proof = {'schema': 'airk-reviewed-test-only-reuse/1', 'owner_sha256': sha(Path(__file__)),
        'original_runtime_manifest_sha256': manifest_digest,
        'original_compiled_source': build['source'], 'current_test_source': before['compiled_native_source'],
        'test_only_changes': differences, 'sealed_file_count': len(manifest['files']),
        'reuse': 'Original r26 binary/build identity retained; only external cfg(test) source changed',
        'before': True, 'after': False, 'command': args.command, 'verification_only': args.verify_only}
    with proof_file.open('x') as output:
        json.dump(proof, output, indent=2)
    environment = dict(os.environ)
    environment.update(AIRK_TEST_BINARY=str(runtime / 'airk.exe'), AIRK_NATIVE_TEST_BINARY=str(runtime / 'airk.exe'),
        AIRK_TEST_F09_QUALIFICATION='1', AIRK_TEST_NATIVE_SOURCE_SHA256=build['source']['sha256'],
        AIRK_TEST_NATIVE_SOURCE_FILES=str(build['source']['files']), AIRK_TEST_INSTALL_MANIFEST_SHA256=manifest_digest)
    if args.verify_only:
        from types import SimpleNamespace
        result = SimpleNamespace(returncode=0)
    else:
        result = subprocess.run([sys.executable, str(BASE / 'run-qualification-gate.py'), args.name, *args.command], env=environment)
    try:
        assert sha(Path(__file__)) == proof['owner_sha256'], 'Reuse owner changed during execution'
        assert freeze.observe() == before, 'Inputs changed during test-only runtime reuse'
        verify_runtime()
        proof['after'] = True
    except BaseException as error:
        proof['verification_error'] = {'type': type(error).__name__, 'message': str(error)[:4096]}
        raise
    finally:
        proof['exit_code'] = result.returncode
        proof_file.write_text(json.dumps(proof, indent=2) + '\n')
        receipt_file = BASE / (args.name + '.json')
        if receipt_file.exists():
            receipt = json.loads(receipt_file.read_text())
            receipt['runtime_reuse_proof'] = proof_file.name
            receipt['runtime_reuse_inputs_unchanged'] = proof['after']
            receipt['runtime_reuse'] = {key: proof[key] for key in ('original_runtime_manifest_sha256',
                'original_compiled_source', 'current_test_source', 'test_only_changes', 'reuse', 'before', 'after')}
            receipt_file.write_text(json.dumps(receipt, indent=2) + '\n')
    raise SystemExit(result.returncode)

if __name__ == '__main__':
    main()
