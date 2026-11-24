// SPDX-License-Identifier: Apache-2.0
// U110: the window's applied search scope becomes the workspace default of
// every folder of the window (`airkctl scope default --workspace <folder>…`;
// a cleared scope clears it). Runs are serialized with the newest scope
// winning, an unchanged automatic scope is not sent again, and a failure is
// reported on the Agents view without rejecting. The native JSON is recorded
// fakes; expected argv and views are written out here, never derived from the
// code under test.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { WindowAgents } from '../airkctl/features/windowAgents';

const app = 'C:\\work\\app', docs = 'D:\\docs';
const idx = (n: number) => 'idx_' + n.toString(16).padStart(2, '0').repeat(16);
const prj = (n: number) => 'prj_' + n.toString(16).padStart(2, '0').repeat(16);

function fixture(answer: (argv: string[]) => unknown, folders: () => string[] = () => [app, docs]) {
    const calls: string[][] = [], logs: string[] = [];
    const host = new WindowAgents({ session: { json: async argv => { calls.push([...argv]); return answer([...argv]); } } }, folders, () => {}, line => logs.push(line));
    return { host, calls, logs };
}

test('U110 following stores each scope for every window folder, one run at a time, and the newest scope wins', async () => {
    const gates: (() => void)[] = [];
    const f = fixture(() => new Promise(resolve => { gates.push(() => resolve({ schema: 1, entries: [] })); }));
    const a = { index_ids: [idx(1)], project_ids: [] }, b = { index_ids: [idx(2)], project_ids: [] }, c = { index_ids: [idx(3)], project_ids: [prj(1)] };
    const run = f.host.followScope(a, true);
    await Promise.resolve();
    void f.host.followScope(b, true); void f.host.followScope(c, true);
    for (let turn = 0; turn < 8 && f.calls.length < 2; turn++) {
        assert.equal(gates.length, f.calls.length, 'one scope default is in flight at a time');
        gates.shift()?.(); await new Promise(resolve => setImmediate(resolve));
    }
    gates.shift()?.(); await run; await f.host.settled();
    assert.deepEqual(f.calls, [
        ['scope', 'default', '--workspace', app, '--workspace', docs, '--index', idx(1)],
        ['scope', 'default', '--workspace', app, '--workspace', docs, '--index', idx(3), '--set', prj(1)],
    ], 'the superseded scope is skipped');
});

test('U110 a cleared scope clears the defaults; an unchanged automatic scope is not sent again; a failure is reported, never thrown', async () => {
    let fail = false;
    const f = fixture(argv => {
        if (argv[0] === 'scope' && argv[1] === 'default' && fail) { throw Object.assign(new Error('The control root is read-only.'), { code: 'workspace_defaults_unwritable' }); }
        if (argv[1] === 'list') { return { entries: [] }; }
        if (argv[1] === 'endpoint') { return { url: 'http://127.0.0.1:5000/mcp', state: 'ready' }; }
        if (argv[1] === 'defaults') { return { schema: 1, entries: [] }; }
        return { schema: 1, entries: [] };
    });
    const auto = { index_ids: [idx(4)], project_ids: [] };
    await f.host.followScope(auto, false);
    await f.host.followScope(auto, false);
    await f.host.followScope(null, true);
    assert.deepEqual(f.calls, [
        ['scope', 'default', '--workspace', app, '--workspace', docs, '--index', idx(4)],
        ['scope', 'default', '--workspace', app, '--workspace', docs],
    ]);
    fail = true;
    await f.host.followScope(auto, true);
    assert.match(f.logs.at(-1)!, /not stored as the workspace default \(workspace_defaults_unwritable\): The control root is read-only\./);
    const view = await f.host.read();
    assert.deepEqual(view.followError, { code: 'workspace_defaults_unwritable', message: 'The control root is read-only.' });
    fail = false;
    await f.host.followScope(auto, false);
    assert.equal((await f.host.read()).followError, undefined, 'the next success clears the failure');
});

test('U110 the same scope is sent again when the window folders change; a window without folders sends nothing', async () => {
    let folders = [app];
    const f = fixture(() => ({ schema: 1, entries: [] }), () => folders);
    const scope = { index_ids: [], project_ids: [prj(2)] };
    await f.host.followScope(scope, false);
    folders = [app, docs];
    await f.host.followScope(scope, false);
    folders = [];
    await f.host.followScope(null, true);
    assert.deepEqual(f.calls, [
        ['scope', 'default', '--workspace', app, '--set', prj(2)],
        ['scope', 'default', '--workspace', app, '--workspace', docs, '--set', prj(2)],
    ]);
});
