# SPDX-License-Identifier: Apache-2.0
"""Summarize retained Neo evidence without relabeling original failed steps."""
from pathlib import Path
import json,re,sys
root=Path(sys.argv[1]);steps=dict(line.split() for line in (root/'results.txt').read_text(encoding='utf-8').splitlines() if line.strip())
required=['source-before','toolchain','cargo-inputs','advisory-cache','dependency-policy','npm-inputs','extension-tests','model-inputs','runtime-inputs','native-build','workspace-build','format','clippy','workspace-tests','doctests','native-smoke','native-clients','native-installer','actual-upgrade','vector-components','licenses','python-tests','repository','whitespace','native-package','extension-package','source-after']
missing=[name for name in required if name not in steps];failed={name:code for name,code in steps.items() if code!='0'}
correction=json.loads((root/'extension-correction-result.json').read_bytes());assert correction['passed'] and correction['source_unchanged_except_declared_test']
native=json.loads((root/'native-correction-result.json').read_bytes());assert native['passed'] and native['source_unchanged_except_declared_corrections']
workspace=(root/'workspace-tests.log').read_text(encoding='utf-8')
started=re.search(r'Starting (\d+) tests across (\d+) binaries \((\d+) tests skipped\)',workspace)
match=re.search(r'Summary \[[^\]]+\] (\d+) tests run: ([^\r\n]+)',workspace)
assert started and int(started[2])>=60 and match and match[1]==started[1],workspace[-3000:]
for name in ['authority_refusal_remains_permanent_across_both_connection_routes','completed_genesis_reads_live_owner_transaction_but_refuses_orphan_overlay','failed_initial_probe_preserves_recovery_without_a_public_selection_or_source_changes']:
 assert any('PASS' in line and name in line for line in workspace.splitlines()),name
counts={kind:int(value) for value,kind in re.findall(r'(\d+)\s+(passed|failed|skipped|flaky)\b',match[2])};assert counts.get('passed',0)+counts.get('failed',0)==int(match[1]),counts
expected_failures={'project_worker::admin::tests::administrative_native_reads_retain_actual_complete_handler_pages','configured_query_capacity_updates_live_worker_and_reads_through_matching_channels'}
observed_failures={line.split()[-1] for line in workspace.splitlines() if re.match(r'\s*FAIL\s+\[',line)}
assert observed_failures==expected_failures and counts.get('failed')==len(expected_failures),(observed_failures,counts)
library=(root/'native-correction-cli-library.log').read_text(encoding='utf-8')
library_counts=re.search(r'test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;',library)
assert library_counts and int(library_counts[1])==4 and 'administrative_native_reads_retain_actual_complete_handler_pages ... ok' in library
capacity=(root/'native-correction-capacity.log').read_text(encoding='utf-8');assert 'test result: ok. 1 passed; 0 failed;' in capacity
repository=(root/'repository.log').read_text(encoding='utf-8');assert 'Broken link in docs/README.md: research/new-issues.md' in repository
assert native['steps']['repository']==0
vectors=(root/'native-correction-vector-components.log').read_text(encoding='utf-8');assert 'test result: ok. 3 passed; 0 failed;' in vectors
python_correction=json.loads((root/'python-complete-correction-r2-result.json').read_bytes());assert python_correction['passed'] and python_correction['source_unchanged_except_declared_corrections']
python=(root/'python-complete-correction-r2-tests.log').read_text(encoding='utf-8');python_total=re.search(r'Ran (\d+) tests in ',python);python_skips=re.search(r'OK \(skipped=(\d+)\)',python);assert python_total and python_skips and int(python_total[1])==411 and int(python_skips[1])==6
for name in ['test_actual_bundles_stage_with_the_pinned_preact_notice','test_actual_compiled_runtime_imports_are_all_staged','test_packaged_parser_closure_and_license']:
 assert any(line.startswith(name+' ') and line.endswith(' ... ok') for line in python.splitlines()),name
summary={'schema':'airk.full-neo-local-qualification/1','original_full_run_passed':not missing and not failed,'original_failed_steps':failed,'missing_steps':missing,'current_source_covered_by_full_run_and_declared_corrections':not missing and failed=={'extension-tests':'1','workspace-tests':'100','vector-components':'101','python-tests':'1','repository':'1'} and correction['passed'] and native['passed'],'catalog_tests':int(started[1]),'catalog_binaries':int(started[2]),'catalog_skips':int(started[3]),'workspace_counts':counts,'workspace_tests':int(match[1]),'original_native_failures':sorted(observed_failures),'corrected_administrative_library_passes':int(library_counts[1]),'corrected_administrative_library_ignored':int(library_counts[2]),'python_tests':int(python_total[1]),'python_skips':int(python_skips[1]),'legacy_standalone_native_fixtures_unavailable':3,'native_correction':native,'extension_correction':correction,'release_signing_qualified':False,'installed_mac_editor_qualified':False,'scope':'Native ARM64 full source campaign, declared fixture/documentation/qualification-input corrections and qualified-shell capacity rerun; no P7 or publication'}
(root/'qualification-summary.json').write_bytes(json.dumps(summary,indent=2).encode());print(json.dumps(summary))
raise SystemExit(0 if summary['current_source_covered_by_full_run_and_declared_corrections'] else 1)
