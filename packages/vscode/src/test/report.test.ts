// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { renderReport } from '../report';

test('coverage report labels incomplete measurements and escapes native text under a strict CSP', () => {
    const html = renderReport('AIRK <coverage>', 'coverage', {
        coverage: { scan_complete: false, percent: 100, eligible: 3, current: 2, extraction_failed: 1 },
        retention: { policy: { policy: { mode: 'automatic' }, origin: 'default' }, protected_bytes: null,
            pending_intents: 1, last_maintenance: { error: '<img src=x onerror=alert(1)>' } },
        explanation: 'Original evidence <script>alert(1)</script>'
    });
    assert.match(html, /default-src 'none'/);
    assert.match(html, /script-src 'nonce-[a-f0-9]+'/);
    assert.match(html, /Open as JSON/);
    assert.match(html, /Coverage.*Unknown/s, 'Incomplete inventory cannot display 100%');
    assert.match(html, /&lt;img src=x onerror=alert\(1\)&gt;/);
    assert.match(html, /&lt;script&gt;alert\(1\)&lt;\/script&gt;/);
    assert.ok(!html.includes('<img src=x') && !html.includes('<script>alert(1)</script>'));
});

test('usage report formats bounded operation rows and preserves unknowns', () => {
    const html = renderReport('Usage', 'usage', {
        report: { calls: 4, dropped_observations: null, operations: {
            search: { calls: 3, successful: 2, failed: 1, returned_utf8_bytes: '9007199254740993' }
        }, estimated_bill_savings: null, provider_token_usage_observed: false },
        status: 'partial', health: { complete_history: false }
    });
    assert.match(html, /<th scope="row">search<\/th><td>3<\/td><td>2<\/td><td>1<\/td><td>9007199254740993<\/td>/);
    assert.match(html, /Dropped observations<\/span><strong>Unknown<\/strong>/);
    assert.match(html, /counterfactual estimates are not measured provider tokens or billed savings/);
    assert.match(html, /Complete history<\/th><td>No<\/td>/);
});
