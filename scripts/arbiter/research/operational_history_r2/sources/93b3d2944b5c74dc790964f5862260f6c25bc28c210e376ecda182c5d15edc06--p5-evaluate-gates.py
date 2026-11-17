# SPDX-License-Identifier: Apache-2.0
"""Private P5 evidence aggregation; preserve frozen limits and raw inputs."""
import json
import math
from pathlib import Path

BASE = Path('artifacts/v190')
FIXTURE = '5c0ddbe2197afd930d64d6f3dbef844e3247fb52ebb2b35175ca9597397da256'

def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))

def measurement(name, mode):
    report = read(BASE / name / 'replay/report.json')
    assert report['status'] == 'complete' and report['source_unchanged']
    assert report['fixture_sha256'] == FIXTURE
    assert len(report['cohorts']) == 3
    errors = retries = receipts = 0
    cohorts = {}
    for cohort in report['cohorts']:
        assert cohort['concurrency'] == 1 and len(cohort['trials']) == 30
        assert cohort['coverage_complete'] and cohort['cleanup_clean']
        for trial in cohort['trials']:
            assert trial['coverage']['complete'] and trial['publication_consistent']
            assert len(trial['calls']) == 9
            for call in trial['calls']:
                retries += call['retries']
                for attempt in call['attempts']:
                    errors += bool(attempt['is_error'])
                    verification = attempt.get('source_verification')
                    if mode == 'off':
                        assert verification is None
                    elif verification is not None:
                        assert verification['mode'] == 'report' and verification['status'] == 'verified'
                        receipts += 1
        cohorts[cohort['connection']] = cohort
    assert errors == retries == 0
    assert receipts == (720 if mode == 'report' else 0)
    return report, cohorts, {'trials':90,'calls':810,'errors':errors,'retries':retries,'verified_receipts':receipts}

def compare(candidate, reference, label):
    gates = []
    for name, cohort in candidate.items():
        baseline = reference[name]
        for metric in ['task_ms', 'task_with_connection_ms']:
            for quantile, fraction, allowance in [('median', .10, 25), ('p95', .15, 50)]:
                observed, expected = cohort[metric][quantile], baseline[metric][quantile]
                bound = expected + max(expected * fraction, allowance)
                gates.append({'reference':label,'connection':name,'metric':metric,'quantile':quantile,
                              'observed':observed,'reference_value':expected,'bound':bound,'passed':observed<=bound})
    return gates

def main():
    off, off_cohorts, off_counts = measurement('p5-188-off-mcp','off')
    report, report_cohorts, report_counts = measurement('p5-188-report-mcp','report')
    _, current_off, off_control_counts = measurement('p5-188-control-off-mcp','off')
    _, current_report, report_control_counts = measurement('p5-188-control-report-mcp','report')
    assert off['expected_version'] == report['expected_version'] == '1.8.8'
    frozen = read('docs/research/baseline-v190.json')
    archived_off = read(BASE/'p4-final-off-mcp/replay/report.json')
    archived_report = read(BASE/'p4-final-report-mcp/replay/report.json')
    gates = compare(off_cohorts, {c['connection']:c for c in frozen['mcp_cohorts'] if c['concurrency']==1}, 'frozen_archived')
    gates += compare(off_cohorts,{c['connection']:c for c in archived_off['cohorts']},'p4_off_archived')
    gates += compare(off_cohorts,current_off,'p4_off_contemporary')
    gates += compare(report_cohorts,{c['connection']:c for c in archived_report['cohorts']},'p4_report_archived')
    gates += compare(report_cohorts,current_report,'p4_report_contemporary')
    quality = read(BASE/'p5-188-relevance.json')
    quality_reference = read(BASE/'p4-final-relevance.json')
    assert quality['passed'] and quality['binary_sha256'] == quality['binary_sha256_after']
    actual, expected = quality['representative'], quality_reference['representative']
    assert actual['complete'] and actual['corpus_sha256'] == expected['corpus_sha256']
    assert actual['source_hashes'] == expected['source_hashes'] and set(actual['scores']) == set(expected['scores'])
    quality_checks = []
    for group, metrics in expected['scores'].items():
        assert set(actual['scores'][group]) == set(metrics)
        for key, value in metrics.items():
            if key in ['latency_samples','p50_ms','p95_ms']:
                continue
            observed = actual['scores'][group][key]
            passed = observed is None if value is None else math.isclose(observed, value, rel_tol=0, abs_tol=1e-12)
            quality_checks.append({'group':group,'metric':key,'observed':observed,'reference':value,'passed':passed})
    result = {'schema':'airk.p5-local-gates/1','working_version':'1.8.8',
              'limits':'Unchanged frozen median +max(10%,25ms); p95 +max(15%,50ms). Report compared to P4 Report.',
              'candidate_off':off_counts,'candidate_report':report_counts,
              'contemporary_off_control':off_control_counts,'contemporary_report_control':report_control_counts,
              'latency_gates':gates,'quality_gates':quality_checks,
              'all_latency_passed':all(x['passed'] for x in gates),
              'all_quality_passed':all(x['passed'] for x in quality_checks)}
    (BASE/'p5-188-gates.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({k:v for k,v in result.items() if not k.endswith('_gates')},indent=2))
    raise SystemExit(0 if result['all_latency_passed'] and result['all_quality_passed'] else 1)

if __name__ == '__main__':
    main()
