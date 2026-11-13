// SPDX-License-Identifier: Apache-2.0
// U104: the window search scope as a union (checked index sets plus checked
// indexes), an index set created for a set-less index before Apply grants it,
// Save as index set, `library set-create` argv, and the automatic-scope flag.
// Every session exchange is a recorded fake; expected frames are written out
// here, never derived from the code under test.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { libraryCommands, WINDOW_SCOPE_KEY, WindowScope, type ScopeSession } from '../airkctl/features/windowScope';

const namespace = 'ab'.repeat(16);
const idx = (n: number) => 'idx_' + n.toString(16).padStart(2, '0').repeat(16);
const prj = (n: number) => 'prj_' + n.toString(16).padStart(2, '0').repeat(16);
const index = (n: number, extra: Record<string, unknown> = {}) => ({ kind: 'index', value: { id: idx(n), name: `Index ${n}`, tombstone: false, definition_revision: 1, source_revision: 1, backing: { root: `/src/${n}` }, ...extra } });
const atom = { source_root_id: 'src_' + '0f'.repeat(16), kind: 'subtree', path: '' };
const membership = (p: number, i: number) => ({ kind: 'membership', value: { project_id: prj(p), index_id: idx(i), revision: 1, scope: [atom] } });
const page = (revision: number, items: unknown[]) => ({ status: 'catalog_page', page: { namespace, revision, items, continuation: null } });
function memento(initial: Record<string, unknown> = {}) {
    const values = new Map(Object.entries(initial));
    return { values, get: <T>(key: string) => values.get(key) as T | undefined, update: async (key: string, value: unknown) => { if (value === undefined) { values.delete(key); } else { values.set(key, value); } } };
}
const ready = () => ['session/2', 'tools/1', 'window-scope/1'];
const base = () => ({ command: '/install/airkctl', installDir: '/install', root: '/src/demo' });
const workspace = 'be'.repeat(32);

/** Set 1 holds idx 1 and 2, set 2 holds idx 5; idx 6 is in no set until `createSet` gives it set 9.
 * idx 4 is the workspace index (binding `workspace`), held by set 2 as well. Each created set bumps the revision. */
function unionSession() {
    const events: string[] = [], selects: any[] = [], created: [string, string[]][] = [];
    let revision = 7;
    const holds: Record<string, number[]> = { [prj(1)]: [1, 2], [prj(2)]: [4, 5] };
    const setName = (id: string) => id === prj(9) ? 'Index 6' : `Set ${parseInt(id.slice(4, 6), 16)}`;
    const fake: ScopeSession = {
        control: async (method: any) => {
            const request = method.catalog_page.request, r = revision;
            events.push(`page:${request.kind}:${r}`);
            if (request.expected_catalog_revision !== null && request.expected_catalog_revision !== r) { return { status: 'failure', code: 'catalog_revision_changed' }; }
            if (request.kind === 'indexes') { return page(r, [index(1), index(2), index(4, { backing: { binding: workspace, root: '/src/w' } }), index(5), index(6)]); }
            if (request.kind === 'projects') { return page(r, Object.keys(holds).map(id => ({ kind: 'project', value: { id, name: setName(id), definition_revision: 1, scope_revision: 1, defaults_revision: 1, default_indexes: [] } }))); }
            if (request.kind === 'memberships') {
                return page(r, Object.entries(holds).flatMap(([set, list]) => list.map(n => ({ set, n })))
                    .filter(row => (request.project_id === null || row.set === request.project_id) && (request.index_id === null || idx(row.n) === request.index_id))
                    .map(row => membership(parseInt(row.set.slice(4, 6), 16), row.n)));
            }
            throw new Error('unexpected control');
        },
        scopeSelect: async selection => { events.push('select'); selects.push(structuredClone(selection)); return { status: 'scope', scope_identity: 'cd'.repeat(32), catalog_revision: selection.expected_catalog_revision, expires_at_ms: 99 } as any; },
        scopeRelease: async () => ({ status: 'scope_released' }) as any,
    };
    const library = { createSet: async (name: string, ids: readonly string[]) => {
        events.push('createSet'); created.push([name, [...ids]]);
        if (ids.length === 1 && ids[0] === idx(6)) { holds[prj(9)] = [6]; } else { holds[prj(8)] = ids.map(id => parseInt(id.slice(4, 6), 16)); }
        revision++;
        return { argv: ['library', 'set-create'], class: 'success', lines: [{ status: 'catalog_applied' }] };
    } };
    return { fake, library, events, selects, created };
}

