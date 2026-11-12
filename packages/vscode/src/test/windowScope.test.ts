// SPDX-License-Identifier: Apache-2.0
// The window search scope (U77; contracts §2.5 W4, §3.7), unselected reads
// under it, and Explorer Assistance drafts and handoffs over the window
// session. Every session exchange is a recorded fake; expected frames are
// written out here, never derived from the code under test.
import { strict as assert } from 'node:assert';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { SCOPE_PAGE_BYTES, WINDOW_SCOPE_KEY, WindowScope, windowScopeKey, type ScopeSession } from '../airkctl/features/windowScope';
import { WindowAssistance, WindowHandoffs } from '../airkctl/features/windowAssistance';
import { AirkctlToolSession, readFailureCode, selectionArguments, ToolCallError } from '../airkctl/tools';
import { EvidenceClient } from '../explorerEvidence';
import { SuggestionStore } from '../explorerSuggestions';

const namespace = 'ab'.repeat(16);
const idx = (n: number) => 'idx_' + n.toString(16).padStart(2, '0').repeat(16);
const prj = (n: number) => 'prj_' + n.toString(16).padStart(2, '0').repeat(16);
const index = (n: number) => ({ kind: 'index', value: { id: idx(n), name: `Index ${n}`, tombstone: false, definition_revision: 1, source_revision: 1, backing: { root: `/src/${n}` } } });
const project = (n: number) => ({ kind: 'project', value: { id: prj(n), name: `Set ${n}`, definition_revision: 1, scope_revision: 1, defaults_revision: 1, default_indexes: [] } });
const atom = { source_root_id: 'src_' + '0f'.repeat(16), kind: 'subtree', path: '' };
const membership = (p: number, i: number, scope: unknown[] = [atom]) => ({ kind: 'membership', value: { project_id: prj(p), index_id: idx(i), revision: 1, scope } });
const cursor = (kind: string, revision: number, after: string, extra: Record<string, unknown> = {}) => ({ kind, namespace, revision, after, index_id: null, project_id: null, ...extra });
const page = (revision: number, items: unknown[], continuation: unknown = null) => ({ status: 'catalog_page', page: { namespace, revision, items, continuation } });

/** A scripted session: `control` answers by request kind (and `after`/`project_id`); window frames are recorded. */
function session(revision: () => number, outcomes: { select?: (selection: any) => any; release?: () => any } = {}) {
    const controls: any[] = [], selects: any[] = [], releases: number[] = [];
    const fake: ScopeSession = {
        control: async (method: any) => {
            controls.push(structuredClone(method));
            const request = method.catalog_page.request, r = revision();
            if (request.expected_catalog_revision !== null && request.expected_catalog_revision !== r) { return { status: 'failure', code: 'catalog_revision_changed' }; }
            if (request.kind === 'indexes') { return request.after ? page(r, [index(2)]) : page(r, [index(1)], cursor('indexes', r, idx(1))); }
            if (request.kind === 'projects') { return page(r, [project(1), project(2)]); }
            if (request.kind === 'memberships') {
                // An empty-scope membership grants no read authority and is left out of the derived indexes.
                if (request.project_id === prj(1)) { return request.after ? page(r, [membership(1, 2), membership(1, 3, [])]) : page(r, [membership(1, 1)], cursor('memberships', r, idx(1), { project_id: prj(1) })); }
                return page(r, []);
            }
            throw new Error('unexpected control');
        },
        scopeSelect: async selection => { selects.push(structuredClone(selection)); return outcomes.select?.(selection) ?? { status: 'scope', scope_identity: 'cd'.repeat(32), catalog_revision: selection.expected_catalog_revision, expires_at_ms: 99 }; },
        scopeRelease: async () => { releases.push(1); return outcomes.release?.() ?? { status: 'scope_released' }; },
    };
    return { fake, controls, selects, releases };
}
function memento(initial: Record<string, unknown> = {}) {
    const values = new Map(Object.entries(initial));
    return { values, get: <T>(key: string) => values.get(key) as T | undefined, update: async (key: string, value: unknown) => { if (value === undefined) { values.delete(key); } else { values.set(key, value); } } };
}
const ready = () => ['session/2', 'tools/1', 'window-scope/1'];
const base = () => ({ command: '/install/airkctl', installDir: '/install', root: '/src/demo' });

