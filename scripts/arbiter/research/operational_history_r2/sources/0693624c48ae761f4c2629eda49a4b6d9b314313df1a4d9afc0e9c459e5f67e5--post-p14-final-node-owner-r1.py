# SPDX-License-Identifier: Apache-2.0
"""Full compiled Node suite with the sealed binary set after p11 environment sanitation.

Only invokes existing compiled tests. No compile, Cargo, normal installation,
retry, process cleanup policy or publication is added here.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

BASE = Path(__file__).resolve().parent
WORK = BASE / 'p10-worktree'


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--source-sha256', required=True)
    parser.add_argument('--source-files', type=int, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    expected = {'sha256': args.source_sha256, 'files': args.source_files}
    assert os.name == 'nt' and re.fullmatch('[0-9a-f]{64}', args.source_sha256)
    runtime = args.runtime.resolve(strict=True)
    output = args.output.absolute()
    assert output.is_relative_to(BASE) and output != BASE
    # Reserve this receipt before starting any fixture. An error remains failed.
    with output.open('x', encoding='utf-8') as receipt:
        result = {'status': 'failed', 'source_expected': expected}
        try:
            builder = load('node_owner_builder', WORK / 'scripts/build-service-runtime.py')
            packager = load('node_owner_packager', WORK / 'scripts/package-native.py')
            assert builder.source_identity() == expected
            seal = packager.validate_runtime(runtime, '1.8.8', 'x86_64-pc-windows-msvc', expected)[0]
            tests = sorted((WORK / 'packages/vscode/out/test').glob('*.test.js'))
            assert tests
            inventory = {str(path.relative_to(WORK)): sha(path) for path in tests}
            environment = os.environ.copy()
            # p11-local-command intentionally removes inherited AIRK_TEST_BINARY.
            # Set it here, after that wrapper, for this child and its test fixtures.
            environment['AIRK_TEST_BINARY'] = str(runtime / 'airk.exe')
            assert sha(runtime / 'airk.exe') == seal['airk.exe']
            command = ['node', '--test', '--test-reporter=tap', '--test-concurrency=2', *map(str, tests)]
            log = output.with_suffix('.node.log')
            started = time.monotonic()
            with log.open('x', encoding='utf-8') as stream:
                code = subprocess.run(command, cwd=WORK, env=environment, stdout=stream,
                    stderr=subprocess.STDOUT, creationflags=subprocess.CREATE_NO_WINDOW).returncode
            text = log.read_text(encoding='utf-8', errors='strict')
            counts = {name: int(number) for name, number in re.findall(
                r'^# (tests|pass|fail|cancelled|skipped|todo) (\d+)\s*$', text, re.M)}
            # Record actual failures/skips before enforcing success; never retry.
            result.update(command=command, exit_code=code, seconds=time.monotonic() - started,
                AIRK_TEST_BINARY=environment['AIRK_TEST_BINARY'], runtime_seal=seal,
                test_files=inventory, counts=counts, log=str(log), log_sha256=sha(log))
            assert builder.source_identity() == expected
            assert packager.validate_runtime(runtime, '1.8.8', 'x86_64-pc-windows-msvc', expected)[0] == seal
            assert all(sha(WORK / name) == digest for name, digest in inventory.items())
            assert counts.get('tests', 0) > 0 and counts.get('pass', 0) > 0
            assert code == 0 and counts.get('fail') == 0 and counts.get('cancelled') == 0
            result['status'] = 'passed'
        except BaseException as error:
            result['error'] = repr(error)
        finally:
            json.dump(result, receipt, indent=2)
            receipt.write('\n')
    print(json.dumps(result), flush=True)
    return 0 if result['status'] == 'passed' else 1


if __name__ == '__main__':
    sys.exit(main())
