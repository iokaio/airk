# SPDX-License-Identifier: Apache-2.0
"""Private P6e phase comparison; reuse the frozen P5 gate definitions unchanged."""
import importlib.util
import json
from pathlib import Path

base = Path('artifacts/v190')
spec = importlib.util.spec_from_file_location('previous_gates', base / 'p5-evaluate-gates.py')
previous = importlib.util.module_from_spec(spec)
spec.loader.exec_module(previous)

off, off_cohorts, off_counts = previous.measurement('p6f-final-off-mcp', 'off')
report, report_cohorts, report_counts = previous.measurement('p6f-final-report-mcp', 'report')
_, current_off, control_off_counts = previous.measurement('p6f-final-control-off-mcp', 'off')
_, current_report, control_report_counts = previous.measurement('p6f-final-control-report-mcp', 'report')
assert off['expected_version'] == report['expected_version'] == '1.8.8'
frozen = previous.read('docs/research/baseline-v190.json')
archived_off = previous.read(base / 'p5-188-off-mcp/replay/report.json')
archived_report = previous.read(base / 'p5-188-report-mcp/replay/report.json')
gates = previous.compare(off_cohorts, {c['connection']: c for c in frozen['mcp_cohorts'] if c['concurrency'] == 1}, 'frozen_archived')
gates += previous.compare(off_cohorts, {c['connection']: c for c in archived_off['cohorts']}, 'p5_off_archived')
gates += previous.compare(off_cohorts, current_off, 'p5_off_contemporary')
gates += previous.compare(report_cohorts, {c['connection']: c for c in archived_report['cohorts']}, 'p5_report_archived')
gates += previous.compare(report_cohorts, current_report, 'p5_report_contemporary')
assert len(gates) == 60
for mode, cohorts in [('off', off_cohorts), ('report', report_cohorts)]:
    prior = previous.read(base / f'p6e-{mode}-mcp/replay/report.json')
    gates += previous.compare(cohorts, {c['connection']: c for c in prior['cohorts']}, f'p6e_{mode}_archived')
assert len(gates) == 84
result = {'phase': 'P6f', 'passed': all(g['passed'] for g in gates), 'limits_changed': False,
          'observations': {'off': off_counts, 'report': report_counts,
                           'control_off': control_off_counts, 'control_report': control_report_counts},
          'gates': gates}
(base / 'p6f-final-gates.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({'passed': result['passed'], 'passed_gates': sum(g['passed'] for g in gates), 'total_gates': len(gates)}))
for gate in gates:
    if not gate['passed']:
        print(json.dumps(gate))
raise SystemExit(0 if result['passed'] else 1)