test('scope choices page both kinds at one catalog revision and continue the index page at that revision', async () => {
    const s = session(() => 7), scope = new WindowScope(s.fake, ready, memento(), base);
    const choices = await scope.readSearchScope();
    assert.deepEqual(s.controls, [
        { catalog_page: { request: { kind: 'indexes', index_id: null, project_id: null, after: null, expected_catalog_revision: null, limit: 16, max_bytes: SCOPE_PAGE_BYTES } } },
        { catalog_page: { request: { kind: 'projects', index_id: null, project_id: null, after: null, expected_catalog_revision: 7, limit: 16, max_bytes: 49152 } } },
    ]);
    assert.deepEqual(choices.indexes.map(item => item.id), [idx(1)]);
    assert.deepEqual(choices.projects.map(item => item.id), [prj(1), prj(2)]);
    assert.equal(choices.revision, 7); assert.equal(choices.namespace, namespace); assert.equal(choices.active, false);
    await scope.readSearchScope('indexes', true);
    assert.deepEqual(s.controls[2], { catalog_page: { request: { kind: 'indexes', index_id: null, project_id: null, after: cursor('indexes', 7, idx(1)), expected_catalog_revision: 7, limit: 16, max_bytes: 49152 } } });
    assert.deepEqual(scope.searchScopeChoices()!.indexes.map(item => item.id), [idx(1), idx(2)]);
    assert.equal(scope.searchScopeChoices()!.responses.length, 3, 'every complete native page is retained');
    await assert.rejects(scope.readSearchScope('projects', true), (error: any) => error.code === 'catalog_continuation_unavailable');
});

test('apply sends exactly one scope_select frame with the set members at the read revision, persists it and clear releases', async () => {
    const store = memento(), s = session(() => 7), scope = new WindowScope(s.fake, ready, store, base);
    let changes = 0; scope.onDidChange(() => { changes++; });
    await scope.readSearchScope();
    const outcome = await scope.applySearchScope({ index_ids: [], project_ids: [prj(1)] });
    assert.deepEqual(s.selects, [{ namespace, expected_catalog_revision: 7, index_ids: [idx(1), idx(2)], project_ids: [prj(1)] }]);
    assert.deepEqual(s.controls.slice(2).map(method => [method.catalog_page.request.kind, method.catalog_page.request.project_id, method.catalog_page.request.expected_catalog_revision]),
        [['memberships', prj(1), 7], ['memberships', prj(1), 7]], 'memberships are paged at the read revision');
    assert.equal(outcome.status, 'scope');
    assert.equal(scope.active, true);
    assert.deepEqual(scope.searchScopeSummary(), { available: true, active: true, outOfDate: false, remembered: true, label: 'Set 1',
        indexIds: [idx(1), idx(2)], projectIds: [prj(1)], scopeIdentity: 'cd'.repeat(32), catalogRevision: 7 });
    assert.deepEqual(store.values.get(WINDOW_SCOPE_KEY), { namespace, project_ids: [prj(1)], index_ids: [idx(1), idx(2)], explicit: false });
    const spec = scope.spec()!;
    assert.equal(spec.windowScope!.key, windowScopeKey({ namespace, project_ids: [prj(1)], index_ids: [idx(2), idx(1)] }), 'the key ignores order');
    assert.match(spec.service!.project, /^[0-9a-f]{64}$/);
    // Narrowing to an index outside the chosen sets is refused before any frame.
    await assert.rejects(scope.applySearchScope({ index_ids: [idx(9)], project_ids: [prj(1)] }), (error: any) => error.code === 'window_scope_index_outside_sets');
    await assert.rejects(scope.applySearchScope({ index_ids: [idx(1)], project_ids: [] }), (error: any) => error.code === 'window_scope_project_required');
    assert.equal(s.selects.length, 1);
    await scope.applySearchScope({ index_ids: [], project_ids: [] });
    assert.deepEqual(s.releases, [1]); assert.equal(scope.active, false);
    assert.equal(store.values.has(WINDOW_SCOPE_KEY), false, 'clearing forgets the remembered choice');
    assert.ok(changes >= 3);
});

