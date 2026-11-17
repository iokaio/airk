# SPDX-License-Identifier: Apache-2.0
"""Prepare only a frozen corrected-production P14 Neo source batch; never SSH or Cargo."""
import argparse
import ast
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile

BASE = Path(__file__).resolve().parent
WORK = BASE / 'p10-worktree'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    with path.open('x', encoding='utf-8') as output:
        json.dump(value, output, indent=2)
        output.write('\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--label', required=True, help='Fresh p14-native-* label')
    parser.add_argument('--source-sha256', required=True)
    parser.add_argument('--source-files', type=int, required=True)
    parser.add_argument('--debug-receipt', type=Path, default=BASE / 'p14-cold-model-debug-semantic-r2.receipt.json')
    parser.add_argument('--clippy-receipt', type=Path, default=BASE / 'p14-cold-model-clippy-r2.receipt.json')
    parser.add_argument('--p12-acceptance', type=Path, default=BASE / 'p12-local-acceptance-commit-r1.json')
    parser.add_argument('--p13-decision', type=Path, default=BASE / 'p13-decision-r1.json')
    args = parser.parse_args()
    assert re.fullmatch(r'p14-native-[a-z0-9-]+', args.label)
    assert re.fullmatch(r'[0-9a-f]{64}', args.source_sha256)
    expected = {'sha256': args.source_sha256, 'files': args.source_files}
    frozen = {'sha256': 'd4e48a534881ea910bcbadd182e8fb4f6a970c844df6ab1951d7b566cbcb2eeb', 'files': 960}
    assert expected == frozen, 'This separately reviewed source batch is frozen at the actual cold-model correction'
    accepted = json.loads(args.p12_acceptance.read_text(encoding='utf-8'))
    assert accepted['phase'] == 'P12' and accepted['status'] == 'accepted'
    qualification = Path(accepted['qualification'])
    assert digest(qualification) == accepted['qualification_sha256']
    q = json.loads(qualification.read_text(encoding='utf-8'))
    assert q['status'] == 'passed' and q['source'] == accepted['compiled_runtime_source']
    decision = json.loads(args.p13_decision.read_text(encoding='utf-8'))
    assert decision['phase'] == 'P13' and decision['decision'] == 'retain_current' and decision['production_changed'] is False
    current_receipts = {}
    for kind, receipt_path in [('debug_semantic', args.debug_receipt), ('workspace_clippy', args.clippy_receipt)]:
        receipt_path = receipt_path.resolve(strict=True)
        observed = json.loads(receipt_path.read_text(encoding='utf-8'))
        assert observed['exit_code'] == observed['actual_exit_code'] == 0 and observed['validation_error'] is None
        assert observed['source_before'] == observed['source_after'] == expected and observed['source_unchanged']
        assert Path(observed['target']).resolve() == (BASE / 'p10-target').resolve()
        log = receipt_path.with_name(receipt_path.name.replace('.receipt.json', '.log'))
        assert digest(log) == observed['log_sha256']
        command = observed['command']; assert Path(command[0]).stem == 'cargo'
        if kind == 'debug_semantic':
            assert command[1:] == ['test', '--locked', '--offline', '--timings', '--all-features', '-p', 'airk-cli', '--test', 'combined_reads', 'split_semantic_ready_destination_has_real_paid_vectors_and_complete_transports', '--', '--test-threads=1']
            assert observed['passing_tests'] == 1
        else:
            assert command[1:] == ['clippy', '--workspace', '--all-targets', '--all-features', '--locked', '--offline', '--timings', '--', '-D', 'warnings']
        current_receipts[kind] = {'path': str(receipt_path), 'sha256': digest(receipt_path), 'log_sha256': digest(log)}
    images = {name: digest(BASE / 'p10-target/debug' / name) for name in ['airk.exe', 'airk-embed.exe']}
    focused_path = args.debug_receipt.resolve(strict=True)
    prerequisites = {'p12_accepted_historical': {'path': str(args.p12_acceptance), 'sha256': digest(args.p12_acceptance)},
        'p13_retained_historical': {'path': str(args.p13_decision), 'sha256': digest(args.p13_decision)},
        'current_source_receipts': current_receipts, 'cargo_debug_images_sha256': images,
        'classification': 'CURRENT SOURCE TESTS. Production changed: split_execution provisions bundled default model. P12 production equivalence/release runtime reuse is explicitly NOT claimed. Fresh P14 optimized producer and later release qualification remain mandatory.',
        'candidate_environment': 'Parent observed matched debug focus with AIRK_CATALOG_CANDIDATE_BINARY and AIRK_TEST_BINARY unset; command uses Cargo --test combined_reads. Neo independently unsets both.'}
    # Reuse the exact reviewed local framing, without invoking its queue/main.
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location('p14_fingerprint', BASE / 'p12-focused-queue-r2.py')
    helper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    env = dict(os.environ, GIT_CONFIG_GLOBAL=str(BASE / 'p10-owned-ignore-controls/global.gitconfig'))
    assert helper.native_identity(env) == expected
    previous = json.loads((BASE / 'p11-native-20261106-r1-plan.json').read_text(encoding='utf-8'))
    plan = {key: previous[key] for key in ['working_version', 'runner_arguments', 'environment']}
    plan.update(phase='P14', draft=False, pending=[], native_source=expected,
                steps=[{'name': 'fmt', 'command': ['cargo', 'fmt', '--all', '--', '--check']}],
                not_run=['Scoped arm64 Neo source/OS custody/native/helper fixtures only. Actual bundled ARM default-model readiness is exercised by semantic-true only; no full Mac workspace, installed release, resource comparison, Linux/Intel/signing/editor or complete P14 matrix claim.'])
    # Compile only each owning target; never all67 package test binaries on Neo.
    # Keep all-features and one target directory through the batch.
    plan['prerequisites'] = prerequisites
    cases = [
        ('actual-provider-false-cutover', 'airk-cli', ['--test', 'combined_reads'], 'split::split_actual_service_provider_cutover_retains_complete_rpc_cli_mcp_evidence', 1),
        ('actual-semantic-true-destination', 'airk-cli', ['--test', 'combined_reads'], 'split::split_semantic_ready_destination_has_real_paid_vectors_and_complete_transports', 1)]
    for name, package, target, pattern, count in cases:
        plan['steps'].append({'name': name, 'expected_passing_tests': count,
            'command': ['cargo', 'test', '-p', package, *target, '--locked', '--offline', '--timings',
                        '--all-features', pattern, '--', '--exact', '--test-threads=1']})
    assert len(plan['steps']) == 3 and sum(row.get('expected_passing_tests', 0) for row in plan['steps']) == 2
    # Existing owner cleanup engine is preserved with the new exact run prefix:
    # even disk-full log failures, exact run target and target-absence verification.
    owner = BASE / 'p14-neo-owner.py'
    launcher = BASE / 'p14-neo-launch.py'
    runner = (BASE / 'p11-native-20261106-r1-runner.py').read_text(encoding='utf-8')
    runner = runner.replace('P11a', 'P14').replace('p11a.neo-qualification', 'p14.neo-qualification').replace('p11-native-', 'p14-native-')
    anchor = "    environment.pop('AIRK_TEST_BINARY', None)"
    assert runner.count(anchor) == 1
    runner = runner.replace(anchor, anchor + "\n    environment.pop('AIRK_CATALOG_CANDIDATE_BINARY', None)\n    environment.pop('AIRK_P10_RESOURCE_OUTPUT_DIR', None)\n    environment.pop('AIRK_P11B_RESOURCE_OUTPUT_DIR', None)")
    ast.parse(runner)
    for suffix in ['.tar.gz', '-snapshot.json', '-plan.json', '-runner.py', '-prepared.json', '-active.json']:
        assert not (BASE / (args.label + suffix)).exists(), 'Preserve old campaign artifacts'
    names = subprocess.check_output(['git', '-c', 'safe.directory=' + WORK.as_posix(),
            'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=WORK, env=env).split(b'\0')
    excluded = {'.git', '.codex', '.agents', '.aws', '.mcp.json', '.ssh', 'temp', 'target', 'artifacts', 'node_modules', '__pycache__'}
    names = sorted({os.fsdecode(name) for name in names if name and not (set(Path(os.fsdecode(name)).parts) & excluded)})
    archive = BASE / (args.label + '.tar.gz')
    manifest = {}
    with tarfile.open(archive, 'x:gz') as output:
        for name in names:
            path = WORK / name
            assert path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(WORK.resolve())
            manifest[name] = digest(path)
            output.add(path, arcname=name, recursive=False)
    assert helper.native_identity(env) == expected
    assert all(digest(WORK / name) == value for name, value in manifest.items())
    save(BASE / (args.label + '-snapshot.json'), {'entry_commit': subprocess.check_output([
        'git', '-c', 'safe.directory=' + WORK.as_posix(), 'rev-parse', 'HEAD'], cwd=WORK, env=env).decode().strip(),
        'native_source': expected, 'files': manifest, 'archive_sha256': digest(archive)})
    save(BASE / (args.label + '-plan.json'), plan)
    with (BASE / (args.label + '-runner.py')).open('x', encoding='utf-8') as output:
        output.write(runner)
    artifacts = [BASE / (args.label + suffix) for suffix in ['.tar.gz', '-snapshot.json', '-plan.json', '-runner.py']]
    save(BASE / (args.label + '-prepared.json'), {'phase': 'P14', 'prepared_only': True, 'ssh_launched': False,
        'native_source': expected, 'matched_debug_semantic_receipt': str(focused_path), 'prerequisites': prerequisites,
        'steps': 3, 'expected_passing_tests': 2, 'public_files': len(manifest),
        'artifacts': {path.name: digest(path) for path in artifacts},
        'owner': {'path': str(owner), 'sha256': digest(owner), 'modification': 'none'},
        'launcher': {'path': str(launcher), 'sha256': digest(launcher)},
        'cleanup': 'Existing remote owner always cargo clean exact target in finally, preserve logs/timings and require target absence. Parent collects cleanup.json and clean exit0; failed/incomplete cleanup never qualifies.'})
    print(json.dumps({'prepared_only': True, 'label': args.label, 'source': expected, 'steps': 3, 'tests': 2,
        'launch_later_by_parent': f'python {launcher} launch {args.label}',
        'collect_later_by_parent': f'python {launcher} collect {args.label}'}))


if __name__ == '__main__':
    main()
