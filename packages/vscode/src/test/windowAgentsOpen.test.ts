// SPDX-License-Identifier: Apache-2.0
// U105: Management › Agents with open connections. Exact argv of
// `agents connect --open`, `agents default` (scope from the window's union or
// automatic choice, choice on/off, clear), `agents revoke`; following runs
// `agents default` for followed connections only, serialized with the newest
// scope winning, and surfaces failures without rejecting. The native JSON is
// recorded fakes; expected argv and views are written out here, never derived
// from the code under test.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { OPEN_AGENTS_KEY, WindowAgents, type AgentScopeSource } from '../airkctl/features/windowAgents';

const folder = '/work/app';
const first = '1a'.repeat(16), second = '2b'.repeat(16), grantBound = '3c'.repeat(16);
const idx = (n: number) => 'idx_' + n.toString(16).padStart(2, '0').repeat(16);
const prj = (n: number) => 'prj_' + n.toString(16).padStart(2, '0').repeat(16);
const open = (revision: number, index_ids: string[], set_ids: string[], choice: 'off' | 'on' = 'off') =>
    ({ defaults_revision: revision, default_index_ids: index_ids, default_index_set_ids: set_ids, agent_choice: choice });

function memento(initial: Record<string, unknown> = {}) {
    const values = new Map(Object.entries(initial));
    return { values, get: <T>(key: string) => values.get(key) as T | undefined, update: async (key: string, value: unknown) => { values.set(key, value); } };
}
interface Answers { [verb: string]: (argv: string[]) => unknown | Promise<unknown> }
function fixture(answers: Answers, scope: AgentScopeSource['agentDefaultScope'] = () => undefined, saved: unknown[] = []) {
    const calls: string[][] = [], logs: string[] = [];
    const store = memento(saved.length ? { [OPEN_AGENTS_KEY]: saved } : {});
    const source: AgentScopeSource = {
        agentDefaultScope: scope,
        choiceInfo: (kind, id) => id === prj(1) ? { name: 'Application set', description: 'Services & <UI>' } : id === idx(5) ? { name: 'Docs' } : undefined,
    };
    const host = new WindowAgents({ session: { json: async argv => {
        calls.push([...argv]);
        const answer = answers[argv[1]];
        if (!answer) { throw new Error(`unexpected ${argv.join(' ')}`); }
        return answer([...argv]);
    } } }, () => folder, () => {}, line => logs.push(line), { store, scope: source });
    return { host, calls, logs, store };
}
const refused = (code: string, message: string) => Object.assign(new Error(message), { code });

test('U105 Connect enrolls an open connection, then sets its default to the window\'s union scope; the connection is remembered as followed', async () => {
    const connected = { schema: 1, client: 'claude', scope: 'project', file: '/work/app/.mcp.json', serverKey: 'airk', connectionId: first,
        url: 'https://127.0.0.1:5000/mcp', changed: true, open: open(1, [], []) };
    const defaults = { schema: 1, connectionId: first, open: open(2, [idx(5)], [prj(1)]) };
    const f = fixture({ list: () => ({ entries: [] }), connect: () => connected, default: () => defaults },
        () => ({ index_ids: [idx(5)], project_ids: [prj(1)] }));
    const outcome = await f.host.connectAgentToScope('claude');
    assert.deepEqual(f.calls, [
        ['agents', 'list', '--project', folder, '--no-user'],
        ['agents', 'connect', '--open', '--client', 'claude', '--project', folder],
        ['agents', 'default', '--connection-id', first, '--index', idx(5), '--set', prj(1)],
    ]);
    assert.deepEqual(outcome, { status: 'connected', client: 'claude', connectionId: first, connected, file: '/work/app/.mcp.json', changed: true,
        defaults, open: open(2, [idx(5)], [prj(1)]),
        scope: [{ kind: 'set', id: prj(1), name: 'Application set', description: 'Services & <UI>' }, { kind: 'index', id: idx(5), name: 'Docs' }], follows: true });
    assert.deepEqual(f.store.values.get(OPEN_AGENTS_KEY), [{ connectionId: first, client: 'claude', follow: true }]);
});

