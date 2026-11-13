// SPDX-License-Identifier: Apache-2.0
// Indexing page (U97): the window search scope and Recommendations, and the
// read-only index library with its AIRK Admin links. Each test executes the
// actual webview module with a minimal hook host.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import * as path from 'node:path';
import * as vm from 'node:vm';
import test from 'node:test';

/** Values built inside the webview's VM context, as plain objects of this realm. */
const plain = (value: unknown): unknown => JSON.parse(JSON.stringify(value));
import { buildSync } from 'esbuild';

const directory = path.resolve(__dirname, '../../src/explorerWebview');
const bundles = new Map<string, string>();
const namespace = 'ab'.repeat(16), indexId = 'idx_' + '12'.repeat(16), secondId = 'idx_' + '56'.repeat(16), projectId = 'prj_' + '34'.repeat(16);
const indexValue = { id: indexId, name: 'Application index', tombstone: false, definition_revision: 1, source_revision: 1, backing: { root: '/source' } };
const secondValue = { id: secondId, name: 'Second index', tombstone: false, definition_revision: 1, source_revision: 1, backing: { root: '/docs' } };
const projectValue = { id: projectId, name: 'Application Project', definition_revision: 1, scope_revision: 1, defaults_revision: 1, default_indexes: [] };
const page = (kind: string, value: unknown) => ({ status: 'catalog_page', page: { namespace, revision: 1, items: [{ kind, value }], continuation: null } });

/** Execute one webview module and its effects, retaining hook state between renders. */
function webview(name: string, state: any, renderWith: (exports: any) => any) {
    const entry = path.join(directory, name);
    let bundle = bundles.get(entry);
    if (!bundle) {
        bundle = buildSync({ entryPoints: [entry], bundle: true, write: false, platform: 'node', format: 'cjs', jsx: 'automatic', jsxImportSource: 'preact', external: ['preact', 'preact/hooks', 'preact/jsx-runtime','airk-explorer-help'] }).outputFiles[0].text;
        bundles.set(entry, bundle);
    }
    const slots: any[] = [], effects: (() => unknown)[] = [], sent: any[] = [];
    const events = new Map<string, (event: any) => void>();
    let cursor = 0, fail = false, failCode = 'HOST_UNAVAILABLE', failMessage = 'Native catalog unavailable';
    const hold=new Set<string>(),held:any[]=[];
    const hooks = {
        useState(initial: any) {
            const at = cursor++;
            if (!(at in slots)) { slots[at] = typeof initial === 'function' ? initial() : initial; }
            return [slots[at], (next: any) => { slots[at] = typeof next === 'function' ? next(slots[at]) : next; }];
        },
        useRef(initial: any) { const at = cursor++; return slots[at] ??= { current: initial }; },
        useEffect(effect: () => unknown, dependencies?: unknown[]) {
            const at = cursor++, prior = slots[at];
            if (!prior || !dependencies || dependencies.some((value, i) => !Object.is(value, prior[i]))) { effects.push(effect); }
            slots[at] = dependencies;
        },
        useMemo(get: () => unknown) { return get(); },
    };
    const module = { exports: {} as any }, nativeRequire = createRequire(entry);
    vm.runInNewContext(bundle, { module, exports: module.exports, TextEncoder,
        require: (id: string) => id === 'preact/hooks' ? hooks : id==='airk-explorer-help'?{topics:[]}:nativeRequire(id),
        setTimeout: () => 1, clearTimeout() {},
        window: { addEventListener: (name: string, listener: (event: any) => void) => events.set(name, listener) },
        acquireVsCodeApi: () => ({ getState() {}, setState() {}, postMessage(message: any) {
            sent.push(message);
            if(hold.has(message.type)){held.push(message);return;}
            queueMicrotask(() => events.get('message')?.({ data: { v: 1, type: 'result', requestId: message.requestId,
                ...(fail ? { ok: false, error: { code: failCode, message: failMessage } } : { ok: true, data: { generation: 1 } }) } }));
        } }),
    });
    return {
        sent, state, exports: module.exports, slots,
        hold(type:string){hold.add(type);},
        release(type:string,data:any={}){const at=held.findIndex(message=>message.type===type);assert.ok(at>=0,`Held ${type} request required`);const message=held.splice(at,1)[0];events.get('message')?.({data:{v:1,type:'result',requestId:message.requestId,ok:true,data}});},
        failure(value: boolean, code = 'HOST_UNAVAILABLE', message = 'Native catalog unavailable') { fail = value; failCode = code; failMessage = message; },
        render() { cursor = 0; return renderWith(module.exports); },
        reset() { slots.length = 0; effects.length = 0; },
        async flush() { while (effects.length) { effects.shift()!(); } for (let i = 0; i < 8; i++) { await Promise.resolve(); } },
    };
}

/** The Indexing screen, directly or through the real WorkspaceView (which keys its child as Preact does). */
function screen(tab: 'window' | 'library' | 'projects', indexing: any = {}, extraState: any = {}, workspace = false) {
    const state: any = { trusted: true, nativeExecutionEnabled: true, connection: 'ready', projects: [], features:{'indexing/window':'implemented','indexing/library':'implemented','indexing/projects':'implemented'}, indexing: { generation: 1, busy: false, ...indexing }, ...extraState };
    const props: any = { state, route: { workspace: 'indexing', tab }, go() {}, publications: [] };
    let mounted = false, childKey: unknown;
    const f = webview(workspace ? 'workspaces.tsx' : 'indexing.tsx', state, exports => {
        if (!workspace) { return exports.IndexingScreen(props); }
        // Reconcile the child chosen by the real WorkspaceView. A changed
        // key unmounts hook storage, just as Preact does in the editor.
        const tree = exports.WorkspaceView(props), child = nodes(tree).find(node => node.type === exports.SCREENS[`indexing/${props.route.tab}`]); assert.ok(child);
        if (mounted && child.key !== childKey) { f.reset(); }
        mounted = true; childKey = child.key;
        return child.type(child.props);
    });
    return { ...f, props };
}

