# SPDX-License-Identifier: Apache-2.0
"""Prepare only a frozen scoped P12 Neo source batch; never SSH or Cargo."""
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
    parser.add_argument('--label', required=True, help='Fresh p12-native-* label')
    parser.add_argument('--source-sha256', required=True)
    parser.add_argument('--source-files', type=int, required=True)
    parser.add_argument('--focused-result', type=Path, required=True)
    args = parser.parse_args()
    assert re.fullmatch(r'p12-native-[a-z0-9-]+', args.label)
    assert re.fullmatch(r'[0-9a-f]{64}', args.source_sha256)
    expected = {'sha256': args.source_sha256, 'files': args.source_files}
    focused_path = args.focused_result.resolve(strict=True)
    focused = json.loads(focused_path.read_text(encoding='utf-8'))
    assert focused['status'] == 'passed' and focused['source_expected'] == expected
    assert focused['script_sha256'] == digest(BASE / 'p12-focused-queue-r2.py'), 'Use the corrected owning39-case Windows queue receipt'
    assert len(focused['required_new_tests']) == 18
    tests = next(row for row in focused['steps'] if row['name'] == 'tests')
    assert tests['cases'] == 39 and tests['candidate_environment_unset']
    inventory = json.loads(Path(tests['inventory']).read_text(encoding='utf-8'))
    for name in focused['required_new_tests']:
        assert sum(row['name'].rsplit('::', 1)[-1] == name and not row['failed'] and not row['skipped'] for row in inventory) == 1
    assert digest(focused_path.parent / 'junit.xml') == tests['junit_sha256']
    # Reuse the exact reviewed local framing, without invoking its queue/main.
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location('p11b_focused', BASE / 'p12-focused-queue-r2.py')
    helper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    env = dict(os.environ, GIT_CONFIG_GLOBAL=str(BASE / 'p10-owned-ignore-controls/global.gitconfig'))
    assert helper.native_identity(env) == expected
    previous = json.loads((BASE / 'p11-native-20261106-r1-plan.json').read_text(encoding='utf-8'))
    plan = {key: previous[key] for key in ['working_version', 'runner_arguments', 'environment']}
    plan.update(phase='P12', draft=False, pending=[], native_source=expected,
                steps=[{'name': 'fmt', 'command': ['cargo', 'fmt', '--all', '--', '--check']}],
                not_run=['Scoped arm64 Neo source/OS custody/native/helper fixtures only. No full Mac workspace, installed release, default ONNX, resource comparison, Linux/Intel/signing/editor or P14 cumulative qualification claim.'])
    # Compile only each owning target; never all67 package test binaries on Neo.
    # Keep all-features and one target directory through the batch.
    # Select the true owning target from observed Nextest JUnit classnames,
    # cross-checked against live source paths. Never substitute --bin airk.
    catalog = json.loads((BASE / 'p12-focused-selection/added-tests.json').read_text(encoding='utf-8'))
    assert len(catalog) == 18
    for row in catalog:
        package = row['file'].split('/')[1]
        match = [item for item in inventory if item['name'].rsplit('::',1)[-1] == row['test']]
        assert len(match) == 1 and not match[0]['failed'] and not match[0]['skipped']
        expected_class = package + ('::combined_reads' if '/tests/combined_reads/' in row['file'] else '')
        assert match[0]['classname'] == expected_class, (row, match[0])
    cases = [
        ('core-selection-golden-contract', 'airk-core', ['--lib'], 'catalog_refresh::tests::', 4),
        ('core-publication-binding', 'airk-core', ['--lib'], 'control_publication_binding_rejects_historical_cross_incarnation_and_successor', 1),
        ('ingest-control-typed-errors', 'airk-ingest', ['--lib'], 'control_snapshot::tests::', 3),
        ('ingest-unix-control-alias', 'airk-ingest', ['--lib'], 'positive_partition_control_alias_remains_unavailable', 1),
        ('store-exact-refresh-receipts', 'airk-store', ['--lib'], 'saved_refresh_result_recovers_exact_publication_and_typed_refusal_never_becomes_success', 1),
        ('store-admission-pause-isolation', 'airk-store', ['--lib'], 'generic_configuration_cannot_admit_refresh_or_consume_unrelated_work', 1),
        ('kernel-guarded-refresh', 'airk-kernel', ['--lib'], 'service_reference_refresh::tests::', 6),
        ('kernel-semantic-publication-carry', 'airk-kernel', ['--lib'], 'refresh_control_baseline_survives_real_shared_model_semantic_metadata_and_vector_batch_then_selected_update', 1),
        ('actual-native-helper-refresh-restart-parity', 'airk-cli', ['--test', 'combined_reads'], 'actual_native_refresh_keeps_profile_scope_shared_model_and_exact_receipts_across_restart', 1)]
    for name, package, target, pattern, count in cases:
        plan['steps'].append({'name': name, 'expected_passing_tests': count,
            'command': ['cargo', 'test', '-p', package, *target, '--locked', '--offline', '--timings',
                        '--all-features', pattern, '--', '--test-threads=1']})
    assert len(plan['steps']) == 10 and sum(row.get('expected_passing_tests', 0) for row in plan['steps']) == 19
    # Existing owner is reused byte-for-byte: unconditional finally cargo clean,
    # even disk-full log failures, exact run target and target-absence verification.
    owner = BASE / 'p12-neo-owner.py'
    launcher = BASE / 'p12-neo-launch.py'
    runner = (BASE / 'p11-native-20261106-r1-runner.py').read_text(encoding='utf-8')
    runner = runner.replace('P11a', 'P12').replace('p11a.neo-qualification', 'p12.neo-qualification').replace('p11-native-', 'p12-native-')
    anchor = "    environment.pop('AIRK_TEST_BINARY', None)"
    assert runner.count(anchor) == 1
    runner = runner.replace(anchor, anchor + "\n    environment.pop('AIRK_CATALOG_CANDIDATE_BINARY', None)\n    environment.pop('AIRK_P10_RESOURCE_OUTPUT_DIR', None)")
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
    save(BASE / (args.label + '-prepared.json'), {'phase': 'P12', 'prepared_only': True, 'ssh_launched': False,
        'native_source': expected, 'focused_windows_result': str(focused_path), 'focused_result_sha256': digest(focused_path),
        'steps': 10, 'expected_passing_tests': 19, 'public_files': len(manifest),
        'artifacts': {path.name: digest(path) for path in artifacts},
        'owner': {'path': str(owner), 'sha256': digest(owner), 'modification': 'none'},
        'launcher': {'path': str(launcher), 'sha256': digest(launcher)},
        'cleanup': 'Existing remote owner always cargo clean exact target in finally, preserve logs/timings and require target absence. Parent collects cleanup.json and clean exit0; failed/incomplete cleanup never qualifies.'})
    print(json.dumps({'prepared_only': True, 'label': args.label, 'source': expected, 'steps': 10, 'tests': 19,
        'launch_later_by_parent': f'python {launcher} launch {args.label}',
        'collect_later_by_parent': f'python {launcher} collect {args.label}'}))


if __name__ == '__main__':
    main()