test('a refused grant keeps its code; scoped_read_unavailable marks an active scope out of date', async () => {
    let refuse = false;
    const s = session(() => 7, { select: () => refuse ? { status: 'refused', code: 'scoped_read_unavailable', retryable: false } : undefined });
    const scope = new WindowScope(s.fake, ready, memento(), base);
    await scope.readSearchScope(); await scope.applySearchScope({ project_ids: [prj(1)] });
    refuse = true;
    await assert.rejects(scope.applySearchScope({ project_ids: [prj(1)], index_ids: [idx(1)] }), (error: any) => error.code === 'scoped_read_unavailable' && /out of date — reselect/.test(error.message));
    assert.equal(scope.active, true, 'a refused grant leaves the previous one'); assert.equal(scope.searchScopeSummary().outOfDate, true);
});

test('a refused window read (Q4 already re-selected once) marks the scope out of date; other codes do not', async () => {
    const s = session(() => 7), scope = new WindowScope(s.fake, ready, memento(), base);
    await scope.readSearchScope(); await scope.applySearchScope({ project_ids: [prj(1)] });
    scope.refused('not_yet_served'); assert.equal(scope.searchScopeSummary().outOfDate, false);
    scope.refused('scoped_read_unavailable'); assert.equal(scope.searchScopeSummary().outOfDate, true);
    assert.equal(scope.searchScopeChoices()!.outOfDate, true);
});

test('without window-scope/1 nothing is read, granted or shown', async () => {
    const s = session(() => 7), scope = new WindowScope(s.fake, () => ['session/2', 'tools/1'], memento({ [WINDOW_SCOPE_KEY]: { namespace, project_ids: [prj(1)], index_ids: [idx(1)], explicit: true } }), base);
    await assert.rejects(scope.readSearchScope(), (error: any) => error.code === 'window_scope_unavailable');
    await assert.rejects(scope.applySearchScope({ project_ids: [prj(1)] }), (error: any) => error.code === 'window_scope_unavailable');
    await scope.restore();
    assert.deepEqual([s.controls.length, s.selects.length], [0, 0]);
    assert.equal(scope.searchScopeSummary().available, false); assert.equal(scope.spec(), undefined);
});

test('a remembered choice is applied again only after the catalog is read at its current revision', async () => {
    let revision = 7;
    const store = memento(), s = session(() => revision), scope = new WindowScope(s.fake, ready, store, base);
    await scope.readSearchScope(); await scope.applySearchScope({ project_ids: [prj(1)], index_ids: [idx(2)] });
    scope.lost(); assert.equal(scope.active, false);
    revision = 9; s.controls.length = 0;
    await scope.restore();
    assert.deepEqual(s.controls.slice(0, 2).map(method => [method.catalog_page.request.kind, method.catalog_page.request.expected_catalog_revision]), [['indexes', null], ['projects', 9]],
        'the catalog is read again before the grant');
    assert.deepEqual(s.selects.at(-1), { namespace, expected_catalog_revision: 9, index_ids: [idx(2)], project_ids: [prj(1)] }, 'the current revision, never the stale one');
    assert.equal(scope.active, true);
    // A remembered index no longer in its set is not applied; the user reselects.
    const stale = memento({ [WINDOW_SCOPE_KEY]: { namespace, project_ids: [prj(1)], index_ids: [idx(5)], explicit: true } }), s2 = session(() => 3);
    const other = new WindowScope(s2.fake, ready, stale, base, () => {});
    await other.restore();
    assert.equal(s2.selects.length, 0); assert.equal(other.active, false); assert.equal(other.searchScopeSummary().outOfDate, true);
});

