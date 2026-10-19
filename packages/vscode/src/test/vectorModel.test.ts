// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { installModelAssets, ModelAsset, writeModelAsset } from '../vectorModel';

const bytes = Buffer.from('original model fixture');
const asset: ModelAsset = { name: 'model.onnx', url: 'https://example.invalid/fixture', bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') };
async function* stream(value = bytes) { yield value.subarray(0, 5); yield value.subarray(5); }

test('model download enforces exact size and digest, including streamed overflow', async () => {
    const directory = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), 'airk-vector-test-'));
    try {
        const target = path.join(directory, 'model.onnx');
        await writeModelAsset(target, asset, stream());
        assert.deepEqual(await fs.readFile(target), bytes);
        await assert.rejects(writeModelAsset(target, asset, stream()), /EEXIST/);
        for (const [name, value] of [['overflow', Buffer.concat([bytes, bytes])], ['short', bytes.subarray(0, 4)], ['wrong', Buffer.alloc(bytes.length)]] as const) {
            await assert.rejects(writeModelAsset(path.join(directory, name), asset, stream(value)), /pinned size|digest mismatch/);
        }
    } finally { await fs.rm(directory, { recursive: true, force: true }); }
});

test('model setup publishes complete assets atomically, cleans failures and preserves custom models', async () => {
    const directory = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), 'airk-vector-test-'));
    const destination = path.join(directory, 'models', 'local');
    let calls = 0;
    try {
        await assert.rejects(installModelAssets(directory, [asset], {}, async function* () {
            await assert.rejects(fs.stat(destination), /ENOENT/);
            throw new Error('offline fixture');
        }, () => {}), /offline fixture/);
        assert.deepEqual(await fs.readdir(path.dirname(destination)), []);
        await installModelAssets(directory, [asset], { fixture: true }, async function* () { calls++; yield* stream(); }, () => {});
        assert.equal(calls, 1);
        assert.deepEqual(await fs.readFile(path.join(destination, 'model.onnx')), bytes);
        assert.deepEqual(JSON.parse(await fs.readFile(path.join(destination, 'model.json'), 'utf8')), { fixture: true });
        await installModelAssets(directory, [asset], {}, async function* () { throw new Error('must not download'); }, () => {});
        assert.deepEqual(JSON.parse(await fs.readFile(path.join(destination, 'model.json'), 'utf8')), { fixture: true });
        await fs.unlink(path.join(destination, 'model.json'));
        await assert.rejects(installModelAssets(directory, [asset], {}, () => stream(), () => {}), /incomplete/);
    } finally { await fs.rm(directory, { recursive: true, force: true }); }
});
