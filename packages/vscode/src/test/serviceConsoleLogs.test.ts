// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import Module from 'node:module';
import * as path from 'node:path';
import { test } from 'node:test';

test('service log opening uses the native 32-record bound and surfaces read failures', async t => {
    const stub = { workspace: { isTrusted: true } };
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    loader._load = function (request: string, ...args: any[]) { return request === 'vscode' ? stub : original.call(this, request, ...args); };
    const calls: string[][] = [], shown: boolean[] = [], lines: string[] = [];
    const page = { schema: 1, records: [{ schema: 1, instance: 'fixture', observed_unix_ms: 123,
        kind: 'model_recovery', project: null, job: null, count: 1, code: 'helper_cooldown_elapsed' }],
        stored_bytes: 257, malformed_records: 0, tail_truncated: false };
    try {
        const bootstrap = require('../serviceBootstrap'); let fail = false;
        t.mock.method(bootstrap, 'nativeRunner', async (_command: string, args: string[]) => {
            calls.push(args); if (fail) { throw new Error('fixture read failure'); }
            assert.equal(Number(args[args.indexOf('--limit') + 1]), 32);
            return page;
        });
        delete require.cache[require.resolve('../serviceConsole')];
        const { ServiceConsole } = require('../serviceConsole');
        const host = { disposed: false, output: { appendLine: (line: string) => lines.push(line), show: (preserve: boolean) => shown.push(preserve) } };
        const observation = { bundle: { main: 'fixture-airk' }, inspection: { log_control_dir: path.resolve('fixture-control') } };
        await ServiceConsole.prototype.logs.call(host, observation);
        assert.deepEqual(calls[0].slice(-4), ['--limit', '32', '--format', 'json']);
        assert.equal(shown[0], false, 'the output channel receives focus');
        assert.deepEqual(JSON.parse(lines[0]), page, 'complete native recovery page survives the console output');
        fail = true;
        await assert.rejects(ServiceConsole.prototype.logs.call(host, observation), /service_logs_read_failed/);
        assert.ok(lines.some(line => line.includes('fixture read failure')));
        stub.workspace.isTrusted = false;
        await assert.rejects(ServiceConsole.prototype.logs.call(host, observation), /no_longer_authorized/);
        assert.equal(calls.length, 2, 'trust refusal does not launch the CLI');
    } finally { loader._load = original; delete require.cache[require.resolve('../serviceConsole')]; }
});