test('reads under the window scope name no selection and report scoped_read_unavailable; project reads name the project', async () => {
    assert.deepEqual(selectionArguments({ window: true, project: 'ab'.repeat(32) }), []);
    const seen: string[][] = [], failed: string[] = [];
    let window = true;
    const refused = { exitCode: 1, class: 'refused' as const, stdout: '', stderr: '', error: { code: 'scoped_read_unavailable', message: 'The catalog changed.', retryable: false } };
    let answer: any = { exitCode: 0, class: 'success' as const, stdout: '{"content":[{"type":"text","text":"{}"}]}', stderr: '' };
    const fake = { closed: false, invoke: async (argv: string[]) => { seen.push(argv); return answer; } };
    const tools = new AirkctlToolSession(fake as never, () => window ? { window: true } : { project: 'ab'.repeat(32) }, { windowReadFailed: code => failed.push(code) });
    await tools.request('tools/call', { name: 'airk_search', arguments: { query: 'x' } }, 1000);
    assert.deepEqual(seen[0], ['tools', 'call', 'airk_search', '--args', '{"query":"x"}', '--output', 'json']);
    window = false;
    await tools.request('tools/call', { name: 'airk_search', arguments: { query: 'x' } }, 1000);
    assert.deepEqual(seen[1], ['tools', 'call', 'airk_search', '--args', '{"query":"x"}', '--service-project', 'ab'.repeat(32), '--output', 'json'], 'the selection is chosen per call');
    window = true; answer = refused;
    await assert.rejects(tools.request('tools/call', { name: 'airk_search', arguments: {} }, 1000));
    assert.deepEqual(failed, ['scoped_read_unavailable']);
    answer = { exitCode: 0, class: 'success', stdout: JSON.stringify({ isError: true, content: [{ type: 'text', text: JSON.stringify({ error: { code: 'window_scope_not_granted' } }) }] }), stderr: '' };
    await tools.request('tools/call', { name: 'airk_search', arguments: {} }, 1000);
    assert.deepEqual(failed, ['scoped_read_unavailable', 'window_scope_not_granted'], 'an error envelope on a success exit is seen too');
    assert.equal(readFailureCode({ exitCode: 1, class: 'refused', stdout: '{"error":{"code":"not_yet_served"}}', stderr: '' }), 'not_yet_served');
});

test('a scoped_read_unavailable read says the search scope is out of date with its native code', async () => {
    const spec = { command: '/install/airkctl', root: '/src', dataDir: '', collection: 'window-scope', name: 'scope', service: { installDir: '/install', project: 'ef'.repeat(32) }, windowScope: { key: 'ef'.repeat(32), label: 'Set 1' } };
    for (const transport of [
        { closed: false, close() {}, request: async () => { throw new ToolCallError('rpc', 'scoped_read_unavailable: changed', { exitCode: 1, class: 'refused', stdout: '', stderr: '', error: { airkctl_error: 1, class: 'refused', code: 'scoped_read_unavailable', message: 'changed', retryable: false } }); } },
        { closed: false, close() {}, request: async () => ({ isError: true, content: [{ type: 'text', text: JSON.stringify({ error: { code: 'scoped_read_unavailable', message: 'changed' } }) }] }) },
    ]) {
        const client = new EvidenceClient(spec, '1.0.0', { transport: () => transport, retryBudget: 0 });
        await assert.rejects(client.call('airk_search', { query: 'x' }), (error: any) => error.error.code === 'scoped_read_unavailable' && /Search scope out of date — reselect/.test(error.message));
        client.dispose();
    }
    const unserved = new EvidenceClient(spec, '1.0.0', { transport: () => ({ closed: false, close() {}, request: async () => ({ isError: true, content: [{ type: 'text', text: '{"error":{"code":"not_yet_served"}}' }] }) }), retryBudget: 0 });
    await assert.rejects(unserved.call('airk_search', { query: 'x' }), (error: any) => error.error.code === 'not_yet_served', 'a multi-index refusal is shown as an ordinary refusal with its code');
    unserved.dispose();
});

