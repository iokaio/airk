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


test('native wait reasons survive continuation without discarding published progress', () => {
    const observations: ProgressObservation[] = [];
    const tracker = new IndexProgressTracker('a', value => observations.push(value));
    const event = { collection: 'a', phase: 'base_committed', revision: 'revision-a', processed_artifacts: 64,
        committed_artifacts: 32, selected_artifacts: null, completed_artifacts: null };
    assert.equal(tracker.line(JSON.stringify(event)), true);
    for (const phase of ['waiting_capacity', 'waiting_collection']) {
        assert.equal(tracker.line(JSON.stringify({ ...event, phase, revision: null, committed_artifacts: 0 })), true);
        tracker.waiting();
        assert.equal(observations.at(-1)?.event.phase, phase);
        assert.equal(observations.at(-1)?.event.revision, 'revision-a');
        assert.equal(observations.at(-1)?.event.committed_artifacts, 32);
    }
    assert.equal(tracker.line(JSON.stringify({ ...event, phase: 'base_committed', committed_artifacts: 64 })), true);
    assert.equal(observations.at(-1)?.event.phase, 'base_committed');
});
