# SPDX-License-Identifier: Apache-2.0
"""Prepared P14 core qualification only. Never builds a release or retries a mutation."""
import argparse
import atexit
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import time
import xml.etree.ElementTree as ET

BASE = Path(__file__).resolve().parent
WORK = BASE / 'p10-worktree'
COMPILED = {'sha256': '4121692a2cc0272358023f5362ae886eeed4fd06203cea7af23bbfc5f4be86b9', 'files': 960}
ALLOWED = 'crates/airk-cli/tests/combined_reads/split.rs'
FOCUS = 'split_semantic_ready_destination_has_real_paid_vectors_and_complete_transports'
LONG = ['two_hundred_current_edits_reach_object_and_database_plateau', 'compact_measurements_compare_small_reads_from_large_catalog', 'manual_backlog_larger_than_one_page_advances_without_unprotected_deletion', 'measure_small_and_larger_sparse_catalog_edits']

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

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-hash', required=True)
    parser.add_argument('--source-files', type=int, required=True)
    parser.add_argument('--focus-pair', type=Path, required=True)
    parser.add_argument('--class-seal', type=Path, required=True)
    parser.add_argument('--class-seal-sha256', required=True)
    parser.add_argument('--p12-acceptance', type=Path, required=True)
    parser.add_argument('--p12-acceptance-sha256', required=True)
    parser.add_argument('--p13-decision', type=Path, required=True)
    parser.add_argument('--p13-decision-sha256', required=True)
    parser.add_argument('--prepared', type=Path, required=True)
    parser.add_argument('--label', required=True)
    args = parser.parse_args()
    assert args.label and all(c.isalnum() or c in '-_' for c in args.label)
    output = BASE / (args.label + '.json')
    assert not output.exists(), 'Fresh immutable queue label required'
    prepared = read(args.prepared)
    assert prepared['runner_sha256'] == sha(__file__)
    for path, digest in prepared['pins'].items():
        assert sha(path) == digest, 'Prepared helper/driver drift: ' + path
    assert sha(args.class_seal) == args.class_seal_sha256
    assert sha(args.p12_acceptance) == args.p12_acceptance_sha256
    assert sha(args.p13_decision) == args.p13_decision_sha256
    source = {'sha256': args.source_hash, 'files': args.source_files}
    acceptance = read(args.p12_acceptance)
    assert acceptance['phase'] == 'P12' and acceptance['status'] == 'accepted'
    assert acceptance['compiled_runtime_source'] == COMPILED
    qualified = Path(acceptance['qualification'])
    assert sha(qualified) == acceptance['qualification_sha256']
    qualification = read(qualified)
    assert qualification['status'] == 'passed' and qualification['source'] == COMPILED, 'P12 qualification incomplete/wrong source'
    decision = read(args.p13_decision)
    assert decision['phase'] == 'P13' and decision['decision'] == 'retain_current'
    assert decision['production_changed'] is False, 'P12 reuse refuses a changed ranker'
    proof = read(args.class_seal)
    assert proof['compiled_runtime_source'] == COMPILED and proof['current_test_source'] == source
    assert set(proof['compiled_native_inputs']) == set(proof['current_native_inputs'])
    assert identity(proof['compiled_native_inputs']) == COMPILED
    assert identity(proof['current_native_inputs']) == source
    changed = [n for n in proof['current_native_inputs'] if proof['current_native_inputs'][n] != proof['compiled_native_inputs'][n]]
    assert changed == [ALLOWED], 'Only external split integration test delta permitted'
    assert Path(proof['runtime']).resolve() == args.focus_pair.resolve()
    release_path = Path(proof['release_receipt'])
    assert sha(release_path) == proof['release_receipt_sha256']
    release = read(release_path)
    assert release['exit_code'] == release['actual_exit_code'] == 0 and release['cargo_incremental'] == '0'
    assert release['source_before'] == release['source_after'] == COMPILED and release['source_unchanged']
    assert sha(proof['release_log']) == release['log_sha256']
    command = release['command']
    assert command[2:5] == ['--profile', 'release', '--destination'] and len(command) == 6
    assert (WORK / command[1]).resolve() == (WORK / 'scripts/build-service-runtime.py').resolve()
    assert Path(command[5]).resolve() == args.focus_pair.resolve()
    assert sha(proof['runtime_seal']) == proof['runtime_seal_sha256']
    original_seal = read(proof['runtime_seal'])
    os.environ['GIT_CONFIG_GLOBAL'] = str(BASE / 'p10-owned-ignore-controls/global.gitconfig')
    builder = load('p14_builder', WORK / 'scripts/build-service-runtime.py')
    sealer = load('p14_sealer', BASE / 'p9-integrated-checks.py')
    record = {'status': 'running', 'source': source, 'compiled_runtime_source': COMPILED,
              'class_seal_sha256': args.class_seal_sha256, 'steps': [],
              'limits': 'Core Windows qualification only; no installed editor, broader resource, Neo, signed release or platform claim.'}
    def save():
        pending = output.with_suffix('.pending')
        pending.write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')
        pending.replace(output)
    def pins():
        assert builder.source_identity() == source, 'Full declared native inventory changed'
        for name, digest in proof['current_native_inputs'].items():
            path = WORK / name
            assert not path.is_symlink() and path.resolve().is_relative_to(WORK.resolve())
            assert sha(path) == digest, 'Current native input drift: ' + name
        for path, digest in prepared['pins'].items():
            assert sha(path) == digest, 'Prepared helper drift: ' + path
        assert sealer.seal_runtime(args.focus_pair, COMPILED) == original_seal, 'Runtime inventory/source seal drift'
    pins()
    lock = BASE / 'p14-core-cargo-owner.lock'
    fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    os.write(fd, str(os.getpid()).encode())
    def unlock():
        os.close(fd)
        lock.unlink()
    atexit.register(unlock)
    env = os.environ.copy()
    env.update(CARGO_NET_OFFLINE='true', AIRK_PRIVATE_CENSUS_BASE=prepared['private_base_proof']['path'],
               AIRK_CATALOG_CANDIDATE_BINARY=str(args.focus_pair / 'airk.exe'))
    for key in ['AIRK_P10_RESOURCE_OUTPUT_DIR', 'AIRK_P11B_RESOURCE_OUTPUT_DIR', 'AIRK_TEST_BINARY']:
        env.pop(key, None)
    # p11-local-command resolves verified real-model/ONNX inputs from the pinned mapping.
    inputs = read(BASE / 'p9-clean-inputs.local.json')['inputs']
    assert (Path(inputs['local-vector-model']) / 'minilm-l6-v2-u8-v1').is_dir()
    assert Path(inputs['local-onnx-runtime']).is_dir()
    junit_path = WORK / 'target/nextest/ci/junit.xml'
    commands = [
        ('format', ['cargo', 'fmt', '--all', '--', '--check']),
        ('workspace-clippy', ['python', str(WORK / 'scripts/dev-check.py'), '--workspace', '--all-features', '--offline', '--skip-fmt', '--target-dir', str(BASE / 'p10-target')]),
        ('semantic-positive', ['cargo', 'test', '--locked', '--offline', '--timings', '--all-features', '-p', 'airk-cli', '--test', 'combined_reads', FOCUS, '--', '--test-threads=1', '--nocapture']),
        ('full-workspace-nextest', ['cargo', 'nextest', 'run', '--workspace', '--all-features', '--locked', '--offline', '--timings', '--profile', 'ci', '--ignore-default-filter', '--no-fail-fast', '--retries', '0', '--test-threads', '4']),
        ('serial-doctests', ['cargo', 'test', '--workspace', '--all-features', '--doc', '--locked', '--offline', '--timings', '--', '--test-threads=1'])]
    save()
    try:
        for name, command in commands:
            pins()
            private = prepared['private_base_proof']
            assert sha(private['receipt']) == private['receipt_sha256']
            observed = subprocess.run(['pwsh', '-NoProfile', '-File', private['verifier'], '-ExpectedReceiptSha256', private['receipt_sha256']], capture_output=True, text=True)
            assert observed.returncode == 0, observed.stdout + observed.stderr
            acl = json.loads(observed.stdout.lstrip('\ufeff'))
            assert all(acl[k] == private[k] for k in ['path', 'owner_sid', 'sddl']) and acl['acl_mutated'] is False
            log = BASE / (args.label + '-' + name + '.log')
            assert not log.exists() and not log.with_suffix('.receipt.json').exists()
            prior_junit = sha(junit_path) if junit_path.exists() else None
            started_ns = time.time_ns()
            result = subprocess.run(['python', str(BASE / 'p11-local-command.py'), log.name, *command], cwd=WORK, env=env)
            receipt_path = log.with_suffix('.receipt.json')
            receipt = read(receipt_path)
            row = {'name': name, 'receipt': str(receipt_path), 'exit_code': result.returncode, 'actual_exit_code': receipt['actual_exit_code'], 'log_sha256': sha(log)}
            record['steps'].append(row)
            save()
            assert receipt['command'] == command and receipt['source_before'] == receipt['source_after'] == source and receipt['source_unchanged']
            assert receipt['target'] == str(BASE / 'p10-target') and receipt['log_sha256'] == sha(log)
            pins()
            if name == 'full-workspace-nextest':
                assert junit_path.exists() and junit_path.stat().st_mtime_ns >= started_ns, 'Missing/stale Nextest JUnit'
                archive = BASE / (args.label + '-full-workspace.junit.xml')
                with archive.open('xb') as handle:
                    handle.write(junit_path.read_bytes())
                cases = ET.parse(archive).getroot().findall('.//testcase')
                row.update(junit=str(archive), junit_sha256=sha(archive), previous_junit_sha256=prior_junit,
                           cases=len(cases), failed=sum(c.find('failure') is not None or c.find('error') is not None for c in cases),
                           skipped=sum(c.find('skipped') is not None for c in cases))
                save()
                assert cases and row['failed'] == 0
                for required in [*LONG, FOCUS]:
                    matched = [c for c in cases if c.attrib['name'].split('::')[-1] == required]
                    assert len(matched) == 1 and all(matched[0].find(x) is None for x in ['failure', 'error', 'skipped']), 'Required case absent/skipped/failed: ' + required
            assert result.returncode == receipt['exit_code'] == receipt['actual_exit_code'] == 0, name
            if name == 'semantic-positive':
                assert receipt['passing_tests'] == 1, 'Semantic preflight must execute exactly one passing test'
            save()
        record['status'] = 'passed'
        save()
    except BaseException as error:
        record['status'] = 'failed'
        record['failure'] = str(error)
        save()
        raise

if __name__ == '__main__':
    main()
