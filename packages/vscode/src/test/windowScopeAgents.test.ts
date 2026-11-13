// SPDX-License-Identifier: Apache-2.0
// U105: the window search scope as the default scope of open agent
// connections, the follow hook after user and automatic grants, and
// descriptions (`library describe` argv, bounds, catalog descriptions). Every
// session exchange is a recorded fake; expected frames and argv are written
// out here, never derived from the code under test.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { describeArgv, libraryCommands, WindowScope, type AgentScopeChange, type ScopeSession } from '../airkctl/features/windowScope';
import { OPEN_AGENTS_KEY, WindowAgents } from '../airkctl/features/windowAgents';

const namespace = 'ab'.repeat(16);
const idx = (n: number) => 'idx_' + n.toString(16).padStart(2, '0').repeat(16);
const prj = (n: number) => 'prj_' + n.toString(16).padStart(2, '0').repeat(16);
const workspace = 'be'.repeat(32);
const atom = { source_root_id: 'src_' + '0f'.repeat(16), kind: 'subtree', path: '' };
const page = (revision: number, items: unknown[]) => ({ status: 'catalog_page', page: { namespace, revision, items, continuation: null } });
const index = (n: number, extra: Record<string, unknown> = {}) => ({ kind: 'index', value: { id: idx(n), name: `Index ${n}`, tombstone: false, definition_revision: 1, source_revision: 1, backing: { root: `/src/${n}` }, ...extra } });
function memento(initial: Record<string, unknown> = {}) {
    const values = new Map(Object.entries(initial));
    return { values, get: <T>(key: string) => values.get(key) as T | undefined, update: async (key: string, value: unknown) => { if (value === undefined) { values.delete(key); } else { values.set(key, value); } } };
}
const ready = () => ['session/2', 'tools/1', 'window-scope/1'];
const base = () => ({ command: '/install/airkctl', installDir: '/install', root: '/src/demo' });

/** Set 1 holds idx 1 and 2 and carries a description; set 2 holds idx 4 (the workspace index) and 5. Each description write bumps the revision. */
function session() {
    const events: string[] = [];
    let revision = 7;
    const holds: Record<string, number[]> = { [prj(1)]: [1, 2], [prj(2)]: [4, 5] };
    const fake: ScopeSession = {
        control: async (method: any) => {
            const request = method.catalog_page.request, r = revision;
            events.push(`page:${request.kind}:${r}`);
            if (request.expected_catalog_revision !== null && request.expected_catalog_revision !== r) { return { status: 'failure', code: 'catalog_revision_changed' }; }
            if (request.kind === 'indexes') { return page(r, [index(1, { description: 'Line one\nLine <two>' }), index(2), index(4, { backing: { binding: workspace, root: '/src/w' } }), index(5)]); }
            if (request.kind === 'projects') {
                return page(r, Object.keys(holds).map(id => ({ kind: 'project', value: { id, name: `Set ${parseInt(id.slice(4, 6), 16)}`, definition_revision: 1, scope_revision: 1, defaults_revision: 1, default_indexes: [],
                    ...(id === prj(1) ? { description: 'Application services' } : {}) } })));
            }
            if (request.kind === 'memberships') {
                return page(r, Object.entries(holds).flatMap(([set, list]) => list.map(n => ({ set, n })))
                    .filter(row => (request.project_id === null || row.set === request.project_id) && (request.index_id === null || idx(row.n) === request.index_id))
                    .map(row => ({ kind: 'membership', value: { project_id: row.set, index_id: idx(row.n), revision: 1, scope: [atom] } })));
            }
            throw new Error('unexpected control');
        },
        scopeSelect: async selection => ({ status: 'scope', scope_identity: 'cd'.repeat(32), catalog_revision: selection.expected_catalog_revision, expires_at_ms: 99 }) as any,
        scopeRelease: async () => ({ status: 'scope_released' }) as any,
    };
    const described: unknown[][] = [];
    const library = {
        createSet: async () => ({}),
        describe: async (kind: string, id: string, text: string | undefined) => { described.push([kind, id, text]); revision++; return { argv: describeArgv(kind as 'index', id, text), class: 'success', lines: [{ status: 'catalog_applied' }] }; },
    };
    return { fake, library, events, described };
}

test('U105 the agent default scope is the checked union, the narrowed indexes of an exact pick, or the automatic index; each grant and a user clear notify', async () => {
    const s = session(), scope = new WindowScope(s.fake, ready, memento(), base, () => {}, s.library);
    const changes: AgentScopeChange[] = [];
    scope.onDidChangeAgentScope(change => changes.push(structuredClone(change)));
    assert.equal(scope.agentDefaultScope(), undefined);
    await scope.ensureDefault(workspace);
    assert.deepEqual(scope.agentDefaultScope(), { index_ids: [idx(4)], project_ids: [] }, 'automatic: the window\'s index');
    await scope.readSearchScope();
    await scope.applySearchScope({ project_ids: [prj(1)], index_ids: [idx(5)] });
    assert.deepEqual(scope.agentDefaultScope(), { index_ids: [idx(5)], project_ids: [prj(1)] }, 'union: the checked sets and indexes');
    await scope.selectIds([prj(2)], [idx(5)]);
    assert.deepEqual(scope.agentDefaultScope(), { index_ids: [idx(5)], project_ids: [] }, 'exact: indexes narrowing a set are sent as indexes');
    await scope.selectIds([prj(2)], []);
    assert.deepEqual(scope.agentDefaultScope(), { index_ids: [], project_ids: [prj(2)] });
    await scope.applySearchScope({ project_ids: [], index_ids: [] });
    assert.equal(scope.agentDefaultScope(), undefined);
    assert.deepEqual(changes, [
        { scope: { index_ids: [idx(4)], project_ids: [] }, user: false },
        { scope: { index_ids: [idx(5)], project_ids: [prj(1)] }, user: true },
        { scope: { index_ids: [idx(5)], project_ids: [] }, user: true },
        { scope: { index_ids: [], project_ids: [prj(2)] }, user: true },
        { scope: null, user: true },
    ]);
});

