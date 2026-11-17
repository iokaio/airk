# SPDX-License-Identifier: Apache-2.0
import json, os, shutil, subprocess, sys, time, xml.etree.ElementTree as ET
from pathlib import Path
base = Path(__file__).resolve().parent
root = base.parents[1]
out = base / 'windows-r1'
assert Path(os.environ['AIRK_PRIVATE_CENSUS_BASE']).resolve() == (root / 'artifacts/v190/p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02').resolve()
for key in ['AIRK_TEST_BINARY', 'AIRK_CATALOG_CANDIDATE_BINARY']:
    assert key not in os.environ
expected = json.loads((out / 'full-native-r1.summary.json').read_text(encoding='utf-8'))['failed']
selection = 'binary(native_owner_recovery) | test(enabled_background_watch_retains_idle_service_until_explicitly_disabled) | test(native_original_data_root_handle_blocks_owned_path_replacement) | test(untouched_windows_bind_and_retained_producer_layout_qualifies_native_boundary) | test(postclear_replay_preserves_b_progress_and_applied_receipt_survives_a_physical_absence_cleanup)'
common = ['--workspace', '--all-features', '--locked', '--offline', '--timings', '--ignore-default-filter', '-E', selection]
inventory_path = out / 'corrected-cases-r2.inventory.json'
with inventory_path.open('x', encoding='utf-8') as stream:
    code = subprocess.run(['cargo', 'nextest', 'list', *common, '--message-format', 'json'], cwd=root, stdout=stream).returncode
if code: sys.exit(code)
inventory = json.loads(inventory_path.read_text(encoding='utf-8'))
names = [name for suite in inventory['rust-suites'].values() for name, case in suite['testcases'].items() if case['filter-match']['status'] == 'matches' and not case['ignored']]
assert set(expected) <= set(names), (expected, names)
print(json.dumps({'selected': names}), flush=True)
start = time.monotonic()
code = subprocess.run(['cargo', 'nextest', 'run', *common, '--profile', 'ci', '--no-fail-fast', '--retries', '0', '--test-threads', '4'], cwd=root).returncode
saved = out / 'corrected-cases-r2.junit.xml'
assert not saved.exists()
shutil.copyfile(root / 'target/nextest/ci/junit.xml', saved)
cases = list(ET.parse(saved).iter('testcase'))
failed = [case.get('name') for case in cases if case.find('failure') is not None or case.find('error') is not None]
passed = [case.get('name') for case in cases if case.find('failure') is None and case.find('error') is None and case.find('skipped') is None]
summary = {'exit_code': code, 'selected': len(names), 'passed': len(passed), 'failed': failed, 'seconds': time.monotonic() - start}
with (out / 'corrected-cases-r2.summary.json').open('x', encoding='utf-8') as stream: json.dump(summary, stream, indent=2)
print(json.dumps(summary), flush=True)
if code: sys.exit(code)
assert not failed and len(passed) == len(names)

