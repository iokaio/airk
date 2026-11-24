// SPDX-License-Identifier: Apache-2.0
// U104, U110: Management › Agents host side. `agents list --no-user` for this
// window's first folder, `agents endpoint`, `scope defaults` resolved for each
// window folder, the URL-only `agents connect`, and removal of one agent's
// project entries with the complete native rows. Following the window's scope
// as the workspace default is in windowAgentsOpen.test.ts. Expected argv and
// views are written out here, never derived from the code under test.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { AirkctlCommandError } from '../airkctl/session';
import { agentEntries, endpointOf, WindowAgents, type AgentScopeSource } from '../airkctl/features/windowAgents';

const folder = '/work/app', docs = '/work/docs', url = 'http://127.0.0.1:5000/mcp';
const idx = (n: number) => 'idx_' + n.toString(16).padStart(2, '0').repeat(16);
const prj = (n: number) => 'prj_' + n.toString(16).padStart(2, '0').repeat(16);
const claudeRow = { agent: 'claude', scope: 'project', file: '/work/app/.mcp.json', containerPath: ['mcpServers'], serverKey: 'airk', transport: 'https', status: 'ok', url };
const report = { checked: ['/work/app/.mcp.json'], findings: [], blockers: [], entries: [
    claudeRow,
    { agent: 'copilot', scope: 'project', file: '/work/app/.vscode/mcp.json', containerPath: ['servers'], serverKey: 'airk', transport: 'https', status: null },
    { agent: 'gemini', scope: 'project', file: '/work/app/.gemini/settings.json', containerPath: ['mcpServers'], serverKey: 'airk', transport: 'stdio', status: 'stdio_retired' },
] };
const endpoint = { url, state: 'ready' };
const defaults = { schema: 1, entries: [
    { folder: '/work', index_ids: [idx(1)], index_set_ids: [], applied_unix_ms: 10 },
    { folder: '/work/app', index_ids: [idx(5)], index_set_ids: [prj(1)], applied_unix_ms: 20 },
] };

type Answer = (argv: string[]) => unknown;
function agents(answers: Record<string, Answer>, folders: string[] = [folder, docs], scope?: AgentScopeSource,
    state: { kind: string; capabilities?: string[] } | undefined = { kind: 'ready', capabilities: ['agents-entry-remove/1'] }) {
    const calls: string[][] = [], logs: string[] = [];
    let ready = true;
    const host = new WindowAgents({ session: { json: async argv => {
        calls.push([...argv]);
        const answer = answers[`${argv[0]} ${argv[1]}`];
        if (!answer) { throw new Error(`unexpected ${argv.join(' ')}`); }
        return answer([...argv]);
    } }, ...(state ? { state } : {}) }, () => folders,
        () => { if (!ready) { throw Object.assign(new Error('not ready'), { code: 'service_missing' }); } }, line => logs.push(line), { scope });
    return { host, calls, logs, setReady(value: boolean) { ready = value; } };
}
const names: AgentScopeSource['choiceInfo'] = (_kind, id) => id === prj(1) ? { name: 'Application set', description: 'Services & <UI>' } : id === idx(5) ? { name: 'Docs' } : undefined;

