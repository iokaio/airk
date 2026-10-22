// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { IndexProgressTracker, ProgressObservation } from '../indexProgress';

test('collection progress ignores foreign workers and preserves the publication while waiting', () => {
    const observations: ProgressObservation[] = []; let now = 100;
    const tracker = new IndexProgressTracker('a', value => observations.push(value), () => now);
    tracker.start(); now = 200;
    const event = { collection: 'b', phase: 'base_committed', revision: 'revision-b', processed_artifacts: 32,
        committed_artifacts: 32, selected_artifacts: null, completed_artifacts: null };
    assert.equal(tracker.line(JSON.stringify(event)), false); assert.equal(observations.length, 1);
    assert.equal(tracker.line(JSON.stringify({ ...event, collection: 'a', revision: 'revision-a' })), true);
    tracker.waiting();
    assert.deepEqual(observations.at(-1), { event: { ...event, collection: 'a', revision: 'revision-a', phase: 'waiting' }, started: 100, updated: 200 });
});
