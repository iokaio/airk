// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { maximumPolicyBytes, withPolicySnapshot } from '../extractionPolicy';

test('policy snapshots bound input, preserve exact bytes and clean up cancellation and errors', async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-original-policy-test-'));
    const input = path.join(directory, 'original.json');
    const original = '{ "version": 1, "rules": [] }\r\n';
    let snapshot = '';
    try {
        await fs.writeFile(input, original);
        await withPolicySnapshot(input, async file => {
            snapshot = file; assert.notEqual(file, input);
            await fs.writeFile(input, '{"version":2}');
            assert.equal(await fs.readFile(file, 'utf8'), original);
        });
        await assert.rejects(fs.stat(snapshot), { code: 'ENOENT' });
        await assert.rejects(withPolicySnapshot(input, async file => { snapshot = file; throw new Error('original canceled action'); }), /original canceled action/);
        await assert.rejects(fs.stat(snapshot), { code: 'ENOENT' });
        await fs.writeFile(input, Buffer.alloc(maximumPolicyBytes + 1, 32));
        await assert.rejects(withPolicySnapshot(input, async () => assert.fail('oversized policy reached native call')), /128 KiB/);
        await assert.rejects(withPolicySnapshot(input, async () => assert.fail('oversized measurement reached native call'), 'Usage measurement', 16_384), /Usage measurement.*16 KiB/);
        await fs.writeFile(input, Buffer.from([0xff]));
        await assert.rejects(withPolicySnapshot(input, async () => assert.fail('invalid UTF-8 reached native call')));
        await assert.rejects(withPolicySnapshot(directory, async () => assert.fail('directory reached native call')), /regular/);
    } finally {
        await fs.rm(input, { force: true }); await fs.rmdir(directory);
    }
});
