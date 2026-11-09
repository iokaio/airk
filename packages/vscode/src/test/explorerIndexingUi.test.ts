// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import * as path from 'node:path';
import * as vm from 'node:vm';
import test from 'node:test';
import { buildSync } from 'esbuild';
import { splitFixture, splitSourceResponse } from './fixtures/split';

const file = path.resolve(__dirname, '../../src/explorerWebview/indexing.tsx');
const bundles = new Map<string, string>();
const namespace = 'ab'.repeat(16), indexId = 'idx_' + '12'.repeat(16), projectId = 'prj_' + '34'.repeat(16);
const indexValue = { id: indexId, name: 'Application index', tombstone: false, definition_revision: 1, source_revision: 1, backing: { root: '/source' } };
const projectValue = { id: projectId, name: 'Application Project', definition_revision: 1, scope_revision: 1, defaults_revision: 1, default_indexes: [] };
const page = (kind: string, value: unknown) => ({ status: 'catalog_page', page: { namespace, revision: 1, items: [{ kind, value }], continuation: null } });
const detail = (kind: string, value: unknown) => ({ status: 'catalog_detail', detail: { namespace, revision: 1, item: { kind, value } } });

/** Execute the actual screen and its effects, retaining hook state between host snapshots. */
function screen(tab: 'window' | 'library' | 'projects', indexing: any = {}, extraState: any = {}, panelProps?: any, workspace=false) {
    const entry = path.join(path.dirname(file), workspace?'workspaces.tsx':panelProps?'indexingSplit.tsx':'indexing.tsx');
    let bundle = bundles.get(entry);
    if (!bundle) {
        bundle = buildSync({ entryPoints: [entry], bundle: true, write: false, platform: 'node', format: 'cjs', jsx: 'automatic', jsxImportSource: 'preact', external: ['preact', 'preact/hooks', 'preact/jsx-runtime','airk-explorer-help'] }).outputFiles[0].text;
        bundles.set(entry, bundle);
    }
    const state: any = { trusted: true, nativeExecutionEnabled: true, features:{'indexing/window':'implemented','indexing/library':'implemented','indexing/projects':'implemented'}, indexing: { generation: 1, busy: false, ...indexing }, ...extraState };
    const props: any = { state, route: { workspace: 'indexing', tab }, go() {}, publications: [] };
    const slots: any[] = [], effects: (() => unknown)[] = [], sent: any[] = [];
    const events = new Map<string, (event: any) => void>();
    let cursor = 0, fail = false, mounted=false, childKey:unknown;
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
    const module = { exports: {} as any }, nativeRequire = createRequire(file);
    vm.runInNewContext(bundle, { module, exports: module.exports, TextEncoder,
        require: (id: string) => id === 'preact/hooks' ? hooks : id==='airk-explorer-help'?{topics:[]}:nativeRequire(id),
        setTimeout: () => 1, clearTimeout() {},
        window: { addEventListener: (name: string, listener: (event: any) => void) => events.set(name, listener) },
        acquireVsCodeApi: () => ({ getState() {}, setState() {}, postMessage(message: any) {
            sent.push(message);
            if(hold.has(message.type)){held.push(message);return;}
            queueMicrotask(() => events.get('message')?.({ data: { v: 1, type: 'result', requestId: message.requestId,
                ...(fail ? { ok: false, error: { code: 'HOST_UNAVAILABLE', message: 'Native catalog unavailable' } } : { ok: true, data: { generation: 1, ...(message.type==='indexing.apply'?{outcome:{status:'catalog_applied'}}:{}) } }) } }));
        } }),
    });
    return {
        sent, state, props,
        hold(type:string){hold.add(type);},
        release(type:string,data:any={}){const at=held.findIndex(message=>message.type===type);assert.ok(at>=0,`Held ${type} request required`);const message=held.splice(at,1)[0];events.get('message')?.({data:{v:1,type:'result',requestId:message.requestId,ok:true,data}});},
        failure(value: boolean) { fail = value; },
        render() {
            cursor = 0;
            if(workspace){
                // Reconcile the child chosen by the real WorkspaceView. A changed
                // key unmounts hook storage, just as Preact does in the editor.
                const tree=module.exports.WorkspaceView(props),child=nodes(tree).find(node=>node.type===module.exports.SCREENS[`indexing/${props.route.tab}`]);assert.ok(child);
                if(mounted&&child.key!==childKey){slots.length=0;effects.length=0;}
                mounted=true;childKey=child.key;
                return child.type(child.props);
            }
            return panelProps ? module.exports.IndexSplitPanel({ ...panelProps, view: state.indexing, disabled: false, run: async (work: () => Promise<unknown>) => work() }) : module.exports.IndexingScreen(props);
        },
        async flush() { while (effects.length) { effects.shift()!(); } for (let i = 0; i < 6; i++) { await Promise.resolve(); } },
    };
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

test('Indexing tabs lead with their own task and hide details belonging to another tab', () => {
    for (const tab of ['window', 'library', 'projects'] as const) {
        const f = screen(tab, { page: page('index', indexValue), pageKind: 'indexes', detail: detail('index', indexValue), canReviewSplit: true });
        const tree = f.render(), heading = nodes(tree).find(node => node.type === 'h2');
        assert.match(text(heading), tab === 'window' ? /Search scope/ : tab === 'library' ? /Index library/ : /Index sets/);
        if (tab !== 'library') {
            assert.equal(nodes(tree).some(node => node.type?.name === 'IndexSplitPanel' && !node.props.recoveryOnly), false, tab);
            assert.equal(button(tree, /Preview rename/), undefined, tab);
        }
    }
    const library = screen('library', { detail: detail('project', projectValue) }).render();
    assert.equal(button(library, /Preview defaults|Delete index set|Preview membership/), undefined);
});

test('library rows expose opening and splitting an index without requiring a hidden Inspect prerequisite', async () => {
    const f = screen('library', { page: page('index', indexValue), pageKind: 'indexes', canReviewSplit: true });
    const tree = f.render(), open = button(tree, /^Open index$/), split = button(tree, /^Split index$/);
    assert.ok(open); assert.ok(split);
    open.props.onClick(); await f.flush();
    assert.equal(f.sent[0].type, 'indexing.detail'); assert.equal(f.sent[0].id, indexId);
    split.props.onClick(); await f.flush();
    const request = f.sent.find(message => message.type === 'indexing.split.open');
    assert.ok(request); assert.equal(request.index, indexId); assert.equal(request.generation, 1);
    assert.ok(f.sent.every(message => ['indexing.page', 'indexing.detail', 'indexing.split.open'].includes(message.type)));
});

test('opening a catalog tab reads metadata once and leaves failures for explicit retry', async () => {
    for (const tab of ['library', 'projects'] as const) {
        const f = screen(tab, { searchScope: { indexes: [], projects: [], selectedIndexIds: [], selectedProjectIds: [] } }); f.failure(true);
        f.render(); await f.flush();
        assert.equal(f.sent.length, 1, tab); assert.equal(f.sent[0].type, 'indexing.page');
        assert.equal(f.sent[0].kind, tab === 'library' ? 'indexes' : 'projects');
        for (let i = 0; i < 3; i++) { f.render(); await f.flush(); }
        assert.equal(f.sent.length, 1, 'failed automatic read must not loop');
        const retry = button(f.render(), /Refresh|Retry|Load indexes|Load index sets/);
        assert.ok(retry); f.failure(false); retry.props.onClick(); await f.flush();
        assert.equal(f.sent.length, 2); assert.equal(f.sent[1].type, 'indexing.page');
    }
});

test('automatic catalog reads respect trust, native execution, active upgrades and in-flight work', async () => {
    for (const extra of [{ trusted: false }, { nativeExecutionEnabled: false }, { runtimeUpgrade: { stage: 'activating' } }]) {
        const f = screen('library', {}, extra); f.render(); await f.flush(); assert.equal(f.sent.length, 0);
    }
    const busy = screen('projects', { busy: true, searchScope: { indexes: [], projects: [], selectedIndexIds: [], selectedProjectIds: [] } }); busy.render(); await busy.flush(); assert.equal(busy.sent.length, 0);
    busy.state.indexing.busy = false; busy.render(); await busy.flush(); assert.equal(busy.sent.length, 1);
    const selected = screen('library', { detail: detail('index', indexValue) });
    selected.render(); await selected.flush(); assert.equal(selected.sent.length, 0, 'opening a split must retain its selected detail');
});

test('empty pages remain loaded for their own tab while a different catalog tab requests its own rows', async () => {
    const empty = { status: 'catalog_page', page: { namespace, revision: 1, items: [], continuation: null } };
    const f = screen('library', { page: empty, pageKind: 'indexes', searchScope: { indexes: [], projects: [], selectedIndexIds: [], selectedProjectIds: [] } });
    let tree = f.render(); await f.flush();
    assert.equal(f.sent.length, 0); assert.match(text(tree), /No indexes on this catalog page/);
    f.props.route.tab = 'projects'; tree = f.render(); await f.flush();
    assert.equal(f.sent.length, 1); assert.equal(f.sent[0].kind, 'projects');
    assert.doesNotMatch(text(tree), /No Index sets on this catalog page/);
    f.state.indexing = { ...f.state.indexing, page: empty, pageKind: 'projects' };
    f.render(); await f.flush(); assert.equal(f.sent.length, 1, 'a successful empty page does not repeat its read');
});

test('Search scope loads local index and set choices once without an association prerequisite', async () => {
    const f = screen('window'); f.render(); await f.flush();
    assert.equal(f.sent.length, 1); assert.equal(f.sent[0].type, 'indexing.scope.read');
    f.render(); await f.flush(); assert.equal(f.sent.length, 1);
    assert.equal(button(f.render(), /Read grants|Read saved association|Use Grant/), undefined);
});

test('real WorkspaceView keeps scope Apply ownership across project and evidence-epoch changes',async()=>{
    const initial={indexes:[indexValue],projects:[projectValue],selectedIndexIds:[],selectedProjectIds:[],active:false};
    const f=screen('window',{searchScope:initial},{connection:'ready',selectedProject:'original',selectionEpoch:1},undefined,true);
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

test('index-set save retains local ownership through choices, page and detail follow-up reads',async()=>{
    const scope={indexes:[indexValue],projects:[],selectedIndexIds:[],selectedProjectIds:[],active:false};
    const f=screen('projects',{revision:1,searchScope:scope,page:page('project',projectValue),pageKind:'projects'},{connection:'ready',selectionEpoch:1});
    for(const type of ['indexing.set.save','indexing.scope.read','indexing.page','indexing.set.read'])f.hold(type);
    let tree=f.render();await f.flush();
    const name=nodes(tree).find(node=>node.type==='label'&&text(node).startsWith('Index set name'));
    nodes(name).find(node=>node.type==='input').props.onInput({currentTarget:{value:'Chosen set'}});tree=f.render();
    const form=nodes(tree).find(node=>node.type==='form'&&button(node,/^Create index set$/));assert.ok(form);
    form.props.onSubmit({preventDefault(){}});await f.flush();
    f.state.selectionEpoch=2;f.state.indexing={generation:3,busy:false};tree=f.render();await f.flush();
    assert.equal(f.sent.length,1,'owner replacement cannot start automatic choice/page reads during save');
    f.release('indexing.set.save',{status:'index_set_saved',id:projectId});await f.flush();
    assert.equal(f.sent.at(-1).type,'indexing.scope.read');f.render();await f.flush();assert.equal(f.sent.length,2);
    f.release('indexing.scope.read');await f.flush();assert.equal(f.sent.at(-1).type,'indexing.page');
    f.render();await f.flush();assert.equal(f.sent.length,3);
    f.release('indexing.page');await f.flush();assert.equal(f.sent.at(-1).type,'indexing.set.read');
    f.state.indexing={generation:4,busy:false,searchScope:scope,page:page('project',projectValue),pageKind:'projects'};
    f.release('indexing.set.read');await f.flush();tree=f.render();await f.flush();
    assert.equal(f.sent.length,4);assert.equal(button(tree,/Refresh index sets/).props.disabled,false);
});

test('catalog loading waits for a ready connection and allows one fresh missing-page read after reconnect', async () => {
    const f = screen('library', {}, { connection: 'connecting' }); f.failure(true);
    f.render(); await f.flush(); assert.equal(f.sent.length, 0, 'connecting is not a catalog attempt');
    f.state.connection = 'ready'; f.render(); await f.flush();
    assert.equal(f.sent.length, 1); assert.equal(f.sent[0].type, 'indexing.page');
    f.render(); await f.flush(); assert.equal(f.sent.length, 1, 'read failure alone cannot cause an automatic retry');
    f.state.connection = 'disconnected'; f.render(); await f.flush(); assert.equal(f.sent.length, 1);
    f.state.connection = 'ready'; f.failure(false); f.render(); await f.flush();
    assert.equal(f.sent.length, 2, 'a reconnect can load a page that was never obtained');
    f.state.indexing = { ...f.state.indexing, page: page('index', indexValue), pageKind: 'indexes' };
    f.render(); await f.flush();
    f.state.connection = 'disconnected'; f.render(); await f.flush();
    f.state.connection = 'ready'; f.render(); await f.flush();
    assert.equal(f.sent.length, 2, 'a retained matching page is not fetched again merely for reconnect');
});

test('a completed runtime upgrade can load the missing catalog without looping on read failure', async () => {
    const f = screen('projects', { searchScope: { indexes: [], projects: [], selectedIndexIds: [], selectedProjectIds: [] } }, { connection: 'ready' }); f.failure(true);
    f.render(); await f.flush(); assert.equal(f.sent.length, 1);
    f.state.runtimeUpgrade = { toVersion: '1.8.9', stage: 'checking' };
    f.render(); await f.flush(); assert.equal(f.sent.length, 1, 'runtime checking defers catalog reads');
    f.state.runtimeUpgrade.stage = 'ready'; f.render(); await f.flush();
    assert.equal(f.sent.length, 2); assert.equal(f.sent[1].kind, 'projects');
    f.render(); await f.flush(); assert.equal(f.sent.length, 2, 'failure after upgrade still requires explicit retry');
});

test('complete native export remains available with fields from every retained result', () => {
    const f = screen('library', { page: page('index', indexValue), pageKind: 'indexes', outcome: { future_native_observation: 'retain-me' } });
    const tree = f.render();
    assert.ok(button(tree, /Copy complete JSON/)); assert.ok(button(tree, /Export JSON/));
    assert.ok(button(tree, /Copy complete Markdown/)); assert.ok(button(tree, /Export Markdown/));
    assert.match(text(tree), /future_native_observation/); assert.match(text(tree), /retain-me/);
});

test('split review guides baseline, source selection and destination steps while retaining the complete native review', async () => {
    const fixture = splitFixture(), f = screen('library', { canReviewSplit: true, revision: 1 }, {}, { index: { id: fixture.index, tombstone: false } });
    let tree = f.render(); await f.flush();
    assert.match(text(tree), /1\. Load the source folders/);
    assert.doesNotMatch(text(tree), /2\. Choose what moves|Destination name/);
    const baseline = button(tree, /Read complete native split baseline/); assert.ok(baseline);
    baseline.props.onClick(); await f.flush(); assert.equal(f.sent[0].type, 'indexing.split.sourceRead');
    f.state.indexing.splitSourceState = splitSourceResponse(fixture);
    tree = f.render(); await f.flush();
    assert.match(text(tree), /2\. Choose what moves to the new index/);
    assert.match(text(tree), /3\. Choose the destination and review limits/);
    const source = nodes(tree).find(node => node.type === 'select' && nodes(node).some(option => option.type === 'option' && option.props.value === fixture.source));
    assert.ok(source, 'native source roots are selectable by their readable paths'); assert.match(text(source), /\/source/);
    assert.ok(button(tree, /Prepare complete native split review/)); assert.equal(button(tree, /Apply reviewed split/), undefined);
    f.state.indexing.splitReview = { response: { draft: fixture.draft }, draft: fixture.draft, selection: fixture.choice };
    tree = f.render(); assert.ok(button(tree, /Apply reviewed split/)); assert.match(text(tree), /future_native_draft/);
});

test('the same keyed split form remains mounted when switching selected-index tasks', async () => {
    const f = screen('library', { detail: detail('index', indexValue) });
    const selectedPanel = (tree: any) => nodes(tree).find(node => node.type?.name === 'IndexSplitPanel' && !node.props.recoveryOnly);
    const wrapper = (tree: any, panel: any) => nodes(tree).find(node => node.type === 'div' && nodes(node.props.children).includes(panel));
    let tree = f.render(); await f.flush();
    let panel = selectedPanel(tree); assert.ok(panel); assert.equal(panel.key, indexId); assert.equal(wrapper(tree, panel).props.hidden, true);
    button(tree, /^Split index$/).props.onClick(); tree = f.render(); await f.flush();
    panel = selectedPanel(tree); assert.equal(panel.key, indexId); assert.equal(wrapper(tree, panel).props.hidden, false);
    button(tree, /^Sources and exclusions$/).props.onClick(); tree = f.render();
    panel = selectedPanel(tree); assert.equal(panel.key, indexId); assert.equal(wrapper(tree, panel).props.hidden, true);
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

test('library Delete index reads current detail before preparing native retirement and never applies implicitly', async () => {
    const f=screen('library',{page:page('index',indexValue),pageKind:'indexes',canReviewSource:true});
    button(f.render(),/^Delete index$/).props.onClick();await f.flush();
    assert.deepEqual(f.sent.map(message=>message.type),['indexing.detail','indexing.source.draft']);
    assert.equal(f.sent[0].id,indexId);assert.equal(f.sent[1].change.operation,'retire_index');
    assert.equal(f.sent.some(message=>message.type==='indexing.apply'),false);
});

test('Delete index review explains source preservation and requires the explicit delete action', async () => {
    const review={request:{operation:{operation:'retire_index',id:indexId}},response:{preview:{},future_native_field:'retained'}};
    const f=screen('library',{page:page('index',indexValue),pageKind:'indexes',review});
    const tree=f.render();assert.match(text(tree),/Original source files are preserved/);assert.match(text(tree),/new index storage folder/);
    assert.match(text(tree),/future_native_field/);assert.ok(button(tree,/^Delete index and stop maintenance$/));
    button(tree,/^Delete index and stop maintenance$/).props.onClick();await f.flush();
    assert.ok(f.sent.some(message=>message.type==='indexing.apply'));assert.equal(f.sent.at(-1).type,'indexing.page');
});


test('index sets use readable index checkboxes instead of identifiers and revision forms', async () => {
    const f=screen('projects',{page:page('project',projectValue),pageKind:'projects',detail:detail('project',projectValue),setEditor:{id:projectId,name:projectValue.name,indexIds:[indexId],complete:true,responses:[]},memberships:page('membership',{project_id:projectId,index_id:indexId,revision:1,scope:[]}),searchScope:{indexes:[indexValue],projects:[projectValue],selectedIndexIds:[],selectedProjectIds:[],managedProjectIds:[]}});
    f.render();await f.flush();let tree=f.render();
    assert.ok(button(tree,/^Save index set$/));assert.equal(nodes(tree).some(node=>node.type==='input'&&node.props.placeholder==='idx_…'),false);
    assert.doesNotMatch(text(tree),/Current membership revision|Source ID|Default index IDs/);
    const form=nodes(tree).find(node=>node.type==='form'&&button(node,/^Save index set$/));assert.ok(form);
    form.props.onSubmit({preventDefault(){}});await f.flush();
    const saved=f.sent.find(message=>message.type==='indexing.set.save');
    assert.equal(saved.selection.id,projectId);assert.equal(saved.selection.name,projectValue.name);
    assert.deepEqual(JSON.parse(JSON.stringify(saved.selection.index_ids)),[indexId]);
});


test('Search scope hides internal support sets and marks edited choices as unapplied', async () => {
    const hidden='prj_'+'99'.repeat(16);
    const f=screen('window',{searchScope:{indexes:[indexValue],projects:[projectValue,{id:hidden,name:'internal'}],selectedIndexIds:[],selectedProjectIds:[],managedProjectIds:[hidden],active:true}});
    f.render();await f.flush();let tree=f.render();
    assert.doesNotMatch(text(nodes(tree).filter(node=>node.type==='fieldset')),/internal/);assert.match(text(tree),/Selection active/);
    nodes(tree).find(node=>node.type==='input'&&node.props.type==='checkbox').props.onChange({currentTarget:{checked:true}});
    tree=f.render();assert.match(text(tree),/Changes not applied/);assert.doesNotMatch(text(tree),/Selection active/);
});

test('existing index sets cannot be saved until complete membership was read', () => {
    const f=screen('projects',{detail:detail('project',projectValue),searchScope:{indexes:[indexValue],projects:[projectValue],selectedIndexIds:[],selectedProjectIds:[]}});
    const tree=f.render();assert.equal(button(tree,/^Save index set$/).props.disabled,true);
    assert.match(text(tree),/Complete index set membership/);
});


test('completed deletion reports logical success without inventing physical cleanup completion', () => {
    const submittedReview={request:{operation:{operation:'retire_index',id:indexId}},response:{preview:{before:[{kind:'index',value:indexValue}]}}};
    const tree=screen('library',{submittedReview,outcome:{status:'catalog_applied'}}).render();
    assert.match(text(tree),/Application index was deleted from search and maintenance/);
    assert.match(text(tree),/completion has not been observed here/);
    assert.ok(nodes(tree).find(node=>node.type==='summary'&&text(node)==='Complete operation result'));
});