test('U104 a checked index outside the checked sets is added with the sets that hold it; the members are granted with it', async () => {
    const s = unionSession(), store = memento(), scope = new WindowScope(s.fake, ready, store, base, () => {}, s.library);
    await scope.readSearchScope();
    await scope.applySearchScope({ project_ids: [prj(1)], index_ids: [idx(5)] });
    assert.deepEqual(s.selects, [{ namespace, expected_catalog_revision: 7, index_ids: [idx(1), idx(2), idx(5)], project_ids: [prj(1), prj(2)] }]);
    assert.equal(scope.searchScopeSummary().label, 'Set 1 + Index 5');
    assert.deepEqual(scope.searchScopeSelection(), { index_ids: [idx(5)], project_ids: [prj(1)], active: true }, 'the page shows what was checked');
    assert.deepEqual(store.values.get(WINDOW_SCOPE_KEY), { namespace, project_ids: [prj(1)], index_ids: [idx(5)], explicit: true, union: true });
    // A checked member of a checked set adds nothing: the whole set is granted.
    await scope.applySearchScope({ project_ids: [prj(1)], index_ids: [idx(1)] });
    assert.deepEqual(s.selects.at(-1), { namespace, expected_catalog_revision: 7, index_ids: [idx(1), idx(2)], project_ids: [prj(1)] });
    assert.equal(s.created.length, 0, 'no set is created when every checked index has one');
    // The union is applied again as a union after the grant is lost.
    await scope.applySearchScope({ project_ids: [prj(1)], index_ids: [idx(5)] });
    scope.lost(); await scope.restore();
    assert.deepEqual(s.selects.at(-1), { namespace, expected_catalog_revision: 7, index_ids: [idx(1), idx(2), idx(5)], project_ids: [prj(1), prj(2)] });
});

test('U104 applying an index in no index set first creates a set named after it, then grants at the new revision', async () => {
    const s = unionSession(), scope = new WindowScope(s.fake, ready, memento(), base, () => {}, s.library);
    await scope.readSearchScope(); s.events.length = 0;
    await scope.applySearchScope({ project_ids: [prj(1)], index_ids: [idx(6)] });
    assert.deepEqual(s.created, [['Index 6', [idx(6)]]]);
    const createdAt = s.events.indexOf('createSet'), selectAt = s.events.indexOf('select');
    assert.ok(createdAt >= 0 && selectAt > createdAt, 'the set is created before the grant');
    assert.ok(s.events.slice(createdAt).includes('page:indexes:8'), 'the choices are read again after creating the set');
    assert.deepEqual(s.selects, [{ namespace, expected_catalog_revision: 8, index_ids: [idx(1), idx(2), idx(6)], project_ids: [prj(1), prj(9)] }]);
    assert.deepEqual(scope.searchScopeChoices()!.indexSets![idx(6)], [prj(9)], 'the memberships are read again too');
});

test('U104 a remembered union never creates an index set on restore', async () => {
    const s = unionSession();
    const scope = new WindowScope(s.fake, ready, memento({ [WINDOW_SCOPE_KEY]: { namespace, project_ids: [], index_ids: [idx(6)], explicit: true, union: true } }), base, () => {}, s.library);
    await scope.restore();
    assert.deepEqual([s.created.length, s.selects.length], [0, 0]);
    assert.equal(scope.searchScopeSummary().outOfDate, true);
});