test('U105 the automatic scope is sent as the window\'s index; a refused default keeps the connection and reports the native code', async () => {
    const f = fixture({ list: () => ({ entries: [] }),
        connect: () => ({ schema: 1, client: 'codex', file: '/work/app/.codex/config.toml', serverKey: 'airk', connectionId: second, changed: true, open: open(1, [], []) }),
        default: () => { throw refused('scope_name_unknown', 'The index is unknown.'); } },
        () => ({ index_ids: [idx(4)], project_ids: [] }));
    const outcome = await f.host.connectAgentToScope('codex');
    assert.deepEqual(f.calls.at(-1), ['agents', 'default', '--connection-id', second, '--index', idx(4)]);
    assert.deepEqual(outcome.defaultsError, { code: 'scope_name_unknown', message: 'The index is unknown.' });
    assert.equal(outcome.status, 'connected');
    assert.deepEqual(outcome.open, open(1, [], []), 'the enrolled defaults are reported when the change was refused');
    assert.deepEqual(f.store.values.get(OPEN_AGENTS_KEY), [{ connectionId: second, client: 'codex', follow: true }]);
});

test('U105 Connect of an agent already connected here only sets the default scope (no new connection, no reconnect)', async () => {
    const f = fixture({ list: () => ({ entries: [{ agent: 'claude', scope: 'project', file: '/work/app/.mcp.json', serverKey: 'airk', connectionId: first, status: 'ok' }] }),
        default: () => ({ schema: 1, connectionId: first, open: open(4, [], [prj(1)]) }) },
        () => ({ index_ids: [], project_ids: [prj(1)] }), [{ connectionId: first, client: 'claude', follow: false }]);
    const outcome = await f.host.connectAgentToScope('claude');
    assert.deepEqual(f.calls, [['agents', 'list', '--project', folder, '--no-user'], ['agents', 'default', '--connection-id', first, '--set', prj(1)]]);
    assert.equal(outcome.status, 'already_connected');
    assert.equal(outcome.connected, undefined);
    assert.deepEqual(f.store.values.get(OPEN_AGENTS_KEY), [{ connectionId: first, client: 'claude', follow: true }], 'Connect turns following on again');
});

test('U105 the choice switch sends --agent-choice on and off and nothing else; invalid input sends nothing', async () => {
    const f = fixture({ default: argv => ({ schema: 1, connectionId: first, open: open(3, [idx(5)], [], argv.at(-1) as 'off' | 'on') }) });
    assert.deepEqual(await f.host.setChoice(first, 'on'), { connectionId: first, choice: 'on', result: { schema: 1, connectionId: first, open: open(3, [idx(5)], [], 'on') } });
    await f.host.setChoice(first, 'off');
    assert.deepEqual(f.calls, [['agents', 'default', '--connection-id', first, '--agent-choice', 'on'], ['agents', 'default', '--connection-id', first, '--agent-choice', 'off']]);
    await assert.rejects(f.host.setChoice('nope', 'on'), (error: any) => error.code === 'INVALID_MESSAGE');
    await assert.rejects(f.host.setChoice(first, 'maybe' as any), (error: any) => error.code === 'INVALID_MESSAGE');
    assert.equal(f.calls.length, 2);
});

test('U105 following runs agents default for followed connections only, one at a time, and the newest scope wins', async () => {
    const gates: (() => void)[] = [];
    const f = fixture({ default: () => new Promise(resolve => { gates.push(() => resolve({ schema: 1 })); }) }, undefined,
        [{ connectionId: first, client: 'claude', follow: true }, { connectionId: second, client: 'codex', follow: false }]);
    const a = { index_ids: [idx(1)], project_ids: [] }, b = { index_ids: [idx(2)], project_ids: [] }, c = { index_ids: [], project_ids: [prj(1)] };
    const run = f.host.followScope(a, true);
    await Promise.resolve();
    void f.host.followScope(b, true); void f.host.followScope(c, true);
    for (let turn = 0; turn < 8 && f.calls.length < 2; turn++) {
        assert.equal(gates.length, f.calls.length, 'one agents default is in flight at a time');
        gates.shift()?.(); await new Promise(resolve => setImmediate(resolve));
    }
    gates.shift()?.(); await run; await f.host.settled();
    assert.deepEqual(f.calls, [
        ['agents', 'default', '--connection-id', first, '--index', idx(1)],
        ['agents', 'default', '--connection-id', first, '--set', prj(1)],
    ], 'the unfollowed connection is never changed and the superseded scope is skipped');
});