/** The read-only library view of one tab. */
function library(tab: 'library' | 'projects', extraState: any = {}) {
    const state: any = { trusted: true, nativeExecutionEnabled: true, connection: 'ready', projects: [{ project: 'cd'.repeat(32) }], projectsObservedAt: 1, features: {}, ...extraState };
    return webview('libraryView.tsx', state, exports => exports.LibraryView({ tab, state }));
}

function nodes(node: any): any[] {
    if (Array.isArray(node)) { return node.flatMap(nodes); }
    return !node || typeof node !== 'object' ? [] : [node, ...nodes(node.props?.children)];
}
function text(node: any): string {
    if (node === undefined || node === null || typeof node === 'boolean') { return ''; }
    return Array.isArray(node) ? node.map(text).join('') : typeof node === 'object' ? text(node.props?.children) : String(node);
}
function button(tree: any, label: RegExp) { return nodes(tree).find(node => node.type === 'button' && label.test(text(node))); }
const component = (tree: any, name: string) => nodes(tree).filter(node => node.type?.name === name);
const fields = (message: any) => Object.fromEntries(Object.entries(message).filter(([key]) => key !== 'v' && key !== 'requestId'));
const cursorOf = (after: string) => ({ kind: 'indexes', namespace, revision: 7, after, index_id: null, project_id: null });
const reply = (items: unknown[], continuation: unknown = null, extra: Record<string, unknown> = {}) => ({
    method: { catalog_page: { request: { kind: 'indexes' } } }, expected: 'catalog_page', observedAt: 1,
    response: { status: 'catalog_page', page: { namespace, revision: 7, items, continuation }, capabilities: { future_native_capability: 'kept' }, ...extra },
});

test('every Indexing tab offers Open AIRK Admin and the library tabs render the read-only library', async () => {
    const window = screen('window', { searchScope: { indexes: [], projects: [], selectedIndexIds: [], selectedProjectIds: [] } });
    let tree = window.render(); await window.flush();
    assert.ok(component(tree, 'AdminButton').some(node => node.props.page === 'index-library'));
    for (const tab of ['library', 'projects'] as const) {
        const f = screen(tab); tree = f.render(); await f.flush();
        const view = component(tree, 'LibraryView'); assert.equal(view.length, 1, tab); assert.equal(view[0].props.tab, tab);
        assert.equal(f.sent.length, 0, 'the screen itself sends no catalog request');
        const lib = library(tab), inner = lib.render();
        assert.ok(component(inner, 'AdminButton').some(node => node.props.page === 'index-library' && !node.props.index && !node.props.set), tab);
        assert.equal(button(inner, /Delete|Split|Apply|Create index set|Save index set|Rename|Prepare/), undefined, `${tab} has no management action`);
    }
});

test('Search scope says it is not available only when the service lacks window-scope/1, keeping the native text in details', async () => {
    const f = screen('window'); f.failure(true, 'CAPABILITY_DISABLED', 'The window search scope is not available from this AIRK Service (window-scope/1).'); f.render(); await f.flush();
    assert.equal(f.sent.length, 1); assert.equal(f.sent[0].type, 'indexing.scope.read');
    const tree = f.render(), notice = component(tree, 'Notice').find(node => node.props.role === 'alert'); assert.ok(notice);
    assert.match(text(nodes(notice).find(node => node.type === 'p')), /window search scope is not available from this AIRK Service/);
    assert.ok(nodes(notice).some(node => node.type === 'details'), 'the native failure remains available');
});

test('Search scope failures show their own message and keep the code in details', async () => {
    const f = screen('window'); f.failure(true, 'scoped_read_unavailable', 'Search scope out of date — reselect.'); f.render(); await f.flush();
    const tree = f.render(), notice = component(tree, 'Notice').find(node => node.props.role === 'alert'); assert.ok(notice);
    assert.match(text(nodes(notice).find(node => node.type === 'p')), /Search scope out of date — reselect/);
    assert.doesNotMatch(text(notice), /not available from this AIRK Service/);
    assert.match(text(nodes(notice).find(node => node.type === 'details')), /scoped_read_unavailable/);
});

test('an out-of-date scope asks to reselect, and Clear releases it by applying nothing', async () => {
    const choices = { indexes: [indexValue], projects: [projectValue], selectedIndexIds: [], selectedProjectIds: [projectId], managedProjectIds: [], active: true, outOfDate: true, responses: [] };
    const f = screen('window', { searchScope: choices }); let tree = f.render(); await f.flush();
    const notice = component(tree, 'Notice').find(node => node.props.role === 'alert'); assert.ok(notice);
    assert.match(text(notice), /Search scope out of date — reselect/);
    const clear = button(tree, /Clear search scope/); assert.ok(clear);
    clear.props.onClick(); await f.flush(); await f.flush(); tree = f.render();
    assert.deepEqual(JSON.parse(JSON.stringify(f.sent.map(fields))), [{ type: 'indexing.scope.apply', generation: 0, selection: { index_ids: [], project_ids: [] } }, { type: 'indexing.scope.read' }]);
    assert.ok(component(tree, 'AdminButton').some(node => node.props.label === 'Create or change index sets in AIRK Admin' && node.props.page === 'index-library'), 'index sets are created in AIRK Admin');
});