test('U110 Agents reads entries, the endpoint and the workspace default of each window folder, with the complete native reports', async () => {
    const f = agents({ 'agents list': () => report, 'agents endpoint': () => endpoint, 'agents manual': argv => ({client:argv[3],scope:'project',file:'/work/app/native-target',format:'json',text:'native exact text\n'}), 'scope defaults': () => defaults }, [folder, docs, '/elsewhere'],
        { agentDefaultScope: () => ({ index_ids: [idx(5)], project_ids: [prj(1)] }), choiceInfo: names });
    const view = await f.host.read();
    assert.deepEqual(f.calls, [['agents', 'list', '--project', folder, '--no-user'], ['agents', 'endpoint'], ['agents', 'manual', '--client', 'claude', '--project', folder], ['agents', 'manual', '--client', 'codex', '--project', folder], ['agents', 'manual', '--client', 'vscode', '--project', folder], ['scope', 'defaults']]);
    assert.deepEqual(view.manual.map(entry=>entry.client),['claude','codex','vscode']);
    assert.ok(view.manual.every(entry=>entry.text==='native exact text\n'));
    assert.equal(view.folder, folder);
    assert.deepEqual(view.folders, [folder, docs, '/elsewhere']);
    assert.deepEqual(view.entries, [
        { client: 'claude', agent: 'claude', file: '/work/app/.mcp.json', scope: 'project', serverKey: 'airk', url, status: 'ok' },
        { client: 'vscode', agent: 'copilot', file: '/work/app/.vscode/mcp.json', scope: 'project', serverKey: 'airk', status: null },
    ]);
    assert.equal(view.report, report, 'the complete native report is returned');
    assert.deepEqual(view.endpoint, { url, state: 'ready' });
    assert.equal(view.endpointReport, endpoint);
    assert.equal(view.defaultsReport, defaults);
    const items = [{ kind: 'set', id: prj(1), name: 'Application set', description: 'Services & <UI>' }, { kind: 'index', id: idx(5), name: 'Docs' }];
    assert.deepEqual(view.workspaceDefaults, [
        { folder, stored: { folder: '/work/app', index_ids: [idx(5)], index_set_ids: [prj(1)], applied_unix_ms: 20, items } },
        { folder: docs, stored: { folder: '/work', index_ids: [idx(1)], index_set_ids: [], applied_unix_ms: 10, items: [{ kind: 'index', id: idx(1) }] } },
        { folder: '/elsewhere', stored: null },
    ], 'the most specific stored folder containing each window folder applies');
    assert.deepEqual(view.windowScope, { index_ids: [idx(5)], project_ids: [prj(1)], items });
    assert.equal(view.followError, undefined);
});

test('U110 an unreadable endpoint or defaults document is reported on the view; entries still load', async () => {
    const refused = Object.assign(new Error('Service stopping.'), { code: 'service_missing' });
    const f = agents({ 'agents list': () => report, 'agents endpoint': () => { throw refused; }, 'scope defaults': () => ({ schema: 1, entries: [{ folder: 3 }] }) });
    const view = await f.host.read();
    assert.equal(view.endpoint, null);
    assert.deepEqual(view.endpointError, { code: 'service_missing', message: 'Service stopping.' });
    assert.equal(view.defaultsError?.code, 'agent_command_failed');
    assert.deepEqual(view.workspaceDefaults, [{ folder, stored: null }, { folder: docs, stored: null }]);
    assert.equal(view.entries.length, 2);
    assert.deepEqual(agentEntries({ entries: [{ agent: 'codex', file: 'c', serverKey: 'airk', url: 'https://example.com/mcp', status: 'ok' }] }),
        [{ client: 'codex', agent: 'codex', file: 'c', scope: 'project', serverKey: 'airk', status: 'ok' }], 'a non-loopback URL is not shown');
    assert.deepEqual(endpointOf({ endpoint: { url } }), { url });
    assert.equal(endpointOf({ url: 'https://127.0.0.1:5000/mcp' }), undefined, 'local trust plan C: a legacy https URL is never the endpoint');
    assert.deepEqual(agentEntries({ entries: [{ agent: 'claude', file: 'c', serverKey: 'airk', url: 'https://127.0.0.1:5000/mcp', status: 'https_endpoint_mismatch' }] }),
        [{ client: 'claude', agent: 'claude', file: 'c', scope: 'project', serverKey: 'airk', url: 'https://127.0.0.1:5000/mcp', status: 'https_endpoint_mismatch' }],
        'a legacy https entry keeps its URL so the mismatch is visible');
});

test('U110 Connect writes the URL-only entry with agents connect and stores the window scope as the workspace default of every folder', async () => {
    const connected = { schema: 1, client: 'codex', scope: 'project', file: '/work/app/.codex/config.toml', serverKey: 'airk', url, changed: true };
    const f = agents({ 'agents connect': () => connected, 'scope default': () => ({ schema: 1, entries: [] }) }, [folder, docs],
        { agentDefaultScope: () => ({ index_ids: [idx(5)], project_ids: [prj(1)] }), choiceInfo: names });
    const outcome = await f.host.connect('codex');
    assert.deepEqual(f.calls, [
        ['agents', 'connect', '--client', 'codex', '--project', folder],
        ['scope', 'default', '--workspace', folder, '--workspace', docs, '--index', idx(5), '--set', prj(1)],
    ]);
    assert.deepEqual(outcome, { status: 'connected', client: 'codex', connected, file: '/work/app/.codex/config.toml', url, changed: true,
        scope: [{ kind: 'set', id: prj(1), name: 'Application set', description: 'Services & <UI>' }, { kind: 'index', id: idx(5), name: 'Docs' }] });
    assert.ok(f.calls.every(argv => !argv.includes('--open') && !argv.includes('--connection-id')), 'no connection is enrolled or named');
});