test('U105 a cleared scope clears followed defaults; an unchanged automatic scope is not sent again; a failure is reported, never thrown', async () => {
    let fail = false;
    const f = fixture({ default: () => { if (fail) { throw refused('agent_defaults_revision_changed', 'The defaults changed.'); } return { schema: 1 }; },
        list: () => ({ entries: [{ agent: 'claude', file: '/work/app/.mcp.json', serverKey: 'airk', connectionId: first, status: 'ok' }] }),
        info: () => ({ schema: 1, connectionId: first, open: open(5, [idx(4)], []) }) },
        () => ({ index_ids: [idx(4)], project_ids: [] }), [{ connectionId: first, client: 'claude', follow: true }]);
    const auto = { index_ids: [idx(4)], project_ids: [] };
    await f.host.followScope(auto, false);
    await f.host.followScope(auto, false);
    await f.host.followScope(null, true);
    assert.deepEqual(f.calls, [['agents', 'default', '--connection-id', first, '--index', idx(4)], ['agents', 'default', '--connection-id', first, '--clear']]);
    fail = true;
    await f.host.followScope(auto, true);
    assert.match(f.logs.at(-1)!, /did not receive this window's search scope \(agent_defaults_revision_changed\)/);
    const view = await f.host.read();
    assert.deepEqual(view.connections, [{ connectionId: first, remembered: true, follows: true,
        open: { ...open(5, [idx(4)], []), scope: [{ kind: 'index', id: idx(4) }] },
        info: { schema: 1, connectionId: first, open: open(5, [idx(4)], []) },
        followError: { code: 'agent_defaults_revision_changed', message: 'The defaults changed.' } }]);
    assert.deepEqual(view.windowScope, { index_ids: [idx(4)], project_ids: [], items: [{ kind: 'index', id: idx(4) }] });
});

test('U105 turning following on sends the current scope now; off only records it', async () => {
    const f = fixture({ default: () => ({ schema: 1 }), list: () => ({ entries: [{ agent: 'codex', file: '/work/app/.codex/config.toml', serverKey: 'airk', connectionId: second, status: 'ok' }] }) },
        () => ({ index_ids: [idx(5)], project_ids: [prj(1)] }));
    assert.deepEqual(await f.host.setFollow(second, true), { connectionId: second, follow: true, result: { schema: 1 } });
    await f.host.setFollow(second, false);
    assert.deepEqual(f.calls, [['agents', 'list', '--project', folder, '--no-user'], ['agents', 'default', '--connection-id', second, '--index', idx(5), '--set', prj(1)]]);
    assert.deepEqual(f.store.values.get(OPEN_AGENTS_KEY), [{ connectionId: second, client: 'codex', follow: false }]);
});

test('U105 Disconnect revokes an open connection no other entry names, keeps one another entry names, and only removes a grant-bound one', async () => {
    let others: unknown[] = [];
    const answers: Answers = { remove: () => ({ schema: 1, actions: [] }), list: () => ({ entries: others }), revoke: () => ({ schema: 1, revoked: true }),
        info: argv => argv.at(-1) === grantBound ? { schema: 1, connectionId: grantBound } : { schema: 1, open: open(1, [], []) } };
    const f = fixture(answers, undefined, [{ connectionId: first, client: 'claude', follow: true }]);
    assert.deepEqual(await f.host.disconnect(first), { removed: { schema: 1, actions: [] }, revoked: { schema: 1, revoked: true } });
    assert.deepEqual(f.calls, [['agents', 'remove', '--project', folder, '--no-user', '--connection-id', first], ['agents', 'list', '--project', folder], ['agents', 'revoke', '--connection-id', first]]);
    assert.deepEqual(f.store.values.get(OPEN_AGENTS_KEY), []);
    f.calls.length = 0; others = [{ agent: 'claude', file: '/home/.claude.json', serverKey: 'airk', connectionId: second }];
    const kept = await f.host.disconnect(second);
    assert.match(kept.kept!, /Another agent entry still names this connection/);
    assert.deepEqual(f.calls.map(argv => argv[1]), ['info', 'remove', 'list']);
    f.calls.length = 0;
    assert.deepEqual(await f.host.disconnect(grantBound), { removed: { schema: 1, actions: [] } });
    assert.deepEqual(f.calls.map(argv => argv[1]), ['info', 'remove']);
});
