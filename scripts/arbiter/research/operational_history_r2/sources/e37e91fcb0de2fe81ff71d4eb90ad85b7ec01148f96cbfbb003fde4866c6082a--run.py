# SPDX-License-Identifier: Apache-2.0
"""Local qualification owner; retain every command, result and Cargo timing."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import xml.etree.ElementTree as ET

sys.stdout.reconfigure(encoding='utf-8')

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
TARGET = ROOT / 'artifacts/v190/p10-target'
DEBUG_RUNTIME = OUT / 'runtime-debug-final'
ENV = os.environ.copy()
ENV['PATH'] = str(Path('C:/Users/tdotn/.cargo/bin')) + os.pathsep + str(ROOT / 'artifacts/tools/nextest') + os.pathsep + ENV['PATH']
ENV['CARGO_HOME'] = 'C:/Users/tdotn/.cargo'
ENV['RUSTUP_HOME'] = 'C:/Users/tdotn/.rustup'
ENV['CARGO_TARGET_DIR'] = str(TARGET)
ENV['CARGO_INCREMENTAL'] = '1'
ENV['AIRK_PRIVATE_CENSUS_BASE'] = str(ROOT / 'artifacts/v190/p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02')
ENV['AIRK_BUNDLED_MODEL_DIR'] = str(ROOT / 'target/local-vector-model')
ENV['AIRK_BUNDLED_RUNTIME_DIR'] = str(ROOT / 'target/local-onnx-runtime')
ENV['AIRK_TEST_MODEL_DIR'] = str(ROOT / 'target/local-vector-model/minilm-l6-v2-u8-v1')
ENV['AIRK_TEST_BINARY'] = str(DEBUG_RUNTIME / 'airk.exe')
ENV['AIRK_CATALOG_CANDIDATE_BINARY'] = ENV['AIRK_TEST_BINARY']
for name in ('AIRK_P10_RESOURCE_OUTPUT_DIR', 'AIRK_P11B_RESOURCE_OUTPUT_DIR'):
    ENV.pop(name, None)

FOCUSED = ' | '.join([
    (ROOT / 'artifacts/qa-1.8.9/transport-unit-filter.txt').read_text(encoding='utf-8').strip(),
    (ROOT / 'artifacts/qa-1.8.9/transport-process-filter.txt').read_text(encoding='utf-8').strip(),
    json.loads((ROOT / 'artifacts/qa-1.8.9/verification-retry-selection.json').read_text(encoding='utf-8'))['filter'],
    'package(airk-mcp) | (package(airk-core) & test(verification::inventory)) | test(installed_) | test(open_fact_) | test(native_relationships_) | test(exposed_code_) | test(qualified_filter_) | test(empty_fact_pages_) | test(focused_literals_) | test(context_radius_) | test(annotated_evidence_) | test(native_public_and_workbench_) | test(native_help_cli_) | test(terse_capabilities_) | test(saved_source_verification_preserves_complete_results_) | test(native_original_data_root_handle_) | binary(native_owner_recovery) | test(untouched_windows_bind_) | binary(catalog_upgrade)'
])

COMMANDS = {
    'package-vsix-r3': [sys.executable, 'scripts/package-extension.py', '--target', 'x86_64-pc-windows-msvc', '--native-dir', str(OUT / 'native/publish'), '--output', str(OUT / 'vscode-r3')],
    'installed-editor-different-r5': ['node', 'scripts/check-native-editor.cjs', 'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe', 'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/c4453f8ea4/resources/app/out/cli.js', str(OUT / 'vscode-r3/publish/airk-1.8.9-win32-x64.vsix')],
    'installed-editor-remaining-r6': ['node', 'scripts/check-native-editor.cjs', 'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe', 'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/c4453f8ea4/resources/app/out/cli.js', str(OUT / 'vscode-r3/publish/airk-1.8.9-win32-x64.vsix')],
    'installed-editor-different-r4': ['node', 'scripts/check-native-editor.cjs', 'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe', 'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/c4453f8ea4/resources/app/out/cli.js', str(OUT / 'vscode-r2/publish/airk-1.8.9-win32-x64.vsix')],
    'ownership-tests': ['node', '--test', 'packages/vscode/out/test/explorerShell.test.js', 'packages/vscode/out/test/indexingCatalog.test.js', 'packages/vscode/out/test/trust.test.js', 'packages/vscode/out/test/explorerUpgradeUi.test.js'],
    'package-vsix-r2': [sys.executable, 'scripts/package-extension.py', '--target', 'x86_64-pc-windows-msvc', '--native-dir', str(OUT / 'native/publish'), '--output', str(OUT / 'vscode-r2')],
    'installed-editor-r3': ['node', 'scripts/check-native-editor.cjs', 'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe', 'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/c4453f8ea4/resources/app/out/cli.js', str(OUT / 'vscode-r2/publish/airk-1.8.9-win32-x64.vsix')],
    'retry-correctives': ['cargo', 'nextest', 'run', '--locked', '--offline', '--timings', '-p', 'airk-cli', '--test', 'native_read_retry', '--all-features', '--profile', 'ci', '--no-fail-fast', '--retries', '0', '--test-threads', '2'],
    'provision-model': [sys.executable, 'scripts/provision-local-model.py', '--destination', ENV['AIRK_BUNDLED_MODEL_DIR'], '--download'],
    'provision-onnx': [sys.executable, 'scripts/provision-onnx-runtime.py', '--download'],
    'scoped-clippy': [sys.executable, 'scripts/dev-check.py', '-p', 'airk-core', '-p', 'airk-cli', '-p', 'airk-kernel', '-p', 'airk-mcp', '-p', 'airk-store', '-p', 'airk-retrieval', '--all-features', '--target-dir', str(TARGET), '--incremental', 'on'],
    'scoped-cli': [sys.executable, 'scripts/dev-check.py', '-p', 'airk-cli', '--all-features', '--target-dir', str(TARGET), '--incremental', 'on'],
    'workspace-clippy': [sys.executable, 'scripts/dev-check.py', '--workspace', '--all-features', '--target-dir', str(TARGET), '--incremental', 'on'],
    'lightweight': ['cargo', 'check', '-p', 'airk-cli', '--no-default-features', '--all-targets', '--locked', '--timings'],
    'runtime-debug': [sys.executable, 'scripts/build-service-runtime.py', '--destination', str(DEBUG_RUNTIME), '--profile', 'debug'],
    'preflight': ['cargo', 'nextest', 'run', '--locked', '--offline', '--timings', '-p', 'airk-cli', '--test', 'catalog_upgrade', '--all-features', '--profile', 'ci', '--no-fail-fast', '--retries', '0', '--test-threads', '2'],
    'workspace-build': ['cargo', 'build', '--workspace', '--locked', '--timings'],
    'focused-list': ['cargo', 'nextest', 'list', '--workspace', '--all-features', '--locked', '--offline', '--timings', '--profile', 'ci', '-E', FOCUSED],
    'focused-tests': ['cargo', 'nextest', 'run', '--workspace', '--all-features', '--locked', '--offline', '--timings', '--profile', 'ci', '--no-fail-fast', '--retries', '0', '--test-threads', '2', '-E', FOCUSED],
    'fixture-tests': ['cargo', 'nextest', 'run', '--workspace', '--all-features', '--locked', '--offline', '--timings', '--profile', 'ci', '--no-fail-fast', '--retries', '0', '--test-threads', '2', '-E', 'test(exposed_code_facts_preserve_retained_producer_kind_and_language_labels) | test(default_terse_stdio_supports_full_and_requested_status_without_changing_revision)'],
    'cli-reference-generate': ['cargo', 'nextest', 'run', '--workspace', '--all-features', '--locked', '--offline', '--timings', '--profile', 'ci', '--no-fail-fast', '--retries', '0', '--test-threads', '2', '-E', 'test(cli_reference_documents_every_public_command)'],
    'schema-correctives': ['cargo', 'nextest', 'run', '--workspace', '--all-features', '--locked', '--offline', '--timings', '--profile', 'ci', '--no-fail-fast', '--retries', '0', '--test-threads', '2', '-E', 'test(durable_key_reads_match_cli_and_mcp_and_reject_filter_combinations) | test(setup_outputs_client_config_and_mcp_exposes_specialized_tools) | test(supervised_jobs_and_direct_queries_preserve_grants_and_stop_on_service_loss) | test(cli_reference_tests::) | test(tools_help_works_without_a_workspace_and_exposes_live_schemas)'],
    'workspace-tests': ['cargo', 'nextest', 'run', '--workspace', '--all-features', '--locked', '--offline', '--timings', '--profile', 'ci', '--ignore-default-filter', '--no-fail-fast', '--retries', '0', '--test-threads', '2'],
    'doctests': ['cargo', 'test', '--doc', '--workspace', '--locked', '--timings', '--', '--test-threads=1'],
    'service-clients': [sys.executable, 'scripts/check-service-clients.py', '--runtime-dir', str(DEBUG_RUNTIME), '--files', '16', '--model'],
    'previous-upgrade': [sys.executable, 'scripts/check-runtime-upgrade.py', '--runtime', str(DEBUG_RUNTIME), '--previous-runtime', str(ROOT / 'artifacts/upgrade-1.8.9-windows/previous-1.8.8'), '--fixture-setup', 'reviewed', '--output', str(OUT / 'previous-upgrade-receipt.json')],
    'extension-compile': ['npm.cmd', 'run', 'compile', '--prefix', 'packages/vscode'],
    'extension-tests': ['npm.cmd', 'test', '--prefix', 'packages/vscode'],
    'onboarding-tests': ['node', '--test', 'packages/vscode/out/test/serviceOnboarding.test.js'],
    'python-tests': [sys.executable, '-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v'],
    'version-check': [sys.executable, 'scripts/release-version.py', 'check', '--expected', '1.8.9'],
    'repo-check': ['pwsh', '-NoProfile', '-File', 'scripts/check-repo.ps1'],
    'notices': [sys.executable, 'scripts/license-notices.py', '--output', str(OUT / 'notices'), '--release'],
    'dependency-audit': [sys.executable, 'scripts/dependency-audit.py'],
    'runtime-release': [sys.executable, 'scripts/build-service-runtime.py', '--destination', str(OUT / 'runtime-release'), '--profile', 'release'],
    'release-clients': [sys.executable, 'scripts/check-service-clients.py', '--runtime-dir', str(OUT / 'runtime-release'), '--files', '16', '--model'],
    'ci-local': ['pwsh', '-NoProfile', '-File', 'scripts/ci-local.ps1', '-SkipBuild', '-RuntimeDir', str(OUT / 'runtime-release')],
    'package-native': [sys.executable, 'scripts/package-native.py', '--target', 'x86_64-pc-windows-msvc', '--runtime-dir', str(OUT / 'runtime-release'), '--output', str(OUT / 'native')],
    'package-vsix': [sys.executable, 'scripts/package-extension.py', '--target', 'x86_64-pc-windows-msvc', '--native-dir', str(OUT / 'native/publish'), '--output', str(OUT / 'vscode')],
    'installed-editor': ['node', 'scripts/check-native-editor.cjs', 'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe', 'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/c4453f8ea4/resources/app/out/cli.js', str(OUT / 'vscode/publish/airk-1.8.9-win32-x64.vsix')],
    'whitespace': ['git', '-c', f'safe.directory={ROOT.as_posix()}', 'diff', '--check'],
}

for stage in sys.argv[1:]:
    cmd = COMMANDS.get(stage, COMMANDS.get(stage.rsplit('-r', 1)[0]))
    if cmd is None:
        raise SystemExit(stage)
    if cmd[0] == 'cargo':
        cmd = ['C:/Users/tdotn/.cargo/bin/cargo.exe', *cmd[1:]]
    candidate = DEBUG_RUNTIME / 'airk.exe'
    if candidate.is_file():
        ENV['AIRK_TEST_BINARY'] = str(candidate)
        ENV['AIRK_CATALOG_CANDIDATE_BINARY'] = str(candidate)
    else:
        ENV.pop('AIRK_TEST_BINARY', None)
        ENV.pop('AIRK_CATALOG_CANDIDATE_BINARY', None)
    receipt_path = OUT / (stage + '.json')
    if receipt_path.exists():
        raise SystemExit(f'Result already exists: {stage}; retain it and use an explicitly named new attempt')
    execution_env = ENV.copy()
    if stage.startswith('runtime-release'):
        execution_env.pop('CARGO_INCREMENTAL', None)
    if stage == 'cli-reference-generate':
        execution_env['AIRK_WRITE_CLI_REFERENCE'] = '1'
    if stage.startswith('installed-editor'):
        execution_env['AIRK_EDITOR_SCENARIOS'] = 'same,different,multi,usage,untrusted'
    if stage.startswith('installed-editor-different'):
        execution_env['AIRK_EDITOR_SCENARIOS'] = 'different'
    if stage.startswith('installed-editor-remaining'):
        execution_env['AIRK_EDITOR_SCENARIOS'] = 'same,multi,usage,untrusted'
    incremental = execution_env.get('CARGO_INCREMENTAL', 'unset; release profile default')
    print(json.dumps({'stage': stage, 'command': cmd, 'target': str(TARGET), 'incremental': incremental}), flush=True)
    spec = importlib.util.spec_from_file_location('qa_builder', ROOT / 'scripts/build-service-runtime.py')
    builder = importlib.util.module_from_spec(spec); spec.loader.exec_module(builder)
    before = builder.source_identity()
    reuse = OUT / 'runtime-reuse-receipt.json'
    runtime_build = DEBUG_RUNTIME / 'airk-build.json'
    started = time.monotonic()
    log = OUT / (stage + '.log')
    with log.open('w', encoding='utf-8') as stream:
        try:
            result = subprocess.run(cmd, cwd=ROOT, env=execution_env, stdout=stream, stderr=subprocess.STDOUT)
            code = result.returncode
        except Exception as exc:
            stream.write(repr(exc))
            code = 127
    timings = []
    saved = OUT / 'cargo-timings' / stage
    saved.mkdir(parents=True, exist_ok=True)
    for report in (TARGET / 'cargo-timings').glob('*.html'):
        if report.stat().st_mtime >= time.time() - (time.monotonic() - started) - 2:
            shutil.copy2(report, saved / report.name)
            timings.append(str(saved / report.name))
    receipt = {'stage': stage, 'command': cmd, 'cwd': str(ROOT), 'target': str(TARGET), 'incremental': incremental,
               'native_source_before': before, 'native_source_after': builder.source_identity(),
               'private_fixture_base': ENV['AIRK_PRIVATE_CENSUS_BASE'],
               'seconds': time.monotonic() - started, 'exit_code': code, 'log': str(log),
               'log_sha256': hashlib.sha256(log.read_bytes()).hexdigest(), 'timings': timings}
    if stage.startswith('installed-editor'):
        receipt['selected_editor_scenarios'] = execution_env['AIRK_EDITOR_SCENARIOS'].split(',')
        receipt['historical_legacy_editor_scenario'] = 'not selected: requires a separate qualified legacy-editor fixture; actual original 1.8.8 native upgrade is recorded independently'
    if runtime_build.is_file():
        receipt['debug_runtime_compiled_source'] = json.loads(runtime_build.read_text(encoding='utf-8'))['source']
        if reuse.is_file():
            receipt['debug_runtime_reuse_evidence'] = str(reuse)
    if stage.startswith(('focused-tests', 'workspace-tests', 'fixture-tests', 'schema-correctives', 'cli-reference-generate')):
        junit = ROOT / 'target/nextest/ci/junit.xml'
        if junit.is_file():
            shutil.copy2(junit, OUT / (stage + '.junit.xml'))
            cases = list(ET.parse(junit).iter('testcase'))
            receipt['test_counts'] = {'total': len(cases), **{tag: sum(case.find(tag) is not None for case in cases) for tag in ('failure', 'error', 'skipped')}}
            receipt['failures'] = [case.attrib for case in cases if case.find('failure') is not None or case.find('error') is not None]
            receipt['campaigns'] = [{**case.attrib, 'passed': all(case.find(tag) is None for tag in ('failure', 'error', 'skipped'))} for case in cases if case.get('name', '').rsplit('::', 1)[-1] in ('compact_measurements_compare_small_reads_from_large_catalog', 'two_hundred_current_edits_reach_object_and_database_plateau', 'measure_small_and_larger_sparse_catalog_edits', 'manual_backlog_larger_than_one_page_advances_without_unprotected_deletion')]
    receipt_path.write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(receipt), flush=True)
    if code:
        print(log.read_text(encoding='utf-8', errors='replace')[-12000:], flush=True)
        raise SystemExit(code)