test('U110 Connect without an applied scope only writes the entry; a refused default is reported, never thrown', async () => {
    const connected = { schema: 1, client: 'claude', scope: 'project', file: '/work/app/.mcp.json', serverKey: 'airk', url, changed: false };
    const none = agents({ 'agents connect': () => connected }, [folder], { agentDefaultScope: () => undefined });
    assert.deepEqual(await none.host.connect('claude'), { status: 'connected', client: 'claude', connected, file: '/work/app/.mcp.json', url, changed: false, scope: null });
    assert.deepEqual(none.calls, [['agents', 'connect', '--client', 'claude', '--project', folder]]);
    const failing = agents({ 'agents connect': () => connected, 'scope default': () => { throw Object.assign(new Error('Too many entries.'), { code: 'workspace_defaults_full' }); } }, [folder],
        { agentDefaultScope: () => ({ index_ids: [idx(4)], project_ids: [] }) });
    const outcome = await failing.host.connect('claude');
    assert.deepEqual(outcome.defaultError, { code: 'workspace_defaults_full', message: 'Too many entries.' });
    assert.match(failing.logs.at(-1)!, /not stored as the workspace default \(workspace_defaults_full\)/);
});

test('U110 Remove deletes only this folder\'s project entries for that agent, as complete native rows; no folder or service sends nothing', async () => {
    const f = agents({ 'agents list': () => report, 'agents remove': () => ({ schema: 1, actions: [], blockers: [] }) });
    assert.deepEqual(await f.host.disconnect('claude'), { removed: { schema: 1, actions: [], blockers: [] } });
    assert.deepEqual(f.calls, [['agents', 'list', '--project', folder, '--no-user'],
        ['agents', 'remove', '--project', folder, '--no-user', '--entry', JSON.stringify(claudeRow)]]);
    await assert.rejects(f.host.disconnect('codex'), (error: any) => error.code === 'agent_entry_not_listed');
    const unverified = agents({ 'agents list': () => report }, [folder], undefined, { kind: 'ready', capabilities: [] });
    await assert.rejects(unverified.host.disconnect('claude'), /verified CLI support/);
    assert.ok(!unverified.calls.some(argv => argv[1] === 'remove'));
    const empty = agents({}, []);
    await assert.rejects(empty.host.read(), (error: any) => error.code === 'NO_PROJECT');
    await assert.rejects(empty.host.connect('claude'), (error: any) => error.code === 'NO_PROJECT');
    f.setReady(false);
    const before = f.calls.length;
    await assert.rejects(f.host.read(), (error: any) => error.code === 'service_missing');
    assert.equal(f.calls.length + empty.calls.length, before);
});

test('typed automatic refusal keeps the complete original result and native manual entry without storing scope', async () => {
 const manual={client:'claude' as const,scope:'project' as const,file:'/work/app/.mcp.json',format:'json' as const,text:'native exact entry\n'};
 const refusal={reason:'automatic_policy',file:manual.file,detail:'parent grants foreign mutation',manual};
 const original:any={exitCode:1,class:'refused',stdout:'',stderr:'',error:{airkctl_error:1,class:'refused',code:'agent_config_manual_required',message:'Manual edit required',retryable:false,native:refusal}};
 const f=agents({'agents connect':()=>{throw new AirkctlCommandError([],original);}},[folder],{agentDefaultScope:()=>({index_ids:[idx(1)],project_ids:[]})});
 const result=await f.host.connect('claude');assert.equal(result.status,'refused');assert.strictEqual(result.connected,original);assert.strictEqual(result.refusal,refusal);assert.strictEqual(result.manual,manual);assert.deepEqual(f.calls,[['agents','connect','--client','claude','--project',folder]]);
});
test('malformed typed refusal is not treated as a safe manual fallback',async()=>{
 const original:any={exitCode:1,class:'refused',stdout:'',stderr:'',error:{native:{reason:'automatic_policy',file:folder,detail:'refused',manual:{text:5}},message:'bad'}};
 const failure=new AirkctlCommandError([],original),f=agents({'agents connect':()=>{throw failure;}});
 await assert.rejects(f.host.connect('claude'),error=>error===failure);
});