test('the Overview and Search headers show the active scope with Clear', async () => {
    const searchScope = { available: true, active: true, outOfDate: false, remembered: true, label: 'Application Project' };
    let went: unknown;
    const f = webview('scopeBanner.tsx', {}, exports => exports.ScopeBanner({ state: { trusted: true, searchScope }, go(route: unknown) { went = route; } }));
    const tree = f.render(); await f.flush();
    assert.match(text(tree), /Searching: Application Project/);
    button(tree, /^Clear$/).props.onClick(); await f.flush();
    assert.deepEqual(JSON.parse(JSON.stringify(f.sent.map(fields))), [{ type: 'indexing.scope.apply', generation: 0, selection: { index_ids: [], project_ids: [] } }]);
    const stale = webview('scopeBanner.tsx', {}, exports => exports.ScopeBanner({ state: { trusted: true, searchScope: { ...searchScope, outOfDate: true } }, go(route: unknown) { went = route; } }));
    const staleTree = stale.render();
    assert.match(text(staleTree), /Search scope out of date — reselect/);
    button(staleTree, /Reselect/).props.onClick(); assert.deepEqual(JSON.parse(JSON.stringify(went)), { workspace: 'indexing', tab: 'window' });
    const none = webview('scopeBanner.tsx', {}, exports => exports.ScopeBanner({ state: { trusted: true, searchScope: { ...searchScope, available: false } }, go() {} }));
    assert.equal(none.render(), null, 'without window-scope/1 nothing is shown');
});

test('the read-only library reads pages through the session relay, continues with the native cursor and keeps every response', async () => {
    const f = library('library'); f.hold('library.read');
    f.render(); await f.flush();
    assert.equal(f.sent.length, 1); assert.deepEqual(fields(f.sent[0]), { type: 'library.read', method: 'page', kind: 'indexes' });
    f.release('library.read', reply([{ kind: 'index', value: indexValue }], cursorOf(indexId))); await f.flush();
    let tree = f.render();
    assert.match(text(tree), /Application index/); assert.match(text(tree), /\/source/);
    const link = component(tree, 'AdminButton').find(node => node.props.index === indexId); assert.ok(link, 'each index row deep-links into AIRK Admin');
    assert.equal(link.props.page, 'index-library');
    button(tree, /^Load more indexes$/).props.onClick(); await f.flush();
    assert.equal(f.sent.length, 2); assert.deepEqual(JSON.parse(JSON.stringify(f.sent[1].after)), cursorOf(indexId)); assert.equal(f.sent[1].revision, 7);
    f.release('library.read', reply([{ kind: 'index', value: secondValue }], null, { future_native_field: 'second-page' })); await f.flush();
    tree = f.render();
    assert.match(text(tree), /Application index/); assert.match(text(tree), /Second index/);
    assert.equal(button(tree, /^Load more/), undefined, 'a complete library offers no continuation');
    const complete = component(tree, 'Complete').map(node => JSON.stringify(node.props.value)).join('\n');
    assert.match(complete, /future_native_capability/); assert.match(complete, /second-page/);
    assert.ok(f.sent.every(message => message.type === 'library.read'), 'the library only reads');
    assert.equal(component(tree, 'CreateIndexForFolders').length, 0);
});

test('index set rows link to AIRK Admin and opening a row reads detail, memberships, sources and policy only', async () => {
    const sets = library('projects'); sets.hold('library.read'); sets.render(); await sets.flush();
    assert.equal(sets.sent[0].kind, 'projects');
    sets.release('library.read', reply([{ kind: 'project', value: projectValue }])); await sets.flush();
    assert.ok(component(sets.render(), 'AdminButton').some(node => node.props.set === projectId));
    const f = library('library'); f.hold('library.read'); f.render(); await f.flush();
    f.release('library.read', reply([{ kind: 'index', value: indexValue }])); await f.flush();
    button(f.render(), /^Show index details$/).props.onClick(); await f.flush();
    for (let i = 0; i < 4; i++) { f.release('library.read', i === 3 ? { method: {}, expected: 'catalog_policy_status', response: { status: 'catalog_policy_status', index_id: indexId, snapshot: null, job: null }, observedAt: 1 } : reply([])); await f.flush(); }
    assert.deepEqual(f.sent.slice(1).map(fields), [
        { type: 'library.read', method: 'detail', id: indexId }, { type: 'library.read', method: 'page', kind: 'memberships', id: indexId },
        { type: 'library.read', method: 'page', kind: 'sources', id: indexId }, { type: 'library.read', method: 'policy', id: indexId },
    ]);
    assert.match(text(f.render()), /No policy recorded for this index/);
});

test('a failed library read shows its native code and is never an empty library', async () => {
    const f = library('library'); f.hold('library.read'); f.render(); await f.flush();
    f.release('library.read', { method: {}, expected: 'catalog_page', error: { code: 'scoped_read_unavailable', message: 'The catalog changed.', retryable: true }, observedAt: 1 }); await f.flush();
    const tree = f.render(), errors = component(tree, 'ErrorLine').filter(node => node.props.error);
    assert.equal(errors.length, 1); assert.equal(errors[0].props.error.code, 'scoped_read_unavailable');
    assert.doesNotMatch(text(tree), /No indexes in the catalog/);
    assert.equal(component(tree, 'CreateIndexForFolders').length, 0, 'a failure never offers creation as if the library were empty');
    f.render(); await f.flush(); assert.equal(f.sent.length, 1, 'a failed read waits for Refresh');
    button(f.render(), /^Refresh indexes$/).props.onClick(); await f.flush(); assert.equal(f.sent.length, 2);
});

test('the library reads only when ready and refreshes after Check again or window focus', async () => {
    for (const extra of [{ trusted: false }, { nativeExecutionEnabled: false }, { connection: 'connecting' }]) {
        const f = library('library', extra); f.render(); await f.flush(); assert.equal(f.sent.length, 0, JSON.stringify(extra));
    }
    const f = library('library'); f.render(); await f.flush(); assert.equal(f.sent.length, 1);
    f.render(); await f.flush(); assert.equal(f.sent.length, 1, 'an unchanged observation does not reread');
    f.state.projectsObservedAt = 2; f.render(); await f.flush();
    assert.equal(f.sent.length, 2, 'a host project refresh (focus, Check again) rereads the first page');
    assert.equal(f.sent[1].after, undefined);
});

