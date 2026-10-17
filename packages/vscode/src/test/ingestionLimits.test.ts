// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { authorizeLimitIncrease, indexWithLimitApproval, LimitApprovalPort, LimitsDescription, ScanLimits } from '../ingestionLimits';

const mib = 1048576;
const normal: ScanLimits = { file_bytes: 256 * mib, corpus_bytes: 2048 * mib, extracted_bytes: 4096 * mib,
    read_bytes: 8192 * mib, files: 100000, discovery_entries: 1000000 };
const description: LimitsDescription = { mode: 'automatic', detected_memory_bytes: 16 * 1024 * mib,
    automatic: normal, effective: normal, maximum: { ...normal, corpus_bytes: 65536 * mib,
        extracted_bytes: 131072 * mib, read_bytes: 131072 * mib, files: 1000000, discovery_entries: 10000000 } };
const failure = new Error('scheduled indexing failed: corpus exceeds configured byte limit: 2147483650 > 2147483648 eligible source bytes at example.txt; previous generation retained');
function fixture() {
    const saved: { proposed: ScanLimits; expected: ScanLimits }[] = [], messages: string[] = [];
    const port: LimitApprovalPort = {
        describe: async () => structuredClone(description),
        review: async message => { messages.push(message); return true; },
        input: async (_prompt, value, validate) => { assert.equal(validate(value), undefined); return value; },
        confirm: async message => { messages.push(message); return true; },
        save: async (proposed, expected) => { saved.push({ proposed, expected }); }
    };
    return { port, saved, messages };
}

test('larger limits require approval, explain resource cost and preserve every unrelated setting', async () => {
    const { port, saved, messages } = fixture();
    assert.equal(await authorizeLimitIncrease(port, 'Example', failure), true);
    assert.deepEqual(saved, [{ proposed: { ...normal, corpus_bytes: 4096 * mib }, expected: normal }]);
    assert.match(messages[1], /exceeds the normal limit for this machine/);
    assert.match(messages[1], /indexing time, memory use and disk usage/);
    assert.match(messages[1], /Disk quotas and per-file extraction limits still apply/);
    assert.match(messages[1], /previous index is retained/);
});

test('dismissal at any approval step leaves limits unchanged', async () => {
    for (const phase of ['review', 'input', 'confirm']) {
        const { port, saved } = fixture();
        if (phase === 'review') { port.review = async () => false; }
        if (phase === 'input') { port.input = async () => undefined; }
        if (phase === 'confirm') { port.confirm = async () => false; }
        assert.equal(await authorizeLimitIncrease(port, 'Example', failure), false);
        assert.equal(saved.length, 0);
    }
});

test('invalid increases, hard ceilings and unrelated failures cannot save a larger policy', async () => {
    for (const value of ['NaN', 'Infinity', '', '-1', '2048', '65537']) {
        const { port, saved } = fixture(); port.input = async () => value;
        await assert.rejects(authorizeLimitIncrease(port, 'Example', failure), /Enter more than/);
        assert.equal(saved.length, 0);
    }
    const { port, saved } = fixture();
    port.describe = async () => ({ ...description, effective: description.maximum });
    port.review = async () => { throw new Error('must not prompt at hard ceiling'); };
    assert.equal(await authorizeLimitIncrease(port, 'Example', failure), false);
    port.describe = async () => { throw new Error('must not inspect unrelated failure'); };
    for (const message of ['disk quota exhausted', 'text exceeds 32 MiB extraction limit', 'invalid source', 'native automatic scheduling is paused']) {
        assert.equal(await authorizeLimitIncrease(port, 'Example', new Error(message)), false);
    }
    assert.equal(saved.length, 0);
});

test('each collection budget is identified without confusing source and extracted bytes', async () => {
    for (const [message, key] of [
        ['extracted corpus exceeds configured byte limit', 'extracted_bytes'],
        ['corpus exceeds configured file count', 'files'],
        ['workspace discovery entry budget exhausted', 'discovery_entries'],
        ['previous-path reconciliation budget exhausted', 'discovery_entries'],
        ['workspace raw source read budget exhausted', 'read_bytes']
    ] as const) {
        const { port, saved } = fixture();
        assert.equal(await authorizeLimitIncrease(port, 'Example', new Error(message)), true);
        assert.deepEqual(saved[0].proposed, { ...normal, [key]: normal[key] * 2 });
    }
});

test('approval permits one retry; cancellation, stale-policy rejection and repeated failure do not loop', async () => {
    let runs = 0, approvals = 0;
    const run = async () => { runs++; throw failure; };
    await assert.rejects(indexWithLimitApproval(run, async () => { approvals++; return true; }), failure);
    assert.equal(runs, 2); assert.equal(approvals, 1);
    runs = 0;
    await assert.rejects(indexWithLimitApproval(run, async () => false), failure); assert.equal(runs, 1);
    runs = 0;
    const { port } = fixture(); port.save = async () => { throw new Error('limits changed while approval was pending'); };
    await assert.rejects(indexWithLimitApproval(run, error => authorizeLimitIncrease(port, 'Example', error)), /limits changed/);
    assert.equal(runs, 1);
    runs = 0;
    assert.equal(await indexWithLimitApproval(async () => { if (++runs === 1) { throw failure; } return 'published'; }, async () => true), 'published');
});
