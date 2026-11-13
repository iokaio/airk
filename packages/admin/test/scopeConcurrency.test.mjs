// SPDX-License-Identifier: Apache-2.0
// (U108) The admin note helper: rounds, wording, boundaries and the
// `service.resources` parse. Node strips the helper's types (no build step).
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  concurrencyNote,
  concurrencyRounds,
  queryConcurrencyOf,
  setSizeAfterAdd,
  typedScopeCount,
} from '../src/scopeConcurrency.ts';

test('U108 no note at n = concurrency; a note from concurrency + 1; never with an unknown concurrency', () => {
  assert.equal(concurrencyRounds(4, 4), null, 'n = concurrency');
  assert.equal(concurrencyRounds(5, 4), 2, 'concurrency + 1');
  assert.equal(concurrencyRounds(9, 4), 3);
  for (const unknown of [null, 0, -4, 1.5, Number.NaN]) {
    assert.equal(concurrencyRounds(32, unknown), null, `concurrency ${String(unknown)}`);
    assert.equal(concurrencyNote({ indexes: 32, concurrency: unknown, subject: 'scope' }), null);
  }
  assert.equal(concurrencyNote({ indexes: 4, concurrency: 4, subject: 'set' }), null);
});

test('U108 wording: about twice, three times, N times; at least for a lower bound', () => {
  assert.equal(
    concurrencyNote({ indexes: 8, concurrency: 4, subject: 'scope' }),
    'This scope searches 8 indexes; this machine searches 4 at a time, so searches take about twice as long.',
  );
  assert.equal(
    concurrencyNote({ indexes: 12, concurrency: 4, subject: 'scope' }),
    'This scope searches 12 indexes; this machine searches 4 at a time, so searches take about three times as long.',
  );
  assert.equal(
    concurrencyNote({ indexes: 32, concurrency: 8, subject: 'scope' }),
    'This scope searches 32 indexes; this machine searches 8 at a time, so searches take about 4 times as long.',
  );
  assert.equal(
    concurrencyNote({ indexes: 5, concurrency: 4, atLeast: true, subject: 'set' }),
    'This index set will hold at least 5 indexes; this machine searches 4 at a time, so searching it takes at least twice as long.',
  );
});

test('U108 counts: a set after adding (incomplete pages are a lower bound); typed ids with index sets are a lower bound', () => {
  assert.deepEqual(setSizeAfterAdd(['a', 'b', 'c', 'd'], true, 'e'), { count: 5, exact: true });
  assert.deepEqual(setSizeAfterAdd(['a', 'b', 'c', 'd'], true, 'a'), { count: 4, exact: true }, 'an existing member once');
  assert.deepEqual(setSizeAfterAdd(['a', 'b', 'c', 'd'], false, 'e'), { count: 5, exact: false }, 'incomplete memberships');
  assert.deepEqual(typedScopeCount(['a', 'b', 'a'], []), { count: 2, exact: true });
  assert.deepEqual(typedScopeCount(['a'], ['prj']), { count: 1, exact: false });
});

test('U108 the service.resources result gives the configured concurrency; a configuration error or other shape is unknown', () => {
  const resources = (configuration_error, query_concurrency) => ({
    status: 'resources',
    observation: { configuration: { query_concurrency, preset: 'balanced' } },
    age_ms: 0,
    configuration_error,
  });
  assert.equal(queryConcurrencyOf(resources(false, 16)), 16);
  assert.equal(queryConcurrencyOf(resources(true, 16)), null, 'a configuration error reports a fallback');
  assert.equal(queryConcurrencyOf(resources(false, 0)), null);
  assert.equal(queryConcurrencyOf({ status: 'health', observation: {} }), null);
  assert.equal(queryConcurrencyOf(undefined), null);
});
