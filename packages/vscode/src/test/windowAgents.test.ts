// SPDX-License-Identifier: Apache-2.0
// U104: Management › Agents host side. `agents list`/`agents remove` argv for
// this window's first folder, the connectable entries of the native report
// field for field, and the Connect hook, which writes nothing until the scope
// connection lands. Expected argv are written out here.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { agentEntries, CONNECT_PENDING, WindowAgents } from '../airkctl/features/windowAgents';

const folder = '/work/app', id = '0a'.repeat(16);
const report = { checked: ['/work/app/.mcp.json'], findings: [], blockers: [], entries: [
    { agent: 'claude', scope: 'project', file: '/work/app/.mcp.json', containerPath: ['mcpServers'], serverKey: 'airk', transport: 'https', status: 'ok', url: 'https://127.0.0.1:5000/mcp', connectionId: id },
    { agent: 'copilot', scope: 'project', file: '/work/app/.vscode/mcp.json', containerPath: ['servers'], serverKey: 'airk', transport: 'https', status: null },
    { agent: 'gemini', scope: 'project', file: '/work/app/.gemini/settings.json', containerPath: ['mcpServers'], serverKey: 'airk', transport: 'stdio', status: 'stdio_retired' },
] };

function agents(answer: unknown = report, folderOf: () => string | undefined = () => folder) {
    const calls: string[][] = [];
    let ready = true;
    const host = new WindowAgents({ session: { json: async argv => { calls.push([...argv]); return answer; } } }, folderOf,
        () => { if (!ready) { throw Object.assign(new Error('not ready'), { code: 'service_missing' }); } });
    return { host, calls, setReady(value: boolean) { ready = value; } };
}

test('U104 Agents reads this folder with agents list --no-user and lists the connectable entries with the complete report', async () => {
    const f = agents();
    const view = await f.host.read();
    assert.deepEqual(f.calls, [['agents', 'list', '--project', folder, '--no-user']]);
    assert.equal(view.folder, folder);
    assert.deepEqual(view.entries, [
        { client: 'claude', agent: 'claude', file: '/work/app/.mcp.json', scope: 'project', serverKey: 'airk', connectionId: id, status: 'ok' },
        { client: 'vscode', agent: 'copilot', file: '/work/app/.vscode/mcp.json', scope: 'project', serverKey: 'airk', status: null },
    ]);
    assert.equal(view.report, report, 'the complete native report is returned');
    assert.deepEqual(agentEntries({ entries: [{ agent: 'codex', file: 'c', serverKey: 'airk', connectionId: 'NOT-AN-ID', status: 'ok' }] }),
        [{ client: 'codex', agent: 'codex', file: 'c', scope: 'project', serverKey: 'airk', status: 'ok' }]);
});

test('U104 Disconnect removes only the entries naming that connection; an invalid id or no folder sends nothing', async () => {
    const f = agents({ schema: 1, actions: [], blockers: [] });
    await f.host.disconnect(id);
    assert.deepEqual(f.calls, [['agents', 'remove', '--project', folder, '--no-user', '--connection-id', id]]);
    await assert.rejects(f.host.disconnect('ZZ'), (error: any) => error.code === 'INVALID_MESSAGE');
    const none = agents(report, () => undefined);
    await assert.rejects(none.host.read(), (error: any) => error.code === 'NO_PROJECT');
    f.setReady(false);
    await assert.rejects(f.host.read(), (error: any) => error.code === 'service_missing');
    assert.equal(f.calls.length + none.calls.length, 1);
});

test('U104 the Connect hook answers that it is coming in the next step and runs no command', async () => {
    const f = agents();
    assert.deepEqual(await f.host.connectAgentToScope('codex'), { status: 'pending', client: 'codex', message: CONNECT_PENDING });
    assert.match(CONNECT_PENDING, /^Coming in the next step/);
    assert.deepEqual(f.calls, []);
});