test('U104 Save as index set creates one set from the checked indexes and the members of the checked sets', async () => {
    const s = unionSession(), scope = new WindowScope(s.fake, ready, memento(), base, () => {}, s.library);
    await scope.readSearchScope();
    await assert.rejects(scope.saveIndexSet('   ', { project_ids: [prj(1)], index_ids: [] }), (error: any) => error.code === 'index_set_name_required');
    await assert.rejects(scope.saveIndexSet('Empty', { project_ids: [], index_ids: [] }), (error: any) => error.code === 'window_scope_empty');
    assert.equal(s.created.length, 0);
    const outcome = await scope.saveIndexSet('  Review set ', { project_ids: [prj(1)], index_ids: [idx(5)] });
    assert.deepEqual(s.created, [['Review set', [idx(5), idx(1), idx(2)]]]);
    assert.equal(outcome.class, 'success');
    assert.equal(scope.searchScopeChoices()!.revision, 8, 'the choices are read again at the new revision');
    assert.ok(scope.searchScopeChoices()!.projects.some(item => item.id === prj(8)));
    assert.equal(s.selects.length, 0, 'saving a set does not change the window scope');
    const without = new WindowScope(unionSession().fake, ready, memento(), base);
    await without.readSearchScope();
    await assert.rejects(without.saveIndexSet('X', { index_ids: [idx(1)] }), (error: any) => error.code === 'index_set_create_unavailable');
});

test('U104 library set-create runs the exact argv and keeps every printed line; a refusal keeps its code', async () => {
    const calls: unknown[][] = [];
    let answer: any = { exitCode: 0, class: 'success', stdout: '{"status":"catalog_preview"}\n{"status":"catalog_applied","revision":8}\n', stderr: '' };
    const library = libraryCommands({ invoke: async (argv, options) => { calls.push([argv, options]); return answer; } });
    const outcome = await library.createSet('Index 6', [idx(6), idx(5)]);
    assert.deepEqual(calls, [[['library', 'set-create', '--name', 'Index 6', '--index', idx(6), '--index', idx(5), '--yes'], { deadlineMs: 60000 }]]);
    assert.deepEqual(outcome.lines, [{ status: 'catalog_preview' }, { status: 'catalog_applied', revision: 8 }]);
    answer = { exitCode: 1, class: 'refused', stdout: '', stderr: '', error: { code: 'catalog_name_conflict', message: 'The name is used.' } };
    await assert.rejects(library.createSet('Index 6', [idx(6)]), (error: any) => error.code === 'catalog_name_conflict' && /was not created: The name is used/.test(error.message)
        && error.nativeResponse.class === 'refused');
});

test('U104 the automatic scope is marked automatic in the choices and the summary; applying it makes it the user choice', async () => {
    const s = unionSession(), store = memento(), scope = new WindowScope(s.fake, ready, store, base, () => {}, s.library);
    await scope.ensureDefault(workspace);
    assert.deepEqual(s.selects, [{ namespace, expected_catalog_revision: 7, index_ids: [idx(4)], project_ids: [prj(2)] }]);
    assert.equal(scope.searchScopeSummary().automatic, true);
    await scope.readSearchScope();
    assert.equal(scope.searchScopeChoices()!.automatic, true);
    assert.deepEqual(scope.searchScopeChoices()!.selectedIndexIds, [idx(4)]);
    await scope.applySearchScope({ project_ids: [], index_ids: [idx(4)] });
    assert.deepEqual(s.selects.at(-1), { namespace, expected_catalog_revision: 7, index_ids: [idx(4)], project_ids: [prj(2)] });
    assert.equal(scope.searchScopeSummary().automatic, undefined);
    assert.equal(scope.searchScopeChoices()!.automatic, undefined);
    assert.deepEqual(store.values.get(WINDOW_SCOPE_KEY), { namespace, project_ids: [], index_ids: [idx(4)], explicit: true, union: true });
});
