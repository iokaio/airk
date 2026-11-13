// SPDX-License-Identifier: Apache-2.0
// (U108) The informational query-concurrency note: rounds, wording, boundaries
// and the union count of a pending window selection.
import assert from 'node:assert/strict';
import test from 'node:test';
import { concurrencyNote, concurrencyRounds, observedConcurrency, scopeIndexCount } from '../indexingCatalog';
import { statusSummary } from '../explorerStatus';

test('U108 no note at n = concurrency; a note from concurrency + 1; never with an unknown concurrency', () => {
    assert.equal(concurrencyRounds(4, 4), undefined, 'n = concurrency');
    assert.equal(concurrencyRounds(5, 4), 2, 'concurrency + 1');
    assert.equal(concurrencyRounds(8, 4), 2);
    assert.equal(concurrencyRounds(9, 4), 3);
    assert.equal(concurrencyRounds(32, 4), 8);
    for (const unknown of [undefined, 0, -4, 1.5, Number.NaN]) {
        assert.equal(concurrencyRounds(32, unknown as number | undefined), undefined, `concurrency ${String(unknown)}`);
        assert.equal(concurrencyNote({ indexes: 32, concurrency: unknown as number | undefined, subject: 'scope' }), undefined);
    }
    assert.equal(concurrencyNote({ indexes: 4, concurrency: 4, subject: 'scope' }), undefined);
    assert.equal(concurrencyNote({ indexes: 0, concurrency: 4, subject: 'set' }), undefined);
    assert.equal(observedConcurrency('4'), undefined);
    assert.equal(observedConcurrency(16), 16);
});

test('U108 wording: about twice, three times, N times; at least for a lower bound; scope and index set', () => {
    assert.equal(concurrencyNote({ indexes: 8, concurrency: 4, subject: 'scope' }),
        'This scope searches 8 indexes; this machine searches 4 at a time, so searches take about twice as long.');
    assert.equal(concurrencyNote({ indexes: 5, concurrency: 4, subject: 'scope' }),
        'This scope searches 5 indexes; this machine searches 4 at a time, so searches take about twice as long.');
    assert.equal(concurrencyNote({ indexes: 12, concurrency: 4, subject: 'scope' }),
        'This scope searches 12 indexes; this machine searches 4 at a time, so searches take about three times as long.');
    assert.equal(concurrencyNote({ indexes: 32, concurrency: 8, subject: 'scope' }),
        'This scope searches 32 indexes; this machine searches 8 at a time, so searches take about 4 times as long.');
    assert.equal(concurrencyNote({ indexes: 9, concurrency: 4, atLeast: true, subject: 'scope' }),
        'This scope searches at least 9 indexes; this machine searches 4 at a time, so searches take at least three times as long.');
    assert.equal(concurrencyNote({ indexes: 8, concurrency: 4, subject: 'set' }),
        'This index set will hold 8 indexes; this machine searches 4 at a time, so searching it takes about twice as long.');
});

test('U108 the pending selection count is the union; incomplete memberships give a lower bound', () => {
    const sets = { a: ['p'], b: ['p', 'q'], c: ['q'], d: [] };
    assert.deepEqual(scopeIndexCount({ indexes: ['a', 'x'], projects: ['p'], indexSets: sets, indexSetsComplete: true }), { count: 3, exact: true }, 'members a, b plus x; a once');
    assert.deepEqual(scopeIndexCount({ indexes: [], projects: ['p', 'q'], indexSets: sets, indexSetsComplete: true }), { count: 3, exact: true }, 'shared b once');
    assert.deepEqual(scopeIndexCount({ indexes: ['a', 'x'], projects: [], indexSetsComplete: false }), { count: 2, exact: true }, 'indexes alone are exact without memberships');
    assert.deepEqual(scopeIndexCount({ indexes: ['x'], projects: ['p'], indexSets: sets, indexSetsComplete: false }), { count: 3, exact: false }, 'incomplete memberships');
    assert.deepEqual(scopeIndexCount({ indexes: ['x'], projects: ['p'] }), { count: 1, exact: false }, 'no memberships read');
});

test('U108 the status summary keeps the reported query_concurrency (single and combined reads); absent or invalid stays unknown', () => {
    assert.equal(statusSummary({ index_state: 'available', query_concurrency: 16 }).queryConcurrency, 16);
    assert.equal(statusSummary({ index_state: 'available' }).queryConcurrency, undefined);
    assert.equal(statusSummary({ index_state: 'available', query_concurrency: 0 }).queryConcurrency, undefined);
    assert.equal(statusSummary({ schema: 'airk.combined-read/1', members: [], query_concurrency: 8 }).queryConcurrency, 8);
});
