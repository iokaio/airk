# SPDX-License-Identifier: Apache-2.0
"""Emit one fresh P14 test-only class seal after accepted P12 and retained P13. No builds."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess

BASE = Path(__file__).resolve().parent
WORK = BASE / 'p10-worktree'
SNAPSHOT = BASE / 'p12-native-20261106-r1-snapshot.json'
COMPILED = {'sha256': '4121692a2cc0272358023f5362ae886eeed4fd06203cea7af23bbfc5f4be86b9', 'files': 960}
TEST = 'crates/airk-cli/tests/combined_reads/split.rs'
PATHSPECS = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo', 'crates',
             'scripts/build-service-runtime.py', 'scripts/provision-onnx-runtime.py',
             'scripts/provision-local-model.py', 'scripts/int8-policy.py', 'scripts/macho-identity.py',
             'scripts/license-notices.py', 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md', 'third_party']

def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def identity(files):
    result = hashlib.sha256()
    for name in sorted(files):
        encoded = name.encode('utf-8')
        result.update(len(encoded).to_bytes(8, 'big'))
        result.update(encoded)
        result.update(bytes.fromhex(files[name]))
    return {'sha256': result.hexdigest(), 'files': len(files)}

def native_paths():
    # Exact builder source_identity pathspec and cached+untracked discovery; no filesystem crawl.
    raw = subprocess.check_output(['git', '-c', 'safe.directory=' + WORK.as_posix(),
                                   'ls-files', '-z', '--cached', '--others', '--exclude-standard', '--', *PATHSPECS], cwd=WORK)
    return sorted(set(os.fsdecode(value) for value in raw.split(b'\0') if value))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prepared', type=Path, required=True)
    parser.add_argument('--source-hash', required=True)
    parser.add_argument('--p12-acceptance', type=Path, required=True)
    parser.add_argument('--p12-acceptance-sha256', required=True)
    parser.add_argument('--p13-decision', type=Path, required=True)
    parser.add_argument('--p13-decision-sha256', required=True)
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--release-receipt', type=Path, required=True)
    parser.add_argument('--release-receipt-sha256', required=True)
    parser.add_argument('--release-log', type=Path, required=True)
    parser.add_argument('--runtime-seal', type=Path, required=True)
    parser.add_argument('--runtime-seal-sha256', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    assert output.parent == BASE.resolve() and not output.exists() and output.suffix == '.json', 'Fresh ignored top-level class-seal output only'
    prepared = read(args.prepared)
    assert prepared['generator_sha256'] == sha(__file__)
    for name, digest in prepared['pins'].items():
        assert sha(name) == digest, 'Prepared authority/helper drift: ' + name
    assert sha(args.p12_acceptance) == args.p12_acceptance_sha256
    acceptance = read(args.p12_acceptance)
    assert acceptance['phase'] == 'P12' and acceptance['status'] == 'accepted'
    assert acceptance['compiled_runtime_source'] == COMPILED
    qualification = Path(acceptance['qualification'])
    assert sha(qualification) == acceptance['qualification_sha256']
    q = read(qualification)
    assert q['status'] == 'passed' and q['source'] == COMPILED
    assert sha(args.p13_decision) == args.p13_decision_sha256
    decision = read(args.p13_decision)
    assert decision['phase'] == 'P13' and decision['decision'] == 'retain_current' and decision['production_changed'] is False
    assert args.runtime.is_absolute() and args.runtime.resolve() == (BASE / 'p12-runtime-final-r1').resolve()
    assert args.release_receipt.resolve() == (BASE / 'p12-release-runtime-r1.receipt.json').resolve()
    assert args.release_log.resolve() == (BASE / 'p12-release-runtime-r1.log').resolve()
    assert sha(args.release_receipt) == args.release_receipt_sha256
    release = read(args.release_receipt)
    assert release['actual_exit_code'] == release['exit_code'] == 0 and release['cargo_incremental'] == '0'
    assert release['source_before'] == release['source_after'] == COMPILED and release['source_unchanged']
    assert sha(args.release_log) == release['log_sha256']
    command = release['command']
    assert len(command) == 6 and command[2:5] == ['--profile', 'release', '--destination']
    assert (WORK / command[1]).resolve() == (WORK / 'scripts/build-service-runtime.py').resolve()
    assert Path(command[5]).resolve() == args.runtime.resolve()
    assert sha(args.runtime_seal) == args.runtime_seal_sha256
    os.environ['GIT_CONFIG_GLOBAL'] = str(BASE / 'p10-owned-ignore-controls/global.gitconfig')
    builder = load('p14_class_builder', WORK / 'scripts/build-service-runtime.py')
    sealer = load('p14_class_sealer', BASE / 'p9-integrated-checks.py')
    snapshot = read(SNAPSHOT)
    assert snapshot['native_source'] == COMPILED
    names = native_paths()
    assert len(names) == 960 and all(n in snapshot['files'] for n in names)
    compiled = {n: snapshot['files'][n] for n in names}
    assert identity(compiled) == COMPILED, 'Roster differs from original full compiled native identity'
    current = {}
    for name in names:
        path = WORK / name
        assert path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(WORK.resolve())
        current[name] = sha(path)
    source = identity(current)
    assert source == {'sha256': args.source_hash, 'files': 960} and builder.source_identity() == source
    changed = [n for n in names if compiled[n] != current[n]]
    assert changed == [TEST], 'P14 must already be applied, with EXACTLY external split.rs delta'
    seal = read(args.runtime_seal)
    assert sealer.seal_runtime(args.runtime, COMPILED) == seal
    proof = {'schema': 'airk.p14.test-only-class-seal/1', 'compiled_runtime_source': COMPILED,
             'current_test_source': source, 'compiled_native_inputs': compiled, 'current_native_inputs': current,
             'only_changed_native_inputs': changed, 'unchanged_native_inputs': 959,
             'runtime': str(args.runtime.resolve()), 'release_receipt': str(args.release_receipt.resolve()),
             'release_receipt_sha256': sha(args.release_receipt), 'release_log': str(args.release_log.resolve()),
             'runtime_seal': str(args.runtime_seal.resolve()), 'runtime_seal_sha256': sha(args.runtime_seal),
             'snapshot': str(SNAPSHOT), 'snapshot_sha256': sha(SNAPSHOT),
             'p12_acceptance': str(args.p12_acceptance.resolve()), 'p12_acceptance_sha256': args.p12_acceptance_sha256,
             'p13_decision': str(args.p13_decision.resolve()), 'p13_decision_sha256': args.p13_decision_sha256,
             'generator_sha256': sha(__file__), 'limits': 'Only external fixture changed; compiled release stays original P12 source. No new build or P14 qualification claimed.'}
    # Recheck every authority and current byte immediately before the sole output side effect.
    assert native_paths() == names and builder.source_identity() == source
    assert all(sha(WORK / n) == digest for n, digest in current.items())
    assert all(sha(n) == digest for n, digest in prepared['pins'].items())
    assert sha(args.p12_acceptance) == args.p12_acceptance_sha256 and sha(args.p13_decision) == args.p13_decision_sha256
    assert sha(qualification) == acceptance['qualification_sha256']
    assert sha(args.release_receipt) == args.release_receipt_sha256 and sha(args.release_log) == release['log_sha256']
    assert sha(args.runtime_seal) == args.runtime_seal_sha256 and sealer.seal_runtime(args.runtime, COMPILED) == seal
    with output.open('x', encoding='utf-8') as handle:
        handle.write(json.dumps(proof, indent=2) + '\n')
    print(json.dumps({'output': str(output), 'sha256': sha(output), 'compiled_runtime_source': COMPILED, 'current_test_source': source}))

if __name__ == '__main__':
    main()