test('with no index the library and its links offer Create an index for this folder', async () => {
    const folders = ['C:\\work\\app', 'D:\\docs'];
    const none = library('library', { projects: [], workspaceFolders: folders }); none.hold('library.read'); none.render(); await none.flush();
    none.release('library.read', reply([])); await none.flush();
    let create = component(none.render(), 'CreateIndexForFolders'); assert.equal(create.length, 1); assert.deepEqual(create[0].props.folders, folders);
    const retired = library('library', { workspaceFolders: folders }); retired.hold('library.read'); retired.render(); await retired.flush();
    retired.release('library.read', reply([{ kind: 'index', value: { ...indexValue, tombstone: true } }])); await retired.flush();
    create = component(retired.render(), 'CreateIndexForFolders'); assert.equal(create.length, 1, 'a catalog with only retired indexes has no live index');
    const partial = library('library', { workspaceFolders: folders }); partial.hold('library.read'); partial.render(); await partial.flush();
    partial.release('library.read', reply([{ kind: 'index', value: { ...indexValue, tombstone: true } }], cursorOf(indexId))); await partial.flush();
    assert.equal(component(partial.render(), 'CreateIndexForFolders').length, 0, 'an incomplete library is not proof of no index');

    const links = webview('adminLinks.tsx', {}, exports => exports.CreateIndexForFolders({ folders }));
    const buttons = component(links.render(), 'AdminButton');
    assert.deepEqual(buttons.map(node => [node.props.page, node.props.folder, node.props.label]), folders.map(folder => ['index-library', folder, 'Create an index for this folder']));
    const opener = webview('adminLinks.tsx', {}, exports => exports.AdminButton({ page: 'index-library', folder: folders[0], label: 'Create an index for this folder' }));
    button(opener.render(), /^Create an index for this folder$/).props.onClick(); await opener.flush();
    button(opener.render(), /^Choose where/).props.onClick(); await opener.flush();
    assert.deepEqual(opener.sent.map(fields), [{ type: 'admin.open', page: 'index-library', folder: folders[0] }, { type: 'admin.open', page: 'index-library', folder: folders[0], choose: true }]);
    const handoff = webview('adminLinks.tsx', {}, exports => exports.AdminHandoffScreen({ route: { workspace: 'manage', tab: 'repair' }, state: { trusted: true } }));
    assert.equal(component(handoff.render(), 'AdminButton')[0].props.page, 'recovery', 'Management routes open the matching admin page');
});

test('Search scope loads local index and set choices once without an association prerequisite', async () => {
    const f = screen('window'); f.render(); await f.flush();
    assert.equal(f.sent.length, 1); assert.equal(f.sent[0].type, 'indexing.scope.read');
    f.render(); await f.flush(); assert.equal(f.sent.length, 1);
    assert.equal(button(f.render(), /Read grants|Read saved association|Use Grant/), undefined);
});

test('real WorkspaceView keeps scope Apply ownership across project and evidence-epoch changes',async()=>{
    const initial={indexes:[indexValue],projects:[projectValue],selectedIndexIds:[],selectedProjectIds:[],active:false};
    const f=screen('window',{searchScope:initial},{connection:'ready',selectedProject:'original',selectionEpoch:1},true);
    f.hold('indexing.scope.apply');f.hold('indexing.scope.read');let tree=f.render();await f.flush();
    const projectChoice=nodes(tree).find(node=>node.type==='label'&&text(node).includes(projectValue.name));
    nodes(projectChoice).find(node=>node.type==='input').props.onChange({currentTarget:{checked:true}});tree=f.render();
    const form=nodes(tree).find(node=>node.type==='form'&&button(node,/^Apply selection$/));assert.ok(form);
    form.props.onSubmit({preventDefault(){}});await f.flush();
    assert.equal(f.sent.filter(message=>message.type==='indexing.scope.apply').length,1);
    assert.equal(f.sent[0].selection.project_ids[0],projectId);
    f.state.selectedProject='discovered';f.state.selectionEpoch=2;f.state.indexing={generation:3,busy:false};tree=f.render();await f.flush();
    assert.equal(button(tree,/Refresh choices/).props.disabled,true);
    button(tree,/Refresh choices/).props.onClick();await f.flush();
    assert.equal(f.sent.filter(message=>message.type==='indexing.scope.read').length,0,'automatic and manual reads cannot race Apply');
    f.release('indexing.scope.apply',{status:'search_scope_selected'});await f.flush();tree=f.render();await f.flush();
    assert.equal(f.sent.filter(message=>message.type==='indexing.scope.read').length,1,'only the explicit post-Apply read runs');
    assert.equal(button(tree,/Refresh choices/).props.disabled,true,'the complete action remains exclusive during hydration');
    f.state.indexing={generation:4,busy:false,searchScope:{...initial,active:true,selectedProjectIds:[projectId],responses:[page('project',projectValue)]}};
    f.release('indexing.scope.read');await f.flush();tree=f.render();await f.flush();
    assert.equal(button(tree,/Refresh choices/).props.disabled,false);assert.equal(f.state.indexing.searchScope.active,true);
    assert.equal(f.sent.filter(message=>message.type==='indexing.scope.read').length,1);
});

