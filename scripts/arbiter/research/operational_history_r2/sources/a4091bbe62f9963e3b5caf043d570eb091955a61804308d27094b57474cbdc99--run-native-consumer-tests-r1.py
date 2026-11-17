# SPDX-License-Identifier: Apache-2.0
"""Focused native deadline/allowance regressions; no model campaign selected."""
import json
import os
from pathlib import Path
import subprocess
import sys
root = Path.cwd()
env = {key: value for key, value in os.environ.items() if not key.upper().startswith('GIT_')}
env.update(json.loads((root / 'artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment'])
env.update(CARGO_BUILD_JOBS='8', CARGO_INCREMENTAL='1', CARGO_NET_OFFLINE='true',
           AIRK_BUNDLED_MODEL_DIR=str(root / 'artifacts/advanced-split/bundled-model-v2'))
selection = ' | '.join([
    '(package(airk-host) & test(combined_context_tests))',
    '(package(airk-supervisor) & test(combined_context_partition_is_refused_before_any_backend_admission))',
    'package(airk-ipc)',
    '(package(airkctl) & (test(read_context::tests) | test(a_stale_window_read_reselects_once_and_retries_once)))',
    '(package(airk-mcp) & (test(parent_context_tests) | test(admission_tests) | test(optional_callback_failure_keeps_canonical_evidence_but_typed_scope_refusal_stops_delivery)))',
    '(package(airk-service) & test(tools_call::tests))',
    '(package(airk-https) & (test(invalid_original_read_parent_observes_no_scope_or_backend) | test(native_admin_cancel_and_close_keep_capacity_until_real_producers_finish)))',
])
command = [sys.executable, 'scripts/dev/receipt_runner.py', '--groups', 'native,harness,fixtures,frontend',
           '--out', 'artifacts/advanced-split/receipts/native-context-r3', '--target-dir',
           'artifacts/v190/p10-target', 'focused-consumer-tests', 'C:/Users/tdotn/.cargo/bin/cargo.exe',
           'nextest', 'run', '--profile', 'ci', '--lib', '--all-features', '--locked', '--timings']
for package in ['airk-host', 'airk-supervisor', 'airk-ipc', 'airkctl', 'airk-mcp', 'airk-service', 'airk-https']:
    command.extend(['-p', package])
command.extend(['-E', selection])
raise SystemExit(subprocess.run(command, cwd=root, env=env).returncode)
