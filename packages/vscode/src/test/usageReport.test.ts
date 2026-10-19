// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { renderUsage, reductionPercent, usageAction } from '../usageReport';

test('usage headline preserves negative reductions, comparable denominator and missing data', () => {
    const html = renderUsage({ report: { calls: 10, eligible_evidence_calls: 6, comparisons: { full_returned_sources: {
        compared_calls: 2, baseline_utf8_bytes: '80', returned_utf8_bytes: '400', token_estimate: { nominal_reduction: '-80', assumption_low: '-187', assumption_high: '-27' }
    } } }, status: 'partial', health: {}, days: 7 });
    assert.match(html, /<strong>-80<\/strong>/);
    assert.match(html, /<strong>-400.0%<\/strong>/);
    assert.match(html, /6 evidence requests were eligible/);
    assert.match(html, /value="7" selected/);
    assert.match(html, /not measured token or bill savings/);
    assert.equal(reductionPercent('90071992547409930', '9007199254740993'), '90.0%');
    assert.equal(reductionPercent('0', '0'), 'Not measured');
    assert.ok(!renderUsage({ report: {} }).includes('<strong>0%</strong>'));
});

test('supplied task deltas require fully measured matching pairs and retain failures', () => {
    const html = renderUsage({ supplemental: [{ day: 1, summaries: [{ stratum: { client: '<script>' }, paired_with_airk: {
        sessions: 2, input_measured_sessions: 1, input_tokens: '100', elapsed_measured_sessions: 2, elapsed_ms: '500'
    }, paired_without_airk: { sessions: 2, input_measured_sessions: 2, input_tokens: '200', elapsed_measured_sessions: 2, elapsed_ms: '200' },
    paired_quality: [0, 0, 2, 0, 0] }] }] });
    assert.match(html, /Not enough comparable measurements/);
    assert.match(html, /<td>-0.300 s<\/td>/);
    assert.match(html, /0 \/ 0 \/ 2 \/ 0 \/ 0/);
    assert.match(html, /&lt;script&gt;/);
});

test('usage explains unavailable comparisons and separates evidence from polling without backfilling old reasons', () => {
    const html = renderUsage({ report: { calls: 3779, eligible_evidence_calls: 5, missing_baseline_calls: 5,
        mismatched_baseline_calls: 0, comparisons: {}, baseline_unavailable: { inventory_limit: 2 },
        operations: { read_file: { calls: 4 }, symbols: { calls: 1 }, collection_maintenance: { calls: 2865 }, coverage: { calls: 844 }, index: { calls: 59 }, status: { calls: 4 }, file_info: { calls: 1 }, capabilities: { calls: 1 } }
    } });
    assert.match(html, /0 of 5 eligible requests compared; 5 unavailable/);
    assert.match(html, /Evidence requests: 5/);
    assert.match(html, /Background and other activity: 3,774/);
    assert.match(html, /Older index exceeds the comparison inventory limit<\/td><td>2/);
    assert.match(html, /Reason not recorded by the earlier recorder<\/td><td>3/);
    assert.match(html, /restart connected agent MCP sessions/);
    assert.match(html, /cannot be reconstructed/);
    assert.ok(!html.includes('<strong>0%</strong>'));
});

test('usage actions reject arbitrary commands, paths, invalid windows and surplus keys', () => {
    assert.deepEqual(usageAction({ type: 'usage', action: 'days', days: 7 }), { type: 'usage', action: 'days', days: 7 });
    for (const input of [{ type: 'usage', action: 'days', days: 365 }, { type: 'usage', action: 'clear', path: 'x' }, { type: 'usage', action: 'runCommand' }, null]) {
        assert.equal(usageAction(input), undefined);
    }
});
