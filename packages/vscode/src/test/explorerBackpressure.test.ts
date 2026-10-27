// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { PassThrough } from 'node:stream';
import { test } from 'node:test';
import { serve } from '../explorerAssistance/server';

test('Assistance bounds queued work and reports overload after draining accepted responses', { timeout: 5000 }, async () => {
    const input = new PassThrough();
    let release!: () => void, handled = 0;
    const gate = new Promise<void>(resolve => { release = resolve; });
    const server = { handle: async () => { await gate; handled++; return { id: handled, result: {} }; } } as unknown as Parameters<typeof serve>[0];
    const responses: any[] = [];
    const serving = serve(server, input, async text => { await new Promise(resolve => setImmediate(resolve)); responses.push(JSON.parse(text)); });
    input.end(('{}\n').repeat(100));
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(handled, 0); release(); await serving; input.destroy();
    assert.equal(handled, 64);
    assert.equal(responses.length, 65);
    assert.match(responses.at(-1).error.message, /queue full/);
});
