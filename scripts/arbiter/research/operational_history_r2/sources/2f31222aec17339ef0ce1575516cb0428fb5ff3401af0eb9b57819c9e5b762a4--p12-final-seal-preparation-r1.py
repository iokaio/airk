# SPDX-License-Identifier: Apache-2.0
"""Seal reviewed P12 final preparation after exact-source functional success.

Read-only source/receipt checks and ignored plan output only. No Cargo, SSH,
runtime producer, tests or qualification queue is launched by this script.
"""
import argparse
import ast
import hashlib
import importlib.util
import json
import os
import re
from pathlib import Path
import xml.etree.ElementTree as ET

BASE = Path(__file__).resolve().parent


def read(path):
    return json.loads(path.read_text(encoding='utf-8'))


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    with path.open('x', encoding='utf-8') as output:
        output.write(json.dumps(value, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-sha256', required=True)
    parser.add_argument('--source-files', required=True, type=int)
    parser.add_argument('--focused-result', required=True, type=Path)
    parser.add_argument('--clippy-receipt', required=True, type=Path)
    args = parser.parse_args()
    assert re.fullmatch(r'[0-9a-f]{64}', args.source_sha256)
    assert args.source_files >= 960
    proposal = BASE / 'p12-final-qualification-command-map-r1.proposed.json'
    plan = read(proposal)
    expected = {'sha256': args.source_sha256, 'files': args.source_files}
    plan['source_expected'] = expected
    plan['release_adoption']['expected_source'] = expected
    plan['adopted_focus']['source'] = expected
    plan['adopted_focus']['qualification'] = str(args.focused_result.resolve(strict=True))
    plan['adopted_current_fmt_clippy'] = str(args.clippy_receipt.resolve(strict=True))
    runner = BASE / 'p12-final-queue-r1.py'
    ast.parse(runner.read_text(encoding='utf-8'))
    focus_path = Path(plan['adopted_focus']['qualification'])
    focus = read(focus_path)
    assert focus['status'] == 'passed' and focus['source_expected'] == expected
    assert len(focus['required_new_tests']) == 18
    tests = next(row for row in focus['steps'] if row['name'] == 'tests')
    assert tests['cases'] == 39 and tests['candidate_environment_unset']
    junit = focus_path.parent / 'junit.xml'
    assert sha(junit) == tests['junit_sha256']
    cases = ET.parse(junit).getroot().findall('.//testcase')
    assert len(cases) == 39 and not any(
        case.find(kind) is not None for case in cases
        for kind in ['failure', 'error', 'skipped'])
    for name in focus['required_new_tests']:
        assert sum(case.get('name', '').rsplit('::', 1)[-1] == name for case in cases) == 1
    for row in focus['steps']:
        receipt = Path(row['receipt'])
        observed = read(receipt)
        assert observed['exit_code'] == observed['actual_exit_code'] == 0
        assert observed['source_before'] == observed['source_after'] == expected
        assert observed['source_unchanged']
        log = receipt.with_name(receipt.name.replace('.receipt.json', '.log'))
        assert sha(log) == observed['log_sha256']
        plan['pinned_inputs'][str(receipt)] = sha(receipt)
        plan['pinned_inputs'][str(log)] = sha(log)
    for path in [focus_path, junit, Path(tests['inventory'])]:
        plan['pinned_inputs'][str(path)] = sha(path)
    ui_path = Path(plan['separate_ui']['receipt'])
    ui = read(ui_path)
    assert ui['exit_code'] == ui['failed'] == ui['cancelled'] == 0
    assert (ui['tests'], ui['passed'], ui['skipped']) == (795, 776, 19)
    assert sha(Path(plan['separate_ui']['log'])) == ui['log_sha256']
    clippy = read(Path(plan['adopted_current_fmt_clippy']))
    assert clippy['exit_code'] == clippy['actual_exit_code'] == 0
    assert clippy['source_before'] == clippy['source_after'] == expected
    assert clippy['source_unchanged']
    clippy_path = Path(plan['adopted_current_fmt_clippy'])
    clippy_log = clippy_path.with_name(clippy_path.name.replace('.receipt.json', '.log'))
    assert sha(clippy_log) == clippy['log_sha256']
    plan['pinned_inputs'][str(clippy_path)] = sha(clippy_path)
    plan['pinned_inputs'][str(clippy_log)] = sha(clippy_log)
    for path, digest in plan['pinned_inputs'].items():
        assert sha(Path(path)) == digest, 'Pinned input drift: ' + path
    # Import only the existing source fingerprint function. The queue main is
    # guarded; this never builds or spawns another qualification owner.
    spec = importlib.util.spec_from_file_location('p12_fingerprint', BASE / 'p12-focused-queue-r2.py')
    helper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    env = dict(os.environ, GIT_CONFIG_GLOBAL=str(BASE / 'p10-owned-ignore-controls/global.gitconfig'))
    assert helper.native_identity(env) == expected
    plan['status'] = 'prepared_not_launched'
    final_map = BASE / 'p12-final-qualification-command-map-r1.json'
    save(final_map, plan)
    save(BASE / 'p12-final-prepared-r1.json', {
        'ready': True, 'prepared_only': True, 'launched': False,
        'source': expected, 'steps': len(plan['commands']),
        'map_sha256': sha(final_map), 'runner_sha256': sha(runner),
        'proposal_sha256': sha(proposal), 'functional_cases': 39,
        'required_new_tests': 18, 'ui': {'tests': 795, 'passed': 776, 'skipped': 19},
        'release': 'Future sole parent-owned producer; this preparation never launches it.'})
    print(json.dumps({'prepared_only': True, 'source': expected, 'steps': len(plan['commands'])}))


if __name__ == '__main__':
    main()
