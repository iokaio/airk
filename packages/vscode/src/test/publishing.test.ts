// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';

const script = readFileSync(join(__dirname, '../../scripts/publish.cjs'), 'utf8');
const packages = ['linux-x64', 'win32-x64', 'darwin-x64', 'darwin-arm64'].map(p => `verified/${p}.vsix`);

function runPublisher(paths: unknown, channel: string, failure?: Error) {
    const calls: unknown[] = [];
    const errors: unknown[] = [];
    const process = { argv: ['node', 'publish.cjs', 'verified.json', channel], exitCode: 0 };
    const completion = runInNewContext(script, {
        process,
        console: { error: (error: unknown) => errors.push(error), log: () => undefined },
        require: (name: string) => {
            if (name === 'node:fs') { return { readFileSync: () => JSON.stringify(paths) }; }
            assert.equal(name, '@vscode/vsce');
            return { publishVSIX: (files: unknown, options: unknown) => {
                calls.push(JSON.parse(JSON.stringify({ files, options })));
                return failure ? Promise.reject(failure) : Promise.resolve();
            } };
        }
    });
    return { calls, errors, process, completion };
}

test('publishing submits the verified packages with Entra authentication and the selected channel', async () => {
    for (const channel of ['preview', 'stable']) {
        const result = runPublisher(packages, channel);
        await result.completion;
        assert.deepEqual(result.calls, [{ files: packages, options: {
            azureCredential: true, preRelease: channel === 'preview'
        } }]);
        assert.equal(result.process.exitCode, 0);
        assert.deepEqual(result.errors, []);
    }
});

test('authentication or publication failure stops without retrying through another credential', async () => {
    const failure = new Error('Fixture publisher identity rejected');
    const result = runPublisher(packages, 'preview', failure);
    await result.completion;
    assert.equal(result.calls.length, 1);
    assert.equal(result.process.exitCode, 1);
    assert.deepEqual(result.errors, [failure]);
});

test('invalid package inventory or channel cannot reach the publisher', () => {
    for (const paths of [null, [...packages, packages[0]], [packages[0], packages[0]], [1, 2, 3, 4]]) {
        assert.throws(() => runPublisher(paths, 'preview'), /Expected at most four verified, unpublished platform packages/);
    }
    assert.throws(() => runPublisher(packages, 'other'), /Expected verified inventory and channel/);
});

test('a verified partial publication submits only missing platforms and a completed retry uploads nothing', async () => {
    const remaining = packages.slice(1);
    const partial = runPublisher(remaining, 'stable');
    await partial.completion;
    assert.deepEqual(partial.calls, [{ files: remaining, options: { azureCredential: true, preRelease: false } }]);
    const complete = runPublisher([], 'stable');
    await complete.completion;
    assert.deepEqual(complete.calls, []);
    assert.equal(complete.process.exitCode, 0);
});
