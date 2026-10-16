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
        console: { error: (error: unknown) => errors.push(error) },
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
    for (const paths of [null, [], packages.slice(1), [...packages, packages[0]], [1, 2, 3, 4]]) {
        assert.throws(() => runPublisher(paths, 'preview'), /Expected four verified platform packages/);
    }
    assert.throws(() => runPublisher(packages, 'other'), /Expected verified inventory and channel/);
});