test('U105 applying a scope runs agents default for the followed open connections of this workspace only; a refusal does not fail the apply', async () => {
    const s = session(), store = memento({ [OPEN_AGENTS_KEY]: [
        { connectionId: '1a'.repeat(16), client: 'claude', follow: true }, { connectionId: '2b'.repeat(16), client: 'codex', follow: false }] });
    const scope = new WindowScope(s.fake, ready, store, base, () => {}, s.library);
    const calls: string[][] = [];
    const agents = new WindowAgents({ session: { json: async argv => { calls.push([...argv]); throw Object.assign(new Error('Unknown connection.'), { code: 'agent_connection_unknown' }); } } },
        () => '/work/app', () => {}, () => {}, { store, scope });
    scope.onDidChangeAgentScope(change => { void agents.followScope(change.scope, change.user); });
    await scope.readSearchScope();
    const outcome = await scope.applySearchScope({ project_ids: [prj(1)], index_ids: [] });
    assert.equal(outcome.status, 'scope', 'the apply succeeds although the follow was refused');
    await agents.settled();
    assert.deepEqual(calls, [['agents', 'default', '--connection-id', '1a'.repeat(16), '--set', prj(1)]]);
});

test('U105 descriptions: library describe argv, bounds, and the catalog descriptions read again afterwards', async () => {
    assert.deepEqual(describeArgv('index', idx(1), 'Line one\nLine two'), ['library', 'describe', '--index', idx(1), '--text=Line one\nLine two', '--yes']);
    // One token, so a description starting with "-" is never read as an option.
    assert.deepEqual(describeArgv('index', idx(1), '- bullet'), ['library', 'describe', '--index', idx(1), '--text=- bullet', '--yes']);
    assert.deepEqual(describeArgv('set', prj(1), undefined), ['library', 'describe', '--set', prj(1), '--clear', '--yes']);
    const invoked: unknown[] = [];
    const commands = libraryCommands({ invoke: async (argv, options) => { invoked.push([argv, options]);
        return argv.includes('--clear') ? { exitCode: 2, class: 'refused', stdout: '', stderr: '', error: { code: 'catalog_revision_changed', message: 'Changed.' } }
            : { exitCode: 0, class: 'success', stdout: '{"status":"catalog_applied"}\n', stderr: '' }; } });
    assert.deepEqual(await commands.describe!('index', idx(1), 'Text'), { argv: ['library', 'describe', '--index', idx(1), '--text=Text', '--yes'], class: 'success', exitCode: 0, lines: [{ status: 'catalog_applied' }] });
    await assert.rejects(commands.describe!('set', prj(1), undefined), (error: any) => error.code === 'catalog_revision_changed' && /not cleared/.test(error.message));
    assert.deepEqual(invoked.map(item => (item as unknown[])[1]), [{ deadlineMs: 60_000 }, { deadlineMs: 60_000 }]);

    const s = session(), scope = new WindowScope(s.fake, ready, memento(), base, () => {}, s.library);
    await scope.readSearchScope();
    assert.deepEqual(scope.choiceInfo('index', idx(1)), { name: 'Index 1', description: 'Line one\nLine <two>' });
    assert.deepEqual(scope.choiceInfo('set', prj(1)), { name: 'Set 1', description: 'Application services' });
    assert.deepEqual(scope.choiceInfo('index', idx(2)), { name: 'Index 2' });
    for (const [kind, id, text, code] of [['index', idx(1), 'x'.repeat(1025), 'description_invalid'], ['index', idx(1), 'tab\there', 'description_invalid'], ['index', idx(1), '   ', 'description_invalid'],
        ['set', idx(1), 'Text', 'description_target_invalid'], ['grant', idx(1), 'Text', 'description_target_invalid']] as const) {
        await assert.rejects(scope.describe(kind, id, text), (error: any) => error.code === code, `${kind} ${text.slice(0, 8)}`);
    }
    assert.equal(s.described.length, 0, 'invalid descriptions are never sent');
    s.events.length = 0;
    await scope.describe('index', idx(1), 'é'.repeat(512));
    await scope.describe('set', prj(1), undefined);
    assert.deepEqual(s.described, [['index', idx(1), 'é'.repeat(512)], ['set', prj(1), undefined]]);
    assert.deepEqual(s.events.slice(0, 2), ['page:indexes:8', 'page:projects:8'], 'the choices are read again at the new revision');
});