test('assistance drafts are subscribed only with assistance/1, fed to the Inbox and acknowledged through their sequence', async () => {
    const frames: unknown[] = [];
    const fake = { subscribe: async (topics: string[]) => { frames.push({ subscribe: { topics } }); return { status: 'subscribed', cursor: {} } as any; },
        assistanceAck: async (through: number) => { frames.push({ assistance_ack: { through } }); return { status: 'acknowledged' } as any; } };
    const store = new SuggestionStore({ binding: () => 'b', revisions: () => ({}), publication: () => undefined, ttlMs: () => 3_600_000 });
    let capabilities = ['session/2'];
    const assistance = new WindowAssistance(fake, () => capabilities, store);
    await assistance.start(); assert.deepEqual(frames, [], 'no subscription without assistance/1');
    capabilities = ['session/2', 'assistance/1'];
    await assistance.start(); await assistance.start();
    assert.deepEqual(frames, [{ subscribe: { topics: ['assistance'] } }], 'one subscription per session child');
    const draft = (sequence: number, key: string) => ({ kind: 'assistance_draft', cursor: {} as any, draft: { sequence, draft_id: 'aa'.repeat(16), kind: 'search',
        draft: { schema_version: '1.0', idempotency_key: key, title: 'Find it', draft: { query: 'original' } } } }) as any;
    await assistance.event(draft(1, 'first'));
    await assistance.event(draft(1, 'first'));
    await assistance.event(draft(2, 'second'));
    await assistance.event({ ...draft(3, 'bad'), draft: { sequence: 3, draft_id: 'bb'.repeat(16), kind: 'unknown', draft: {} } });
    assert.deepEqual(store.list().map(item => item.draft.idempotencyKey).sort(), ['first', 'second'], 'each draft reaches the Inbox once');
    assert.deepEqual(frames.slice(1), [{ assistance_ack: { through: 1 } }, { assistance_ack: { through: 2 } }, { assistance_ack: { through: 3 } }],
        'a repeat is not acknowledged twice; an unsupported draft is acknowledged so the queue moves');
    await assistance.event({ kind: 'session_lost', code: 'session_closed' });
    await assistance.start(); assert.deepEqual(frames.at(-1), { subscribe: { topics: ['assistance'] } }, 'a new session child subscribes again');
});

test('a handoff is published through the window with only the reviewed text and its identity', async () => {
    const frames: unknown[] = [];
    const handoffs = new WindowHandoffs({ handoffPublish: async (handoff: unknown, ttl: number) => { frames.push({ handoff_publish: { handoff, ttl_ms: ttl } }); return { status: 'handoff', handoff_id: '12'.repeat(16), expires_at_ms: 600 } as any; } }, () => 'file:///src/demo');
    const value = await handoffs.share('studio', 3, 'Complete reviewed instructions', 'airk-r1:t');
    assert.deepEqual(frames, [{ handoff_publish: { handoff: { kind: 'studio', draft_revision: 3, content: 'Complete reviewed instructions',
        content_sha256: createHash('sha256').update('Complete reviewed instructions').digest('hex'), evidence_publication: 'airk-r1:t' }, ttl_ms: 600000 } }]);
    assert.equal(value.token, '12'.repeat(16)); assert.equal(value.expires_unix_ms, 600);
    assert.doesNotMatch(JSON.stringify(frames), /file:\/\//, 'no workspace path leaves the editor');
    const refused = new WindowHandoffs({ handoffPublish: async () => ({ status: 'refused', code: 'handoff_capacity', retryable: true }) as any }, () => '');
    await assert.rejects(refused.share('planner', 1, 'text'), (error: any) => error.code === 'handoff_capacity');
});

test('Recommendations and agent search drafts select their scope after reading the catalog at its current revision', async () => {
    let revision = 4;
    const s = session(() => revision), scope = new WindowScope(s.fake, ready, memento(), base);
    await scope.readSearchScope(); revision = 5;
    await scope.selectIds([prj(1)], [idx(1)]);
    assert.deepEqual(s.controls.slice(2, 4).map(method => [method.catalog_page.request.kind, method.catalog_page.request.expected_catalog_revision]), [['indexes', null], ['projects', 5]]);
    assert.deepEqual(s.selects, [{ namespace, expected_catalog_revision: 5, index_ids: [idx(1)], project_ids: [prj(1)] }]);
    await assert.rejects(scope.selectIds(undefined, [idx(1)]), (error: any) => error.code === 'window_scope_project_required');
    assert.equal(s.selects.length, 1);
});