test('Search scope checkboxes apply explicit indexes and sets including an empty selection', async () => {
    const f=screen('window',{searchScope:{indexes:[indexValue],projects:[projectValue],selectedIndexIds:[],selectedProjectIds:[],managedProjectIds:[]}});
    let tree=f.render();await f.flush();tree=f.render();
    const boxes=nodes(tree).filter(node=>node.type==='input'&&node.props.type==='checkbox');
    assert.equal(boxes.length,2);assert.match(text(tree),/Application Project/);
    boxes[0].props.onChange({currentTarget:{checked:true}});boxes[1].props.onChange({currentTarget:{checked:true}});
    tree=f.render();const form=nodes(tree).find(node=>node.type==='form');form.props.onSubmit({preventDefault(){}});await f.flush();
    const applied=f.sent.find(message=>message.type==='indexing.scope.apply');
    assert.deepEqual(JSON.parse(JSON.stringify(applied.selection)),{index_ids:[indexId],project_ids:[projectId]});
    for(const box of nodes(tree).filter(node=>node.type==='input'&&node.props.type==='checkbox'))box.props.onChange({currentTarget:{checked:false}});
    nodes(f.render()).find(node=>node.type==='form').props.onSubmit({preventDefault(){}});await f.flush();
    assert.deepEqual(JSON.parse(JSON.stringify(f.sent.filter(message=>message.type==='indexing.scope.apply').at(-1).selection)),{index_ids:[],project_ids:[]});
    assert.equal(f.sent.some(message=>/grant|context/.test(message.type)),false);
});

test('Search scope lists every index set and marks edited choices as unapplied', async () => {
    // U103: the never-populated managedProjectIds filter is gone; every listed index set is a choice.
    const other='prj_'+'99'.repeat(16);
    const f=screen('window',{searchScope:{indexes:[indexValue],projects:[projectValue,{id:other,name:'Other set'}],selectedIndexIds:[],selectedProjectIds:[],active:true}});
    f.render();await f.flush();let tree=f.render();
    assert.match(text(nodes(tree).filter(node=>node.type==='fieldset')),/Other set/);assert.match(text(tree),/Selection active/);
    nodes(tree).find(node=>node.type==='input'&&node.props.type==='checkbox').props.onChange({currentTarget:{checked:true}});
    tree=f.render();assert.match(text(tree),/Changes not applied/);assert.doesNotMatch(text(tree),/Selection active/);
});

test('U104 Apply follows the union: changed or automatic selections apply; outside and set-less indexes are listed, never refused', async () => {
    const { scopeApplyState } = webview('indexing.tsx', {}, exports => exports).exports;
    const other = 'prj_' + '99'.repeat(16);
    const sets = { [indexId]: [projectId] }, applied = { appliedIndexes: [], appliedProjects: [projectId], indexSets: sets, indexSetsComplete: true };
    assert.deepEqual(plain(scopeApplyState({ ...applied, indexes: [], projects: [projectId] })), { changed: false, valid: true, enabled: false, extra: [], newSets: [] }, 'nothing changed');
    assert.deepEqual(plain(scopeApplyState({ ...applied, indexes: [indexId], projects: [projectId] })), { changed: true, valid: true, enabled: true, extra: [], newSets: [] }, 'a member of a checked set adds nothing');
    assert.deepEqual(plain(scopeApplyState({ ...applied, indexes: [indexId], projects: [] })), { changed: true, valid: true, enabled: true, extra: [indexId], newSets: [] }, 'an index alone with a set');
    assert.deepEqual(plain(scopeApplyState({ ...applied, indexes: [secondId], projects: [] })), { changed: true, valid: true, enabled: true, extra: [secondId], newSets: [secondId] }, 'an index in no set gets one on Apply');
    assert.deepEqual(plain(scopeApplyState({ ...applied, indexes: [secondId], projects: [projectId] })), { changed: true, valid: true, enabled: true, extra: [secondId], newSets: [secondId] }, 'a set plus a set-less index');
    assert.deepEqual(plain(scopeApplyState({ ...applied, indexSets: { ...sets, [secondId]: [other] }, indexes: [secondId], projects: [projectId] })),
        { changed: true, valid: true, enabled: true, extra: [secondId], newSets: [] }, 'a set plus an index of another set is a union');
    assert.deepEqual(plain(scopeApplyState({ indexes: [indexId], projects: [], appliedIndexes: [indexId], appliedProjects: [], automatic: true, indexSets: sets, indexSetsComplete: true })),
        { changed: false, valid: true, enabled: true, extra: [indexId], newSets: [] }, 'the automatic scope can be applied unchanged to make it the user choice');
    assert.deepEqual(plain(scopeApplyState({ indexes: [secondId], projects: [], appliedIndexes: [], appliedProjects: [], indexSetsComplete: false })),
        { changed: true, valid: true, enabled: true, extra: [], newSets: [] }, 'unknown memberships leave validation to the service');
    assert.deepEqual(plain(scopeApplyState({ indexes: [], projects: [], appliedIndexes: [], appliedProjects: [] })), { changed: false, valid: true, enabled: false, extra: [], newSets: [] }, 'nothing checked and nothing applied');
    const many = Array.from({ length: 33 }, (_, n) => 'idx_' + n.toString(16).padStart(2, '0').repeat(16));
    const large = scopeApplyState({ indexes: [], projects: [projectId], appliedIndexes: [], appliedProjects: [], indexSets: Object.fromEntries(many.map(id => [id, [projectId]])), indexSetsComplete: true });
    assert.equal(large.enabled, false); assert.match(large.limit, /at most 32 indexes; this selection has 33/);
});

test('U104 checking an index in no index set keeps Apply enabled and says Apply creates a set named after it', async () => {
    const f=screen('window',{searchScope:{indexes:[indexValue,secondValue],projects:[projectValue],selectedIndexIds:[],selectedProjectIds:[],active:false,indexSets:{[indexId]:[projectId]},indexSetsComplete:true,responses:[]}});
    let tree=f.render();await f.flush();tree=f.render();
    const apply=()=>button(f.render(),/^Apply selection$/);
    assert.equal(apply().props.disabled,true,'nothing changed');
    assert.match(text(tree),/In no index set: Apply creates an index set named after it/);
    const box=(label:string)=>nodes(nodes(f.render()).find(node=>node.type==='label'&&text(node).includes(label))).find(node=>node.type==='input');
    box('Second index').props.onChange({currentTarget:{checked:true}});box('Application Project').props.onChange({currentTarget:{checked:true}});
    tree=f.render();
    assert.equal(apply().props.disabled,false,'a set plus a set-less index can be applied');
    assert.match(text(tree),/Second index is in no index set\. Apply creates an index set named “Second index” for it/);
    assert.equal(component(tree,'AdminButton').some(node=>node.props.index===secondId),false,'no Make readable callout on this page');
    assert.doesNotMatch(text(tree),/narrow/);
    nodes(tree).find(node=>node.type==='form').props.onSubmit({preventDefault(){}});await f.flush();
    assert.deepEqual(plain(f.sent.find(message=>message.type==='indexing.scope.apply').selection),{index_ids:[secondId],project_ids:[projectId]});
});

