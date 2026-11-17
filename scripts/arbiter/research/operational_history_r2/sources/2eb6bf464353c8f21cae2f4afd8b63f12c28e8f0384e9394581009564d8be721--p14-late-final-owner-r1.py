# SPDX-License-Identifier: Apache-2.0
"""Root-only four-check owner; requires actual final docs/review/editor evidence.

No Cargo, native execution, editor launch, compilation, package or publication.
This owner records qualification results; it never accepts P14 automatically.
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

BASE = Path(__file__).resolve().parent
WORK = BASE / 'p10-worktree'
CATALOG = BASE / 'p14-late-final-checks-proposed-r1/command-catalog.json'
CATALOG_SHA = '87b3350a839ee043d502413c7b76be6d6312b9a132fdb00f7e9e34c0636ba3a2'
RECORD = BASE / 'p14-late-final-supplement-r1.json'


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def checked_input(path, digest, parse_json=True):
    resolved = Path(path).resolve(strict=True)
    assert resolved.is_relative_to(BASE.resolve()), 'Evidence must stay in the owned artifacts/v190 tree'
    assert re.fullmatch('[0-9a-f]{64}', digest) and sha(resolved) == digest
    return resolved, read(resolved) if parse_json else None


def public_files():
    names = subprocess.check_output(['git', '-c', 'safe.directory=' + WORK.as_posix(),
        'ls-files', '-co', '--exclude-standard', '-z'], cwd=WORK).split(b'\0')
    paths = sorted({name.decode('utf-8') for name in names if name})
    assert len(paths) > 1700
    assert all(not p.startswith(('artifacts/', 'temp/', '.airk/', '.git/')) for p in paths)
    return {name: sha(WORK / name) for name in paths if (WORK / name).is_file()}


def main():
    parser = argparse.ArgumentParser()
    for name in ['final-docs-manifest', 'primary-review', 'editor-evidence', 'final-qualification']:
        parser.add_argument('--' + name, required=True)
        parser.add_argument('--' + name + '-sha256', required=True)
    parser.add_argument('--native-sha256', required=True)
    parser.add_argument('--native-files', required=True, type=int)
    args = parser.parse_args()
    assert re.fullmatch('[0-9a-f]{64}', args.native_sha256) and args.native_files > 0
    expected = {'sha256': args.native_sha256, 'files': args.native_files}
    assert not RECORD.exists(), 'Fresh result path required'
    assert sha(CATALOG) == CATALOG_SHA
    catalog = read(CATALOG)
    docs_path, docs = checked_input(args.final_docs_manifest, args.final_docs_manifest_sha256)
    review_path, review = checked_input(args.primary_review, args.primary_review_sha256)
    editor_path, editor = checked_input(args.editor_evidence, args.editor_evidence_sha256)
    qualification_path, qualification = checked_input(args.final_qualification, args.final_qualification_sha256)
    assert qualification['status'] == 'passed'
    assert qualification.get('native_source', qualification.get('source')) == expected
    rows = docs['files']
    assert len(rows) == len({row['path'] for row in rows}) == 15
    required_docs = {'README.md', 'docs/README.md', 'docs/releases/release-1.8.8.md',
        'docs/releases/release-1.9.0.md', 'docs/research/implementation-v190.md',
        'docs/research/plan-v190.md', 'docs/architecture.md', 'docs/index-splitting.md',
        'packages/vscode/README.md', 'docs/vscode-extension.md', 'docs/mcp.md',
        'docs/unified-mcp.md', 'docs/mcp-developer-guide.md', 'docs/service-retrieval.md',
        'docs/index-self-healing.md'}
    assert {row['path'] for row in rows} == required_docs
    for row in rows:
        path = Path(row['path'])
        assert not path.is_absolute() and '..' not in path.parts
        assert sha(WORK / path) == row['proposed_sha256'], 'Final docs must already be applied'
    assert review['status'] == 'passed'
    assert review['docs_manifest_sha256'] == args.final_docs_manifest_sha256
    assert review['readme_docs_index_release_marketplace_agree'] is True
    assert review['selected_local_scope_and_limits_reviewed'] is True
    assert review['editor_evidence_sha256'] == args.editor_evidence_sha256
    assert review['native_source'] == expected
    assert review['final_qualification_sha256'] == args.final_qualification_sha256
    assert editor['schema'] == 'airk.p14.editor-coverage/1'
    assert editor['status'] == 'passed' and editor['current_native_source'] == expected
    scenarios = editor['scenario_results']
    assert [row['scenario'] for row in scenarios] == ['same', 'different', 'multi', 'usage', 'untrusted', 'legacy']
    editor_pins = {}
    for row in scenarios:
        assert row['status'] == 'passed' and row['evidence']
        if row['scenario'] in ['usage', 'legacy']:
            assert row['retained'] is False and row['native_source'] == expected
            assert re.fullmatch('[0-9a-f]{64}', row['vsix_sha256'])
        else:
            assert row['retained'] is True
            assert row['native_source'] == {'sha256': '5d8912b117b410306ac9a3b44af6d96d70bb018006c2d49a1efa365597fe3e11', 'files': 960}
            assert row['vsix_sha256'] == '49170b51765048e438dfd9516a35e4884d7667efe5ad2652d02303caf22f4c90'
        for proof in row['evidence']:
            proof_path, _ = checked_input(proof['path'], proof['sha256'], parse_json=False)
            editor_pins[str(proof_path)] = proof['sha256']
    current_packages = {row['vsix_sha256'] for row in scenarios if row['scenario'] in ['usage', 'legacy']}
    assert len(current_packages) == 1, 'Fresh usage/legacy must share the actual final native package'
    pins = dict(catalog['adopted_observed_inputs'])
    pins[str(docs_path)] = args.final_docs_manifest_sha256
    pins[str(review_path)] = args.primary_review_sha256
    pins[str(editor_path)] = args.editor_evidence_sha256
    pins[str(qualification_path)] = args.final_qualification_sha256
    pins.update(editor_pins)
    for path, digest in pins.items():
        assert sha(path) == digest, 'Adopted evidence changed'
    assert sha(catalog['wrapper']['path']) == catalog['wrapper']['sha256']
    for path, digest in catalog['late_tooling_inputs'].items():
        assert sha(WORK / path) == digest
    steps = catalog['commands']
    assert [row['name'] for row in steps] == ['python-tests', 'repo-check', 'version-consistency', 'whitespace']
    for step in steps:
        log = Path(step['log'])
        assert log.parent == BASE and not log.exists() and not log.with_suffix('.receipt.json').exists()
    environment = os.environ.copy()
    environment.update(PYTHONUTF8='1', CARGO_NET_OFFLINE='true',
        AIRK_PRIVATE_CENSUS_BASE=str(BASE / 'p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02'))
    for name in ['AIRK_P10_RESOURCE_OUTPUT_DIR', 'AIRK_P11B_RESOURCE_OUTPUT_DIR',
        'AIRK_CATALOG_CANDIDATE_BINARY', 'AIRK_TEST_BINARY']:
        environment.pop(name, None)
    spec = importlib.util.spec_from_file_location('p14_late_source_identity', WORK / 'scripts/build-service-runtime.py')
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    assert builder.source_identity() == expected
    public = public_files()
    record = {'status': 'running', 'native_source': expected, 'historical_adoptions': 'Original catalog receipts retain source5d89; current native qualification is separately bound, never relabeled', 'catalog_sha256': CATALOG_SHA,
        'owner_sha256': sha(__file__), 'bound_inputs': pins, 'public_input_hashes': public,
        'steps': [], 'p14_acceptance': 'Not decided by this owner; primary acceptance remains separate',
        'limits': catalog['final_supplement_record']['boundaries']}
    def save():
        RECORD.write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')
    save()
    try:
        failures = []
        for step in steps:
            assert public_files() == public and builder.source_identity() == expected
            assert all(sha(path) == digest for path, digest in pins.items())
            # UTF-8 applies to the wrapper itself, as well as its inherited children.
            command = [sys.executable, '-X', 'utf8', *step['command'][1:]]
            result = subprocess.run(command, cwd=WORK, env=environment)
            log = Path(step['log'])
            receipt_path = log.with_suffix('.receipt.json')
            receipt = read(receipt_path)
            valid = result.returncode == receipt['exit_code'] == receipt['actual_exit_code'] == 0
            valid = valid and receipt['validation_error'] is None and receipt['source_unchanged'] is True
            valid = valid and receipt['source_before'] == receipt['source_after'] == expected
            valid = valid and sha(log) == receipt['log_sha256']
            observed = {'name': step['name'], 'command': command, 'exit_code': result.returncode,
                'receipt': str(receipt_path), 'receipt_sha256': sha(receipt_path), 'valid': valid}
            if step['name'] == 'python-tests':
                body = log.read_text(encoding='utf-8')
                totals = re.findall(r'^Ran (\d+) tests? in ', body, re.M)
                skips = re.findall(r'^OK(?: \(skipped=(\d+)\))?$', body, re.M)
                cases = catalog['python_validation']['new_cases']
                count = int(totals[-1]) if len(totals) == 1 else None
                skipped = int(skips[0] or '0') if len(skips) == 1 else None
                counts = {name: len(re.findall('^' + re.escape(name) + r' \(', body, re.M)) for name in cases}
                observed.update(actual_total=count, actual_skipped=skipped, new_case_occurrences=counts)
                observed['valid'] = valid and count == 468 and skipped == 9 and set(counts.values()) == {1}
            record['steps'].append(observed)
            if not observed['valid']:
                failures.append(step['name'])
            assert public_files() == public and builder.source_identity() == expected
            save()
        record['failed_checks'] = failures
        record['status'] = 'passed' if not failures else 'failed'
        save()
        return 0 if not failures else 1
    except BaseException as error:
        record['status'] = 'failed'
        record['failure'] = repr(error)
        save()
        raise


if __name__ == '__main__':
    sys.exit(main())
