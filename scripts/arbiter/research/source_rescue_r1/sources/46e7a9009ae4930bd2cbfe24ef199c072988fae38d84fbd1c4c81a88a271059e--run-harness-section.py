# SPDX-License-Identifier: Apache-2.0
"""One frozen Python harness section, retaining all diagnostics and identities."""
from datetime import datetime, timezone
import hashlib, json, os, platform, subprocess, sys, time, unittest
from pathlib import Path

ROOT = Path.cwd().resolve()
OUT = Path(sys.argv[1]).resolve()
if ROOT not in OUT.parents or OUT.exists():
    raise RuntimeError('Fresh workspace receipt output required')
OUT.mkdir()
MODULES = (
    'test_http_fixture', 'test_service_clients', 'test_benchmark_service_http',
    'test_real_corpora_benchmark', 'test_mcp_investigation_benchmark',
    'test_retrieval_quality', 'test_usage_http_transport', 'test_runtime_upgrade',
    'test_service_install', 'test_container_check', 'test_http_port_relocation',
    'test_issuer_rotation_campaign', 'test_local_test_setup', 'test_u110_harnesses',
    'test_arbiter_lifecycle', 'test_receipt_runner',
)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def inputs():
    return {path.relative_to(ROOT).as_posix(): digest(path)
            for path in sorted((ROOT / 'scripts').rglob('*.py'))
            if '__pycache__' not in path.parts}

def utc():
    return datetime.now(timezone.utc).isoformat()

start = utc()
clock = time.monotonic()
before = inputs()
report = {
    'schema': 1, 'started_utc': start, 'source_commit': subprocess.check_output(
        ['git', 'rev-parse', 'HEAD'], text=True).strip(),
    'command': [sys.executable, '-B', str(Path(__file__).resolve()), str(OUT)],
    'driver_sha256': digest(Path(__file__).resolve()),
    'scope': 'Grouped changed Python harnesses and shared HTTP/client/lifecycle/receipt consumers; no native or Cargo qualification',
    'test_modules': MODULES, 'source_inputs_before': before,
    'environment': {name: os.environ.get(name) for name in ('PYTHONDONTWRITEBYTECODE', 'TEMP', 'TMP', 'TMPDIR', 'AIRK_PRIVATE_CENSUS_BASE')},
    'host': {'platform': platform.platform(), 'machine': platform.machine(),
             'logical_cpus': os.cpu_count(), 'python': sys.version},
    'identity': {'account': 'TYAI\\tdotn', 'sid': 'S-1-5-21-3678370870-3858642417-826968105-1002',
                 'evidence': 'Verified escalated shell effective SID matches active quser and Explorer owner in session1 before this section; children inherit that identity'},
    'selected': None, 'passed': None, 'failed': None, 'skipped': None,
    'native_airk_executed': False, 'cargo_executed': False,
}
(OUT / 'receipt.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
sys.path.insert(0, str(ROOT / 'scripts' / 'tests'))
try:
    with (OUT / 'tests.log').open('x', encoding='utf-8', buffering=1) as log:
        import contextlib
        with contextlib.redirect_stdout(log), contextlib.redirect_stderr(log):
            suite = unittest.TestSuite(unittest.defaultTestLoader.loadTestsFromName(name) for name in MODULES)
            selected = suite.countTestCases()
            if not selected:
                raise RuntimeError('Zero selected tests is a failure')
            report['selected'] = selected
            result = unittest.TextTestRunner(stream=log, verbosity=2).run(suite)
        report.update(passed=result.testsRun - len(result.failures) - len(result.errors) - len(result.skipped),
            failed=len(result.failures), errors=len(result.errors), skipped=len(result.skipped),
            skipped_tests=[{'test': str(case), 'reason': reason} for case, reason in result.skipped],
            observed_tests=result.testsRun, successful=result.wasSuccessful(),
            unexpected_successes=[str(case) for case in result.unexpectedSuccesses])
except BaseException as error:
    report.update(successful=False, runner_error={'type': type(error).__name__, 'message': str(error)})
finally:
    report['ended_utc'] = utc()
    report['elapsed_seconds'] = time.monotonic() - clock
    report['source_inputs_after'] = inputs()
    report['inputs_unchanged'] = before == report['source_inputs_after']
    report['outputs'] = {'tests.log': digest(OUT / 'tests.log')} if (OUT / 'tests.log').exists() else {}
    report['cleanup'] = 'Each owned fixture uses its tested settlement policy; failed fixture retention remains in tests.log; no global process or native settlement claim'
    report['passed_section'] = bool(report.get('successful') and report['inputs_unchanged'] and report.get('selected', 0) > 0)
    (OUT / 'receipt.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
print(json.dumps({key: report.get(key) for key in ('selected','observed_tests','passed','failed','errors','skipped','elapsed_seconds','inputs_unchanged','passed_section')}, indent=2))
print('Receipt: ' + str(OUT / 'receipt.json'))
print('Full diagnostics: ' + str(OUT / 'tests.log'))
if not report['passed_section']:
    print((OUT / 'tests.log').read_text(encoding='utf-8')[-16000:])
raise SystemExit(0 if report['passed_section'] else 1)