test('U104 the automatic scope enables Apply unchanged; Save as index set sends the checked choice; Connect an agent opens Management › Agents', async () => {
    const f=screen('window',{searchScope:{indexes:[indexValue,secondValue],projects:[projectValue],selectedIndexIds:[indexId],selectedProjectIds:[],active:true,automatic:true,indexSets:{[indexId]:[projectId]},indexSetsComplete:true,responses:[]}});
    let went:unknown;f.props.go=(route:unknown)=>{went=route;};
    let tree=f.render();await f.flush();tree=f.render();
    assert.equal(button(tree,/^Apply selection$/).props.disabled,false);
    assert.match(text(tree),/read automatically\. Apply to make it this window's chosen scope/);
    const save=()=>button(f.render(),/^Save as index set$/);
    assert.equal(save().props.disabled,true,'a name is required');
    nodes(f.render()).find(node=>node.type==='input'&&node.props.type==='text').props.onInput({currentTarget:{value:'  Review set '}});
    assert.equal(save().props.disabled,false);
    save().props.onClick();await f.flush();await f.flush();
    assert.deepEqual(plain(f.sent.map(fields).filter(message=>message.type!=='indexing.scope.read')),[{type:'indexing.scope.saveSet',name:'Review set',selection:{index_ids:[indexId],project_ids:[]}}]);
    assert.equal(f.sent.at(-1).type,'indexing.scope.read','the choices are read again after saving');
    const links=nodes(f.render()).filter(node=>node.type==='button'&&/^Connect an agent$/.test(text(node)));
    assert.ok(links.length>=1);links[0].props.onClick();
    assert.deepEqual(plain(went),{workspace:'manage',tab:'agents'});
});

test('U104 Management › Agents lists this folder\'s entries, disconnects one and connects the chosen agent', async () => {
    const searchScope={available:true,active:true,outOfDate:false,remembered:true,label:'Application Project'};
    const state:any={trusted:true,nativeExecutionEnabled:true,connection:'ready',projects:[],features:{},searchScope};
    const props:any={state,route:{workspace:'manage',tab:'agents'},go(){},publications:[]};
    const f=webview('agents.tsx',state,exports=>exports.AgentsScreen(props));
    const id='0a'.repeat(16),report={entries:[],checked:[],future_native_field:'kept'};
    const listed={folder:'/work/app',report,entries:[{client:'claude',agent:'claude',file:'/work/app/.mcp.json',scope:'project',serverKey:'airk',connectionId:id,status:'ok'}]};
    f.hold('agents.read');f.render();await f.flush();
    assert.deepEqual(f.sent.map(fields),[{type:'agents.read'}]);
    f.release('agents.read',listed);await f.flush();
    let tree=f.render();
    assert.match(text(tree),/Search scope: Application Project/);
    assert.match(text(tree),/Claude Code · \/work\/app\/\.mcp\.json · airk · connection 0a0a/);
    assert.deepEqual(plain(component(tree,'CompleteNativeResponse').at(-1).props.value),report,'the complete native report is shown');
    assert.ok(component(tree,'AdminButton').some(node=>node.props.page==='agents'),'Open AIRK Admin stays for advanced management');
    assert.deepEqual(plain(nodes(tree).filter(node=>node.type==='option').map(node=>node.props.value)),['claude','codex','vscode']);
    nodes(tree).find(node=>node.type==='select').props.onChange({currentTarget:{value:'codex'}});
    f.hold('agents.connect');button(f.render(),/^Connect$/).props.onClick();await f.flush();
    const second='1b'.repeat(16);
    f.release('agents.connect',{status:'connected',client:'codex',connectionId:second,file:'/work/app/.codex/config.toml',changed:true,scope:[{kind:'set',id:projectId,name:'Application Project'}],follows:true,open:{agent_choice:'off'}});await f.flush();
    f.release('agents.read',listed);await f.flush();
    tree=f.render();
    assert.match(text(tree),/Codex connected in \/work\/app\/\.codex\/config\.toml \(connection 1b1b/);
    assert.match(text(tree),/It follows this window's search scope\. Agent choice is off\./);
    assert.match(text(tree),/If Codex is already running, restart it to load the new entry\./,'a new entry may need a running agent to load it');
    assert.doesNotMatch(text(tree),/Coming in the next step/);
    f.hold('agents.disconnect');button(f.render(),/^Disconnect$/).props.onClick();await f.flush();
    f.release('agents.disconnect',{schema:1,actions:[]});await f.flush();
    f.release('agents.read',{...listed,entries:[]});await f.flush();
    assert.deepEqual(f.sent.map(fields).slice(1),[{type:'agents.connect',client:'codex'},{type:'agents.read'},{type:'agents.disconnect',connectionId:id},{type:'agents.read'}]);
    assert.match(text(f.render()),/No AIRK agent entries in this folder/);
    // Without an applied scope, Connect is disabled and the page links to Search scope.
    let went:unknown;
    const none:any={...state,searchScope:{...searchScope,active:false}};
    const g=webview('agents.tsx',none,exports=>exports.AgentsScreen({...props,state:none,go(route:unknown){went=route;}}));
    g.hold('agents.read');tree=g.render();await g.flush();tree=g.render();
    assert.equal(button(tree,/^Connect$/).props.disabled,true);
    assert.match(text(tree),/No search scope is applied in this window/);
    button(tree,/^Choose a search scope$/).props.onClick();assert.deepEqual(plain(went),{workspace:'indexing',tab:'window'});
    const screens=webview('workspaces.tsx',{},exports=>exports).exports.SCREENS;
    assert.equal(screens['manage/agents'].name,'AgentsScreen');
});

test('U105 an open connection shows its default scope, Follow and the choice switch (off by default) with what each setting means', async () => {
    const searchScope={available:true,active:true,outOfDate:false,remembered:true,label:'Application Project'};
    const state:any={trusted:true,nativeExecutionEnabled:true,connection:'ready',projects:[],features:{},searchScope};
    const props:any={state,route:{workspace:'manage',tab:'agents'},go(){},publications:[]};
    const f=webview('agents.tsx',state,exports=>exports.AgentsScreen(props));
    const id='0a'.repeat(16),grant='0c'.repeat(16),scopeItems=[{kind:'set',id:projectId,name:'Application Project',description:'Services <b>and</b> UI'},{kind:'index',id:secondId}];
    const openView={defaults_revision:2,default_index_ids:[secondId],default_index_set_ids:[projectId],agent_choice:'off',scope:scopeItems};
    const listed=(choice:'off'|'on',follows=true,extra:any={})=>({folder:'/work/app',report:{entries:[]},windowScope:{items:scopeItems},entries:[
        {client:'claude',agent:'claude',file:'/work/app/.mcp.json',scope:'project',serverKey:'airk',connectionId:id,status:'ok'},
        {client:'vscode',agent:'copilot',file:'/work/app/.vscode/mcp.json',scope:'project',serverKey:'airk',connectionId:id,status:'ok'},
        {client:'codex',agent:'codex',file:'/work/app/.codex/config.toml',scope:'project',serverKey:'airk',connectionId:grant,status:'ok'}],
        connections:[{connectionId:id,remembered:true,follows,open:{...openView,agent_choice:choice},info:{open:openView},...extra},{connectionId:grant,remembered:false,follows:false,info:{schema:1}}]});
    f.hold('agents.read');f.render();await f.flush();
    f.release('agents.read',listed('off'));await f.flush();
    let tree=f.render();
    const switches=()=>nodes(f.render()).filter(node=>node.type==='input'&&node.props.role==='switch');
    assert.equal(switches().length,1,'settings show once per open connection; a grant-bound connection has none');
    assert.equal(switches()[0].props.checked,false,'agent choice is off');
    assert.match(text(tree),/Let the agent choose indexes/);
    assert.match(text(tree),/Off \(default\): the agent searches only its default scope, which is this window's search scope while it follows the window\. It cannot list or pick indexes\./);
    assert.match(text(tree),/This connection reads the scope it was connected with; manage it in AIRK Admin › Agents\./);
    const follow=()=>nodes(f.render()).find(node=>node.type==='input'&&node.props.type==='checkbox'&&node.props.role!=='switch');
    assert.equal(follow().props.checked,true);
    // Scope names render escaped; each description is the tooltip.
    const names=component(tree,'ScopeNames');assert.ok(names.length>=2);
    const rendered=names[0].type(names[0].props),spans=nodes(rendered).filter((node:any)=>node.type==='span'&&node.props.title);
    assert.deepEqual(plain(spans.map((node:any)=>[node.props.title,text(node)])),[['Services <b>and</b> UI','Application Project'],[secondId,secondId]]);
    // Choice on.
    f.hold('agents.choice');switches()[0].props.onChange({currentTarget:{checked:true}});await f.flush();
    f.release('agents.choice',{connectionId:id,choice:'on',result:{}});await f.flush();
    f.release('agents.read',listed('on'));await f.flush();
    tree=f.render();
    assert.equal(switches()[0].props.checked,true);
    assert.match(text(tree),/On: the agent can list every index and index set on this computer, with their descriptions and folders, and search any of them\./);
    assert.match(text(tree),/Agent choice turned on\. A running Claude Code session may need \/mcp to reload its tool list\./);
    // Follow off.
    f.hold('agents.follow');follow().props.onChange({currentTarget:{checked:false}});await f.flush();
    f.release('agents.follow',{connectionId:id,follow:false});await f.flush();
    f.release('agents.read',listed('on',false,{followError:{code:'agent_defaults_revision_changed',message:'The defaults changed.'}}));await f.flush();
    tree=f.render();
    assert.equal(follow().props.checked,false);
    assert.match(text(tree),/This connection keeps its current default scope\./);
    assert.match(text(tree),/This window's search scope was not passed on \(agent_defaults_revision_changed\): The defaults changed\./);
    assert.deepEqual(f.sent.map(fields),[{type:'agents.read'},{type:'agents.choice',connectionId:id,choice:'on'},{type:'agents.read'},{type:'agents.follow',connectionId:id,follow:false},{type:'agents.read'}]);
    // Following on: no reconnect is needed for a default-scope change.
    f.hold('agents.follow');follow().props.onChange({currentTarget:{checked:true}});await f.flush();
    f.release('agents.follow',{connectionId:id,follow:true,result:{}});await f.flush();
    f.release('agents.read',listed('on'));await f.flush();
    assert.match(text(f.render()),/follows this window's search scope\. Changing the default scope takes effect on the agent's next call; no reconnect is needed\./);
});

test('U105 Search scope shows each index and set description as escaped text and edits it within 1 KiB', async () => {
    const described={...indexValue,description:'Line one\n<script>alert(1)</script>'},setDescribed={...projectValue,description:'Shared code'};
    const f=screen('window',{searchScope:{indexes:[described,secondValue],projects:[setDescribed],selectedIndexIds:[],selectedProjectIds:[],active:false,responses:[]}});
    let tree=f.render();await f.flush();tree=f.render();
    const smalls=nodes(tree).filter(node=>node.type==='small'&&node.props.class==='description');
    assert.deepEqual(plain(smalls.map(node=>node.props.children)),['Line one\n<script>alert(1)</script>','Shared code'],'descriptions are text children, never markup');
    const edits=()=>nodes(f.render()).filter(node=>node.type==='button'&&text(node)==='Edit description');
    assert.equal(edits().length,3,'every index and index set offers Edit description');
    edits()[0].props.onClick();
    const area=()=>nodes(f.render()).find(node=>node.type==='textarea');
    assert.equal(area().props.value,'Line one\n<script>alert(1)</script>');
    assert.ok(edits().every(node=>node.props.disabled),'one description is edited at a time');
    area().props.onInput({currentTarget:{value:'x'.repeat(1025)}});
    assert.equal(button(f.render(),/^Save$/).props.disabled,true,'more than 1 KiB cannot be saved');
    assert.match(text(f.render()),/1025 of 1024 bytes/);
    area().props.onInput({currentTarget:{value:'Payments API\nand its tests'}});
    button(f.render(),/^Save$/).props.onClick();await f.flush();await f.flush();
    assert.equal(area(),undefined,'the editor closes after saving');
    edits()[2].props.onClick();
    button(f.render(),/^Clear$/).props.onClick();await f.flush();await f.flush();
    assert.deepEqual(plain(f.sent.map(fields)),[{type:'indexing.scope.describe',kind:'index',id:indexId,text:'Payments API\nand its tests'},{type:'indexing.scope.read'},
        {type:'indexing.scope.describe',kind:'set',id:projectId},{type:'indexing.scope.read'}]);
    // Clear is offered only for an existing description.
    edits()[1].props.onClick();
    assert.equal(button(f.render(),/^Clear$/).props.disabled,true);
});

test('Refresh choices is suggested only while it is enabled, and Dismiss only with loaded recommendations', async () => {
    const offline=screen('window',{},{connection:'disconnected'});const tree=offline.render();await offline.flush();
    assert.doesNotMatch(text(offline.render()),/Use Refresh choices to retry/);
    assert.equal(button(tree,/Keep this layout \/ Dismiss/),undefined);
    const loaded=screen('window',{recommendations:{schema:'airk.recommendations/1',selected:{},cards:[],available:[],omitted_index_ids:[],observation:{source:'saved_metadata'},observed_unix_ms:1}});loaded.render();await loaded.flush();
    assert.ok(button(loaded.render(),/Keep this layout \/ Dismiss/));
});

test('Index actions send the selected index and the exact job for retry and cancel', async () => {
    const project={project:'cd'.repeat(32),name:'demo',collection:'demo',root:'/src',current:true,paused:false,watching:true,jobId:'ef'.repeat(32),jobState:'failed',pendingJobId:'12'.repeat(32),pendingJobState:'pending'};
    const f=webview('projectStatus.tsx',{},exports=>exports.WorkActions({project,disabled:false}));
    const tree=f.render();
    assert.deepEqual(nodes(tree).filter(node=>node.type==='button').map(node=>text(node)),['Index now','Pause','Turn watching off','Retry job','Cancel queued job']);
    for(const label of ['Index now','Pause','Turn watching off','Retry job','Cancel queued job']){button(f.render(),new RegExp(`^${label}$`)).props.onClick();await f.flush();}
    assert.deepEqual(f.sent.map(fields),[
        {type:'work.action',action:'index',project:project.project},{type:'work.action',action:'pause',project:project.project},
        {type:'work.action',action:'watch_off',project:project.project},{type:'work.action',action:'retry',project:project.project,job:project.jobId},
        {type:'work.action',action:'cancel',project:project.project,job:project.pendingJobId}]);
    const choices=webview('projectStatus.tsx',{},exports=>exports).exports.workActionChoices;
    assert.deepEqual(plain(choices({paused:true,watching:false,jobId:'ef'.repeat(32),jobState:'running'}).map((choice:any)=>[choice.action,choice.job ?? null])),
        [['index',null],['resume',null],['watch_on',null],['cancel','ef'.repeat(32)]]);
});

test('Management › Usage reads the usage report of the selected index and renders it with the complete response', async () => {
    const report={ok:true,argv:['usage','report','--days','7','--format','json','--service-project','cd'.repeat(32),'--output','json'],class:'success',exitCode:0,
        result:{status:'usage_document',document:{days:7,requests:42,returned_bytes:9000,clients:{explorer:30,agents:12}}}};
    const state:any={trusted:true,nativeExecutionEnabled:true,connection:'ready',selectedProject:'cd'.repeat(32),projects:[{project:'cd'.repeat(32),name:'demo'}],features:{}};
    const f=webview('manageViews.tsx',state,exports=>exports.UsageScreen({state,route:{workspace:'manage',tab:'usage'},go(){},publications:[]}));
    f.hold('work.usage');f.render();await f.flush();
    assert.deepEqual(f.sent.map(fields),[{type:'work.usage',days:7}]);
    f.release('work.usage',report);await f.flush();
    const tree=f.render(),shown=text(tree);
    assert.match(shown,/Read complete/);
    assert.deepEqual(plain(component(tree,'Facts')[0].props.rows),[['status','usage_document'],['document.days','7'],['document.requests','42'],['document.returned_bytes','9000'],['document.clients.explorer','30'],['document.clients.agents','12']]);
    const complete=component(tree,'CompleteNativeResponse');assert.equal(complete.length,1);assert.deepEqual(complete[0].props.value,report);
    assert.equal(component(tree,'AdminHandoffScreen').length,0);
    assert.ok(component(tree,'AdminButton').some(node=>node.props.page==='usage'),'usage settings stay in AIRK Admin');
    const workspace=webview('workspaces.tsx',{},exports=>exports).exports.SCREENS;
    assert.equal(workspace['manage/usage'].name,'UsageScreen');assert.equal(workspace['manage/activity'].name,'ActivityScreen');assert.equal(workspace['manage/storage'].name,'AdminHandoffScreen');
});
