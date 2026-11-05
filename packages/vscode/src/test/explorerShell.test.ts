// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import Module from 'node:module';
import * as path from 'node:path';
import { test } from 'node:test';
import type { ExplorerViewState } from '../explorerContracts';
import { FEATURES } from '../explorerFeatures';
import { HandoffStore } from '../explorerHandoffs';
import { IndexingCatalogController } from '../indexingCatalogController';

// Minimal vscode stub: panels record HTML/messages and expose their message handler.
const disposable = { dispose() {} };
const panels: any[] = [];
const executedCommands:string[]=[];
const vscodeStub = {
    commands:{executeCommand:async(name:string)=>{executedCommands.push(name);}},
    env: {clipboard:{writeText:async(_text:string)=>{}}},
    workspace: { textDocuments: [] as any[] },
    Uri: { file: (fsPath: string) => ({ scheme: 'file', fsPath }), joinPath: (base: any, ...parts: string[]) => ({ path: [base.path, ...parts].join('/'), toString() { return `vscode-resource:${this.path}`; } }) },
    ViewColumn: { Active: 1 },
    EventEmitter: class { event = () => disposable; fire() {} dispose() {} },
    window: {
        tabGroups: { all: [] as any[] },
        showWarningMessage: async (): Promise<string | undefined> => undefined,
        showOpenDialog: async (): Promise<any[] | undefined> => undefined,
        showQuickPick: async (_items: any[]): Promise<any> => undefined,
        createWebviewPanel: (_type: string, _title: string, _column: unknown, options: any) => {
            let receive: (value: unknown) => void = () => undefined;
            let closed: () => void = () => undefined;
            const panel = {
                options, posted: [] as any[], disposed: false,
                webview: { html: '', cspSource: 'vscode-resource:', asWebviewUri: (uri: any) => uri, postMessage: async (message: unknown) => { panel.posted.push(structuredClone(message)); return true; },
                    onDidReceiveMessage: (handler: (value: unknown) => void) => { receive = handler; return disposable; } },
                onDidChangeViewState: () => disposable,
                onDidDispose: (handler: () => void) => { closed = handler; return disposable; },
                reveal() {}, dispose() { panel.disposed = true; closed(); },
                send: (value: unknown) => receive(value),
            };
            panels.push(panel); return panel;
        },
    },
};
const load = (Module as any)._load;
(Module as any)._load = function (request: string, ...rest: unknown[]) { return request === 'vscode' ? vscodeStub : load.call(this, request, ...rest); };
// eslint-disable-next-line @typescript-eslint/no-require-imports
const { ExplorerController } = require('../explorerController') as typeof import('../explorerController');
const { explorerHtml } = require('../explorerPanel') as typeof import('../explorerPanel');
const { ExplorerSidebar, renderSidebar, sidebarContent, sidebarMessage } = require('../explorerSidebar') as typeof import('../explorerSidebar');
const { SuggestionStore } = require('../explorerSuggestions') as typeof import('../explorerSuggestions');

const project = 'ab'.repeat(32);
test('sharing requires explicit confirmation and rechecks the draft revision after the modal', async t => {
    let revision = 2, approved = false, changeDuringDialog = false;
    const handoffs = new HandoffStore({ authorized: () => true, binding: () => 'ab'.repeat(8), revisions: () => ({ studio: revision }), publication: () => undefined });
    const { explorer } = controller(true, new Map(), handoffs);
    t.mock.method(vscodeStub.window, 'showWarningMessage', async () => { if (changeDuringDialog) { revision++; } return approved ? 'Share snapshot' : undefined; });
    const request = { v: 1, requestId: 'share', type: 'handoff.share', kind: 'studio', revision: 2, content: 'Complete reviewed instructions' };
    try {
        assert.deepEqual(await (explorer as any).dispatch(request), { shared: false });
        approved = true; const shared = await (explorer as any).dispatch(request);
        assert.equal(handoffs.get(shared.token)?.content, request.content);
        changeDuringDialog = true;
        await assert.rejects((explorer as any).dispatch(request), /draft, authorization or evidence changed/);
        assert.equal(handoffs.get(shared.token), undefined);
    } finally { explorer.dispose(); }
});
function controller(trusted = true, stored = new Map<string, unknown>(), handoffs?: HandoffStore) {
    let specsListener=()=>{};
    const suggestions = new SuggestionStore({ binding: () => 'ab12ab12ab12ab12', revisions: () => ({}), publication: () => undefined, ttlMs: () => 3600_000 });
    const statusBar: string[] = [];
    const runs: unknown[] = [];
    const consoleState: any = { trusted, connected: true, busy: false, health: { state: 'live', checkedAt: 1, value: { observation: { policy: { accepting: true } } } },
        version:'1.5.0-test',authority:'Local execution host',responseMode:'terse',selected:new Set([project]),pending:[],events:[],historyOmitted:false,
        resources:{state:'unavailable',checkedAt:1},model:{state:'unavailable',checkedAt:1},sources:{state:'unavailable',checkedAt:1},
        bootstrap: { state: 'live', checkedAt: 1, value: {} }, currentProjects: new Set([project]),
        page: { revision: 1, cached: true, observed_unix_ms: 7, after: null, projects: [{ project: { registration: { binding: project, root: '/src/demo', collection: 'demo' } } }] } };
    const host = {
        extensionUri: { path: '/ext' } as any, version: '1.5.0-test', trusted: () => trusted,workspaceTrusted:()=>trusted,
        specs: () => [{ command: '/launcher', root: '/src/demo', dataDir: '/data', collection: 'demo', name: 'demo', service: { installDir: '/install', project } }],
        activeProject: () => project, onDidChangeSpecs: (listener:()=>void) => {specsListener=listener;return disposable;}, log: () => undefined,
        setStatusBar: (text: string) => { statusBar.push(text); },
        workspaceState: { get: <T>(key: string, fallback?: T) => (stored.has(key) ? stored.get(key) : fallback) as T, update: async (key: string, value: unknown) => { stored.set(key, value); } },
        suggestions, handoffs,
        control: { controller: { state: consoleState }, run: async (action: string, fields?: unknown) => { runs.push([action, fields]); },
            retain: () => disposable, onDidChangeState: () => disposable },
    };
    class TestController extends ExplorerController {
        protected override session(): any {
            return { call: async (tool: string, args: any) => {
                if (tool === 'airk_search') {
                    assert.deepEqual(args, { query: 'x', limit: 5, max_bytes: 8192, full_chunks: false, mode: 'lexical' });
                    return { publication: 'airk-r1:t', payload: { truncated: false, publication: { token: 'airk-r1:t' },
                        hits: [{ citation: { path: 'a.ts', start_line: 2, end_line: 4, content_hash: 'cd'.repeat(32) }, text: 'x', truncated: false }] } };
                }
                assert.ok(tool === 'airk_status' || tool === 'airk_diagnostics', 'only scalar readiness or explicit details are expected');
                return { payload: { index_state: 'available', publication: { token: 'airk-r1:t' }, semantic_readiness: { state: 'ready' }, freshness: { state: 'idle' } } };
            }, dispose() {} };
        }
    }
    return { explorer: new TestController(host as any), statusBar, runs, suggestions, host,fireSpecs:()=>specsListener() };
}
const flush = () => new Promise(resolve => setTimeout(resolve, 5));

test('Recommendations copy and temporary scope use revalidate native scope and never mutate catalog or configuration',async t=>{
    const f=controller(),instance=f.explorer as any,calls:unknown[]=[],copied:string[]=[],applied:unknown[]=[];
    const projects=['prj_'+'01'.repeat(16)],indexes=['idx_'+'02'.repeat(16)];
    let current={schema:'airk.recommendations/1',scope_identity:'scope',catalog_revision:2,selected:{project_ids:projects,index_ids:indexes,member_availability:'require'},instructions:{status:'available',text:'inert native scope instructions'}};
    instance.indexing=new IndexingCatalogController({identity:()=> 'one',authorized:()=>f.host.trusted(),changed(){},control:{catalogRead:async()=>({}),catalogPrepare:async()=>{throw new Error('unexpected mutation');},catalogApply:async()=>{throw new Error('unexpected mutation');},catalogReconcile:async()=>[],catalogPending:()=>[]}});
    instance.session=()=>({call:async(tool:string,args:unknown)=>{assert.equal(tool,'airk_query_scope');calls.push(structuredClone(args));return {payload:structuredClone(current)};}});
    (f.host as any).prefillRecommendedSearch=async(search:unknown)=>{applied.push(search);};
    t.mock.method(vscodeStub.env.clipboard,'writeText',async(text:string)=>{copied.push(text);});
    try{
        await instance.dispatch({type:'indexing.recommendations',arguments:{situation:'shared_dependencies'}});
        assert.equal(copied.length,0);assert.equal(applied.length,0);
        await instance.dispatch({type:'indexing.recommendations.copy'});assert.deepEqual(copied,['inert native scope instructions']);
        await instance.dispatch({type:'indexing.recommendations.use'});
        assert.deepEqual(applied,[{query:'',mode:'lexical',limit:12,maxBytes:8192,fullChunks:false,project_ids:projects,index_ids:indexes,member_availability:'require'}]);
        assert.equal(calls.length,3);assert.deepEqual(f.runs,[]);
        current={...current,catalog_revision:3};
        await assert.rejects(instance.dispatch({type:'indexing.recommendations.copy'}),/changed/);assert.equal(copied.length,1);
        f.host.trusted=()=>false;await assert.rejects(instance.dispatch({type:'indexing.recommendations.use'}));assert.equal(applied.length,1);
    }finally{f.explorer.dispose();}
});

test('registered spec-change listener fences same-backing scoped identity even before an evidence client exists',()=>{
    const f=controller(),before=f.explorer.snapshot().selectionEpoch,base=f.host.specs()[0];
    assert.ok(before!==undefined,'the registered controller has an initial selected-project epoch');
    try{
        f.host.specs=()=>[{...base,catalogSession:{schema:1,context_key:'11'.repeat(32),namespace:'22'.repeat(16),expected_catalog_revision:1,grant_id:'grt_'+'33'.repeat(16),expected_grant_revision:1,index_id:'idx_'+'44'.repeat(16),project_ids:['prj_'+'55'.repeat(16)]}}];
        f.fireSpecs();assert.equal(f.explorer.snapshot().selectionEpoch,before+1);assert.equal(f.explorer.snapshot().selectedProject,project);
        f.fireSpecs();assert.equal(f.explorer.snapshot().selectionEpoch,before+1,'identical scoped specs do not repeatedly reset the session');
    }finally{f.explorer.dispose();}
});

test('actual Explorer distinguishes Restricted Mode, revoked authority and native-off while trust review stays inert',async()=>{
    const f=controller(false),instance=f.explorer as any;
    try{
        assert.equal(f.explorer.snapshot().workspaceTrusted,false);assert.equal(f.explorer.snapshot().connection,'untrusted');
        await assert.rejects(instance.dispatch({type:'indexing.context.read'}),/Restricted Mode/);
        const before=executedCommands.length;await instance.dispatch({type:'workspace.manageTrust'});
        assert.deepEqual(executedCommands.slice(before),['workbench.trust.manage']);assert.deepEqual(f.runs,[]);
        f.host.workspaceTrusted=()=>true;f.host.trusted=()=>false;
        assert.equal(f.explorer.snapshot().connection,'disconnected');
        await assert.rejects(instance.dispatch({type:'indexing.context.read'}),/client is no longer authorized/);
        await assert.rejects(instance.dispatch({type:'workspace.manageTrust'}),/client is no longer authorized/);
        f.host.trusted=()=>true;(f.host as any).nativeExecutionEnabled=()=>false;
        await assert.rejects(instance.dispatch({type:'indexing.context.read'}),/native_execution_disabled/);
        await instance.dispatch({type:'workspace.manageTrust'});assert.deepEqual(f.runs,[]);
        f.explorer.dispose();await assert.rejects(instance.dispatch({type:'workspace.manageTrust'}),/client is no longer authorized/);
    }finally{f.explorer.dispose();}
});

test('same-panel ready resends the complete retained catalog while scalar updates do not',async()=>{
    const f=controller(),nativePage={status:'catalog_page',page:{namespace:'aa'.repeat(16),revision:1,items:[],continuation:null,native_metadata:{retained:'complete'}}};
    let reads=0;
    const indexing=new IndexingCatalogController({authorized:()=>true,identity:()=> 'one-owner',changed:()=> (f.explorer as any).publish(),control:{
        catalogRead:async()=>{reads++;return nativePage;},catalogPrepare:async()=>{throw new Error('unexpected preview');},catalogApply:async()=>{throw new Error('unexpected apply');},catalogReconcile:async()=>[],catalogPending:()=>[],
    }});
    (f.explorer as any).indexing=indexing;
    try{
        f.explorer.open('#indexing/projects');const panel=panels.at(-1);panel.send({v:1,type:'ready',requestId:'ready'});await flush();
        await indexing.page('projects');await flush();
        const initial=panel.posted.filter((message:any)=>message.type==='indexing.session').length;
        assert.deepEqual(panel.posted.filter((message:any)=>message.type==='indexing.session').at(-1).session.page,nativePage);
        (f.explorer as any).publish();await flush();
        assert.equal(panel.posted.filter((message:any)=>message.type==='indexing.session').length,initial);
        assert.ok(panel.posted.filter((message:any)=>message.type==='state').every((message:any)=>message.state.indexing===undefined));
        panel.send({v:1,type:'ready',requestId:'ready-again'});await flush();
        assert.equal(panel.posted.filter((message:any)=>message.type==='indexing.session').length,initial+1);
        assert.deepEqual(panel.posted.filter((message:any)=>message.type==='indexing.session').at(-1).session.page,nativePage);
        assert.equal(reads,1,'ready and scalar updates do not replay a catalog read');
    }finally{f.explorer.dispose();}
});

test('refresh confirms unchanged observations and reports disconnection instead of silent success',async()=>{
    const f=controller();
    try{
        let result=await (f.explorer as any).dispatch({type:'manage.action',action:'refresh'});
        assert.match(result.message,/Status checked at/);assert.match(result.message,/No change in project work or settings/);
        assert.match(result.message,/does not restart or repair/);assert.equal(f.runs.length,1);
        f.host.control.controller.state.connected=false;
        result=await (f.explorer as any).dispatch({type:'manage.action',action:'refresh'});
        assert.match(result.message,/Could not refresh/);assert.match(result.message,/Retained observations/);
    }finally{f.explorer.dispose();}
});

test('copy panel details uses cached metadata, rejects foreign projects and requires trust',async t=>{
    const copied:string[]=[];t.mock.method(vscodeStub.env.clipboard,'writeText',async (text:string)=>{copied.push(text);});
    const f=controller();
    try{
        await (f.explorer as any).dispatch({type:'details.copy',section:'activity',project});
        assert.equal(copied.length,1);assert.match(copied[0],/Worker progress/);assert.equal(f.runs.length,0);
        await assert.rejects((f.explorer as any).dispatch({type:'details.copy',section:'activity',project:'ff'.repeat(32)}),/no longer available/);
        assert.equal(copied.length,1);
        f.host.trusted=()=>false;
        await assert.rejects((f.explorer as any).dispatch({type:'details.copy',section:'service'}));assert.equal(copied.length,1);
    }finally{f.explorer.dispose();}
});

test('pending update is included in the view and reload dispatch is restricted to a pending update',async()=>{
    const f=controller(),host=f.host as any;let reloads=0;
    host.reloadUpdatedExtension=async()=>{reloads++;};
    try{
        await (f.explorer as any).dispatch({type:'extension.reload'});assert.equal(reloads,0);
        host.extensionUpdate=()=>({runningVersion:'1.5.8',installedVersion:'1.5.9'});
        assert.deepEqual(f.explorer.snapshot().extensionUpdate,{runningVersion:'1.5.8',installedVersion:'1.5.9'});
        await (f.explorer as any).dispatch({type:'extension.reload'});assert.equal(reloads,1);
    }finally{f.explorer.dispose();}
});

test('visible Explorer observations refresh readiness even when the publication token has not changed',()=>{
    const instance=Object.create(ExplorerController.prototype) as any;let reads=0,publishes=0;
    Object.assign(instance,{retained:disposable,panel:{open:true},selected:project,status:{publication:'same'},lastAutomaticStatusCheck:0,
        control:{projects:()=>[{project,publication:'same'}]},loadStatus:async()=>{reads++;},publish:()=>{publishes++;}});
    instance.observedControl();instance.observedControl();assert.equal(reads,1);
    instance.lastAutomaticStatusCheck=Date.now()-6000;instance.observedControl();assert.equal(reads,2);
    instance.retained=undefined;instance.lastAutomaticStatusCheck=0;instance.observedControl();assert.equal(reads,2);
    assert.equal(publishes,4);
});

test('repair approval uses the exact preview shown by the webview and never silently accepts a replacement',async()=>{
    const f=controller(),id='de'.repeat(32),preview='ef'.repeat(32);
    try{
        f.host.control.controller.state.plan={value:{id,preview_digest:preview},items:[],after:null};
        const stale=await (f.explorer as any).dispatch({type:'manage.action',action:'approvePlan',allowFallback:false,expectedPlan:id,expectedPreview:'ff'.repeat(32)});
        assert.match(stale.message,/preview changed/);assert.equal(f.runs.length,0);
        await (f.explorer as any).dispatch({type:'manage.action',action:'approvePlan',allowFallback:false,expectedPlan:id,expectedPreview:preview});
        assert.deepEqual(f.runs,[['approvePlan',{allowFallback:false}]]);
    }finally{f.explorer.dispose();}
});

test('embedded usage pins its project session and rejects invalid actions and obsolete bindings', async () => {
    const { explorer, host } = controller(); let opens = 0, disposals = 0;
    const guards: (() => boolean)[] = [], actions: unknown[] = [];
    (host as any).usage = async (_spec: unknown, valid: () => boolean) => {
        opens++; guards.push(valid);
        return { view: () => ({ html: '<p>Scoped report</p>' }), action: async (message: unknown) => { assert.ok(valid()); actions.push(message); return { html: '<p>Updated</p>' }; }, dispose: () => { disposals++; } };
    };
    try {
        assert.deepEqual(await (explorer as any).dispatch({ type: 'usage.read' }), { html: '<p>Scoped report</p>' });
        await (explorer as any).dispatch({ type: 'usage.read' }); assert.equal(opens, 1);
        await assert.rejects((explorer as any).dispatch({ type: 'usage.action', action: 'days', days: 8 }));
        assert.equal(actions.length, 0);
        await (explorer as any).dispatch({ type: 'usage.action', action: 'days', days: 7 });
        assert.deepEqual(actions, [{ type: 'usage', action: 'days', days: 7 }]);
        (explorer as any).select(undefined); assert.equal(guards[0](), false);
        (explorer as any).select(project); await (explorer as any).dispatch({ type: 'usage.read' });
        assert.equal(opens, 2); await flush(); assert.equal(disposals, 1);
    } finally { explorer.dispose(); }
});

test('search results, drafts and page survive closing Explorer without workspace writes', async () => {
    const stored = new Map<string, unknown>(), { explorer } = controller(true, stored);
    try {
        explorer.open('#search/query'); const first = panels.at(-1);
        first.send({ v: 1, type: 'ready', requestId: 'ready' }); await flush();
        const acknowledgment = await (explorer as any).dispatch({ v: 1, type: 'search.run', requestId: 'run', query: 'x', mode: 'lexical', limit: 5, maxBytes: 8192, fullChunks: false });
        assert.deepEqual(acknowledgment, { completed: true });
        const result = first.posted.filter((entry: any) => entry.type === 'search.session' && entry.session.result).at(-1).session.result;
        const epoch = explorer.snapshot().selectionEpoch!;
        await (explorer as any).dispatch({ v: 1, type: 'search.form', requestId: 'draft', project, selectionEpoch: epoch,
            query: 'unsent edit', mode: 'hybrid', pathPrefix: 'src/', limit: 100, maxBytes: 16384, fullChunks: true });
        first.dispose(); explorer.open(); const reopened = panels.at(-1);
        reopened.send({ v: 1, type: 'ready', requestId: 'ready' }); await flush();
        const restored = reopened.posted.find((entry: any) => entry.type === 'search.session').session;
        assert.equal(restored.form.query, 'unsent edit'); assert.deepEqual(restored.result, result);
        assert.equal((explorer as any).refs.get(restored.result.hits[0].ref, project).path, 'a.ts');
        assert.equal(stored.size, 0, 'session snippets and query drafts are not written to workspace Memento');
        await assert.rejects((explorer as any).dispatch({ v: 1, type: 'search.page', requestId: 'page', project, selectionEpoch: epoch - 1, page: 1 }), /newer request/);
        (explorer as any).select(undefined);
        reopened.dispose(); explorer.open(); const changed = panels.at(-1);
        changed.send({ v: 1, type: 'ready', requestId: 'ready' }); await flush();
        assert.ok(!changed.posted.some((entry: any) => entry.type === 'search.session'), 'a new project/binding does not restore old evidence');
    } finally { explorer.dispose(); }
});

test('a search finishing after panel recreation restores results without replaying its old request ID', async () => {
    const { explorer } = controller();
    let finish!: (result: unknown) => void;
    try {
        explorer.open('#search/query'); const first = panels.at(-1);
        first.send({ v: 1, type: 'ready', requestId: 'ready' }); await flush();
        (explorer as any).session = () => ({ call: (tool: string) => tool === 'airk_search' ? new Promise(resolve => { finish = resolve; })
            : Promise.resolve({ payload: { index_state: 'available', publication: { token: 'airk-r1:t' } } }) });
        first.send({ v: 1, type: 'search.run', requestId: 'w1', query: 'x', mode: 'lexical', limit: 5, maxBytes: 8192, fullChunks: false });
        first.dispose(); explorer.open(); const reopened = panels.at(-1);
        reopened.send({ v: 1, type: 'ready', requestId: 'ready' }); await flush();
        assert.equal(reopened.posted.find((entry: any) => entry.type === 'search.session').session.busy, true);
        finish({ publication: 'airk-r1:t', payload: { hits: [{ citation: { path: 'late.ts', start_line: 1, end_line: 1, content_hash: 'cd'.repeat(32) }, text: 'late result' }] } });
        await flush();
        const restored = reopened.posted.filter((entry: any) => entry.type === 'search.session').at(-1).session;
        assert.equal(restored.busy, false); assert.equal(restored.result.hits[0].text, 'late result');
        assert.ok(!reopened.posted.some((entry: any) => entry.type === 'result' && entry.requestId === 'w1'));
        assert.equal([...first.posted, ...reopened.posted].filter((entry: any) => entry.type === 'search.session' && entry.session.result).length, 1,
            'late authorized evidence is delivered once to the current panel, with no old completion response');
    } finally { explorer.dispose(); }
});

test('the host restores the selected search page and resets it for a replacement query', async () => {
    const { explorer } = controller();
    try {
        explorer.open('#search/query'); panels.at(-1).send({ v: 1, type: 'ready', requestId: 'ready' }); await flush();
        (explorer as any).session = () => ({ call: async (tool: string) => tool === 'airk_search'
            ? { publication: 'airk-r1:t', payload: { hits: Array.from({ length: 45 }, (_, at) => ({ citation: { path: `file-${at}.ts`, start_line: 1, end_line: 1, content_hash: 'cd'.repeat(32) }, text: `hit ${at}` })) } }
            : { payload: { index_state: 'available', publication: { token: 'airk-r1:t' } } } });
        const run = { v: 1, type: 'search.run', requestId: 'run', query: 'x', mode: 'lexical', limit: 100, maxBytes: 65536, fullChunks: false };
        await (explorer as any).dispatch(run);
        await (explorer as any).dispatch({ v: 1, type: 'search.page', requestId: 'page', project, selectionEpoch: explorer.snapshot().selectionEpoch, page: 4 });
        panels.at(-1).dispose(); explorer.open(); const reopened = panels.at(-1);
        reopened.send({ v: 1, type: 'ready', requestId: 'ready' }); await flush();
        assert.equal(reopened.posted.find((entry: any) => entry.type === 'search.session').session.page, 2, 'page is clamped to the final retrieved page');
        await (explorer as any).dispatch(run);
        assert.equal(reopened.posted.filter((entry: any) => entry.type === 'search.session').at(-1).session.page, 0);
    } finally { explorer.dispose(); }
});

test('Inspector without a citation uses only the host-selected file in the selected project', async () => {
    const { RefRegistry } = require('../explorerSearch') as typeof import('../explorerSearch');
    const instance = Object.create(ExplorerController.prototype) as any;
    const calls: unknown[] = [];
    Object.assign(instance, { selected: project, sourceFiles: new Map([[project, 'chosen.ts']]), refs: new RefRegistry(),
        requireProject: () => project, session: () => ({ call: async (tool: string, args: unknown) => {
            calls.push([tool, args]); return { publication: 'p', payload: { content_hash: 'ab'.repeat(32) } };
        } }) });
    const view = await instance.dispatch({ type: 'evidence.inspect' });
    assert.equal(view.path, 'chosen.ts');
    assert.deepEqual(calls, [['airk_file_info', { path: 'chosen.ts', response_mode: 'full' }]]);
    instance.selected = 'other-project';
    await assert.rejects(instance.dispatch({ type: 'evidence.inspect' }), /source file/);
    assert.equal(calls.length, 1);
});

test('source context survives Explorer focus and stays isolated by project', async t => {
    const rootA = path.resolve('source-a'), rootB = path.resolve('source-b');
    const instance = Object.create(ExplorerController.prototype) as any;
    Object.assign(instance, { selected: 'A', selectionEpoch: 1, sourceFiles: new Map(), publish() {}, host: { trusted: () => true,
        specs: () => [{ root: rootA, service: { project: 'A' } }, { root: rootB, service: { project: 'B' } }] } });
    const editor = (file: string) => ({ document: { uri: { scheme: 'file', fsPath: file } } });
    assert.equal(instance.rememberSource(editor(path.join(rootA, 'src', 'one.ts'))), true);
    instance.rememberSource(undefined);
    assert.equal(instance.activeRelativePath(), 'src/one.ts', 'webview focus does not erase the target');
    instance.selected = 'B'; assert.equal(instance.activeRelativePath(), undefined);
    instance.rememberSource(editor(path.join(rootB, 'two.ts'))); assert.equal(instance.activeRelativePath(), 'two.ts');
    instance.selected = 'A'; assert.equal(instance.activeRelativePath(), 'src/one.ts');
    t.mock.method(vscodeStub.window, 'showQuickPick', async (items: any[]) => items.at(-1));
    t.mock.method(vscodeStub.window, 'showOpenDialog', async () => [{ scheme: 'file', fsPath: path.join(rootB, 'foreign.ts') }]);
    await assert.rejects(instance.dispatch({ type: 'source.choose' }), /Choose a file inside/);
    t.mock.method(vscodeStub.window, 'showOpenDialog', async () => [{ scheme: 'file', fsPath: path.join(rootA, 'chosen.ts') }]);
    await instance.dispatch({ type: 'source.choose' }); assert.equal(instance.activeRelativePath(), 'chosen.ts');
    t.mock.method(vscodeStub.window, 'showOpenDialog', async () => { instance.selectionEpoch++; return [{ scheme: 'file', fsPath: path.join(rootA, 'stale.ts') }]; });
    await assert.rejects(instance.dispatch({ type: 'source.choose' }), /binding changed/);
    assert.equal(instance.activeRelativePath(), 'chosen.ts');
    vscodeStub.window.tabGroups.all = [{ tabs: [{ input: { uri: { scheme: 'file', fsPath: path.join(rootB, 'open-tab.ts') } } }] }];
    t.after(() => { vscodeStub.window.tabGroups.all = []; });
    instance.select = (selected: string) => { instance.selected = selected; };
    t.mock.method(vscodeStub.window, 'showQuickPick', async (items: any[]) => { assert.match(items[0].detail, /switches the selected project/); return items[0]; });
    await instance.dispatch({ type: 'source.choose' });
    assert.equal(instance.selected, 'B'); assert.equal(instance.activeRelativePath(), 'open-tab.ts');
});

test('Help offers native context and general choices without retrieving evidence', async t => {
    const instance = Object.create(ExplorerController.prototype) as any;
    t.mock.method(vscodeStub.window, 'showQuickPick', async (items: any[]) => { assert.equal(items[0].label, 'Help for Code › Outline'); return items[0]; });
    assert.equal(await instance.dispatch({ type: 'help.choose', route: 'code/outline' }), 'screen');
    t.mock.method(vscodeStub.window, 'showQuickPick', async (items: any[]) => items[1]);
    assert.equal(await instance.dispatch({ type: 'help.choose', route: 'code/outline' }), 'general');
    t.mock.method(vscodeStub.window, 'showQuickPick', async () => undefined);
    assert.equal(await instance.dispatch({ type: 'help.choose', route: 'code/outline' }), null);
});

function sidebarView() {
    let visibility = () => {}, disposed = () => {}, receive = (_value: unknown) => {};
    const view = { visible: true, webview: { options: {}, html: '', posted: [] as any[],
        postMessage: async (value: any) => { view.webview.posted.push(value); return true; },
        onDidReceiveMessage: (listener: typeof receive) => { receive = listener; return disposable; } },
        onDidChangeVisibility: (listener: () => void) => { visibility = listener; return disposable; },
        onDidDispose: (listener: () => void) => { disposed = listener; return disposable; } };
    return { view, visible: (value: boolean) => { view.visible = value; visibility(); },
        send: (value: unknown) => receive(value), dispose: () => disposed() };
}

test('activity entry opens Explorer once per reveal, even without a service or trusted snapshot', async () => {
    const fixture = sidebarView(); let opens = 0, hides = 0, released = 0;
    let finish!: () => void;
    const sidebar = new ExplorerSidebar({ commandUris: [], folders: () => 1,
        attach: async () => ({ dispose: () => { released++; } }),
        open: () => { opens++; return new Promise<void>(resolve => { finish = resolve; }); },
        hide: async () => { hides++; fixture.visible(false); }, search: () => {} });
    sidebar.resolveWebviewView(fixture.view as any);
    fixture.visible(true); await flush();
    assert.equal(opens, 1, 'visibility during resolution cannot open a second panel');
    assert.match(fixture.view.webview.html, /Opening AIRK Explorer/);
    assert.doesNotMatch(fixture.view.webview.html, /Get started with AIRK/);
    finish(); await flush(); assert.equal(hides, 1);
    fixture.visible(true); assert.equal(opens, 2, 'a subsequent activity click reveals Explorer again');
    fixture.visible(false); finish(); await flush();
    assert.equal(hides, 1, 'a delayed launch cannot close the sidebar the user switched to');
    fixture.dispose(); sidebar.dispose(); assert.equal(released, 1);
});

test('activity entry shows the old panel only on an opening failure and can retry', async () => {
    const fixture = sidebarView(); let fail = true, hides = 0;
    const sidebar = new ExplorerSidebar({ commandUris: [], folders: () => 1,
        attach: async () => { throw new Error('snapshot unavailable'); },
        open: async () => { if (fail) { throw new Error('<missing bundle>'); } },
        hide: async () => { hides++; }, search: () => {} });
    sidebar.resolveWebviewView(fixture.view as any); await flush();
    const html = fixture.view.webview.posted.at(-1).html;
    assert.match(html, /Get started with AIRK/); assert.match(html, /&lt;missing bundle&gt;/);
    assert.match(html, /Retry AIRK Explorer/); assert.equal(hides, 0);
    fail = false; fixture.send({ type: 'sidebar', action: 'open' }); await flush();
    assert.equal(hides, 1); assert.doesNotMatch(fixture.view.webview.posted.at(-1).html, /Get started with AIRK/);
    fixture.dispose(); sidebar.dispose();
});

test('a recreated activity view waits for the old launch and then opens its own Explorer', async () => {
    const first = sidebarView(), second = sidebarView(); let finish!: () => void, opens = 0, hides = 0;
    const sidebar = new ExplorerSidebar({ commandUris: [], folders: () => 1, attach: async () => disposable,
        open: () => { opens++; return opens === 1 ? new Promise<void>(resolve => { finish = resolve; }) : Promise.resolve(); },
        hide: async () => { hides++; }, search() {} });
    sidebar.resolveWebviewView(first.view as any); first.dispose(); sidebar.resolveWebviewView(second.view as any);
    finish(); await flush();
    assert.equal(opens, 2); assert.equal(hides, 1);
    second.dispose(); sidebar.dispose();
});

test('status selection generations load the new project after a pending old request settles', async () => {
    const instance = Object.create(ExplorerController.prototype) as any;
    let release!: (value: any) => void;
    const calls: string[] = [];
    Object.assign(instance, { selected: 'A', selectionEpoch: 1,
        host: { trusted: () => true, specs: () => ['A','B'].map(binding => ({command:'/launcher',root:`/src/${binding}`,service:{project:binding}})) },
        control: {observationIdentity:'same-owner',state:{inactiveProjects:false}},panel:{open:true,generation:1},retained:disposable,publish() {},
        session() { const selected = this.selected; return { call: () => {
            calls.push(selected);
            return selected === 'A' ? new Promise(resolve => { release = resolve; }) : Promise.resolve({ payload: { index_state: 'available', publication: { token: 'B' } } });
        } }; } });
    const old = instance.loadStatus();
    instance.selected = 'B'; instance.selectionEpoch++;
    void instance.loadStatus();
    release({ payload: { index_state: 'available', publication: { token: 'A' } } });
    await old; await flush();
    assert.deepEqual(calls, ['A', 'B']); assert.equal(instance.statusFor, 'B'); assert.equal(instance.status.publication, 'B');
});

test('Expand uses exact hash/revision and bounded context rather than a range beyond EOF', async () => {
    const instance = Object.create(ExplorerController.prototype) as any;
    const calls: any[] = [];
    Object.assign(instance, { requireProject: () => project, refs: { get: () => ({ path: 'a.ts', startLine: 1, endLine: 1, contentHash: 'ab'.repeat(32), publication: 'p' }), add: () => 'h2' },
        session: () => ({ call: async (tool: string, args: any) => {
            calls.push({ tool, args });
            return { publication: 'p', payload: tool === 'airk_file_info' ? { line_basis: 'source' } : { hits: [{ citation: { path: 'a.ts', start_line: 1, end_line: 1, content_hash: 'ab'.repeat(32) }, text: 'one line' }] } };
        } }) });
    const result = await instance.dispatch({ type: 'evidence.expand', ref: 'h1', before: 5, after: 5 });
    assert.equal(result.text, 'one line');
    assert.equal(calls[1].tool, 'airk_context');
    assert.equal(calls[1].args.revision, 'p'); assert.equal(calls[1].args.known_hash, 'ab'.repeat(32));
    assert.equal(calls[1].args.end_line, undefined);
});

test('live help selectors and continuation call only airk_help and preserve its page metadata', async () => {
    const instance = Object.create(ExplorerController.prototype) as any, calls: any[] = [];
    Object.assign(instance, { requireProject: () => project, session: () => ({ call: async (tool: string, args: any) => {
        calls.push({ tool, args }); return { payload: { topic: 'tools', title: 'Tool reference', content: 'schema page', tool: 'airk_code_facts', version: '1.5.0', start_line: 8, next_line: 20, truncated: true } };
    } }) });
    const reply = await instance.dispatch({ type: 'help.engine', tool: 'airk_code_facts', startLine: 8 });
    assert.deepEqual(calls, [{ tool: 'airk_help', args: { tool: 'airk_code_facts', start_line: 8, max_bytes: 16384 } }]);
    assert.equal(reply.startLine, 8); assert.equal(reply.nextLine, 20); assert.equal(reply.tool, 'airk_code_facts');
    await assert.rejects(instance.dispatch({ type: 'help.engine', tool: 'airk_search', command: 'runtime' }), /Choose one help selector/);
    assert.equal(calls.length, 1, 'conflicting selectors reach no native operation');
});

test('the panel shell uses a nonce CSP, no command URIs and only the out/ bundles', () => {
    const html = explorerHtml({ cspSource: 'vscode-resource:', asWebviewUri: (uri: any) => uri }, { path: '/ext' } as any, 'n0nce');
    assert.match(html, /default-src 'none'/);
    assert.match(html, /script-src 'nonce-n0nce'/);
    assert.doesNotMatch(html, /unsafe-inline|unsafe-eval|https?:\/\//);
    assert.match(html, /\/ext\/out\/explorerWebview\.js/);
    assert.match(html, /\/ext\/out\/explorerWebview\.css/);
});

test('the controller publishes state, loads published status and rejects invalid or unknown messages', async () => {
    const { explorer, statusBar, runs } = controller();
    explorer.open('#search/query');
    const panel = panels.at(-1);
    assert.equal(panel.options.enableCommandUris, false);
    assert.deepEqual(panel.options.localResourceRoots.map((uri: any) => uri.path), ['/ext/out']);
    panel.send({ v: 1, type: 'ready', requestId: 'ready' });
    await flush();
    const state = panel.posted.filter((message: any) => message.type === 'state').at(-1).state as ExplorerViewState;
    assert.equal(state.selectedProject, project);
    assert.equal(state.status?.publication, 'airk-r1:t');
    assert.equal(state.projects[0].collection, 'demo');
    assert.deepEqual(state.features, FEATURES);
    assert.ok(panel.posted.some((message: any) => message.type === 'navigate' && message.route === '#search/query'));
    assert.equal(statusBar.at(-1), 'AIRK: ready');

    panel.send({ v: 1, type: 'project.select', requestId: 'p1', project: 'cd'.repeat(32) });
    panel.send({ v: 1, type: 'nav.go', requestId: 'n1', route: '#nowhere' });
    panel.send({ v: 1, type: 'search.run', requestId: 's1', query: 'x', mode: 'lexical', limit: 5, maxBytes: 8192, fullChunks: false });
    panel.send({ v: 1, type: 'source.open', requestId: 'o1', ref: 'h404' });
    panel.send({ v: 1, type: 'search.run', requestId: 's2', query: 'x', mode: 'lexical', limit: 5, maxBytes: 8192, fullChunks: false, pathPrefix: '../secret' });
    panel.send({ v: 1, type: 'manage.action', requestId: 'm1', action: 'index', project });
    panel.send({ v: 1, type: 'manage.action', requestId: 'm2', action: 'shell', project });
    panel.send({ v: 1, type: 'shell.exec', requestId: 'x1', command: 'rm -rf /' });
    await flush();
    const results = Object.fromEntries(panel.posted.filter((message: any) => message.type === 'result').map((message: any) => [message.requestId, message]));
    assert.equal(results.p1.ok, false); assert.equal(results.p1.error.code, 'NO_PROJECT');
    assert.equal(results.n1.ok, false); assert.equal(results.n1.error.code, 'INVALID_MESSAGE');
    assert.equal(results.s1.ok, true, JSON.stringify(results.s1)); assert.deepEqual(results.s1.data, {completed:true});
    const completedSearch=panel.posted.filter((message:any)=>message.type==='search.session'&&message.session.result).at(-1).session.result;
    assert.equal(completedSearch.hits[0].path, 'a.ts'); assert.match(completedSearch.hits[0].ref, /^h/);
    assert.equal(completedSearch.ordering, 'index_identity');
    assert.equal(results.o1.ok, false); assert.equal(results.o1.error.code, 'UNKNOWN_REFERENCE');
    assert.equal(results.s2.ok, false); assert.equal(results.s2.error.code, 'INVALID_MESSAGE');
    assert.equal(results.m1.ok, true, JSON.stringify(results.m1));
    assert.equal(results.m2.error.code, 'INVALID_MESSAGE', 'unknown console actions are refused without dispatch');
    assert.deepEqual(runs, [['index', { project }]], 'index runs only through the shared console for a project on its page');
    assert.equal(results.x1.error.code, 'INVALID_MESSAGE', 'unrecognized messages receive a bounded refusal before dispatch');
    explorer.dispose();
    assert.equal(panel.disposed, true);
});

test('an untrusted window publishes no projects and never loads evidence', async () => {
    const { explorer } = controller(false);
    explorer.open();
    const panel = panels.at(-1);
    panel.send({ v: 1, type: 'ready', requestId: 'ready' });
    await flush();
    const state = panel.posted.filter((message: any) => message.type === 'state').at(-1).state as ExplorerViewState;
    assert.equal(state.trusted, false);
    assert.deepEqual(state.projects, []);
    assert.equal(state.status, undefined);
    explorer.dispose();
});

test('the sidebar accepts only open and bounded search, escapes values and falls back to setup guidance', () => {
    assert.deepEqual(sidebarMessage({ type: 'sidebar', action: 'open' }), { type: 'sidebar', action: 'open' });
    assert.deepEqual(sidebarMessage({ type: 'sidebar', action: 'search', query: 'refresh token' }), { type: 'sidebar', action: 'search', query: 'refresh token' });
    for (const bad of [{ type: 'sidebar', action: 'search', query: '' }, { type: 'sidebar', action: 'search', query: 'a\nb' },
        { type: 'sidebar', action: 'open', command: 'x' }, { type: 'sidebar', action: 'run' }, { type: 'other', action: 'open' }, null]) {
        assert.equal(sidebarMessage(bad), undefined, JSON.stringify(bad));
    }
    assert.match(sidebarContent(undefined, 1), /Get started with AIRK/);
    const state: ExplorerViewState = { revision: 1, extensionVersion: 'x', trusted: true, connection: 'ready', features: FEATURES, selectedProject: project,
        projects: [{ project, collection: 'c', name: '<b>n</b>', root: '/r', current: true }] };
    const html = sidebarContent(state, 1);
    assert.match(html, /&lt;b&gt;n&lt;\/b&gt;/);
    assert.match(html, /Search indexed evidence/);
    assert.match(html, /not observed/);
    const page = renderSidebar(state, 1, 'abc');
    assert.match(page, /script-src 'nonce-abc'/);
    assert.doesNotMatch(page, /unsafe-inline/);
});

test('search recipes are validated, kept per workspace, replaced by name and capped', async () => {
    const stored = new Map<string, unknown>([['airk.explorer.searchRecipes', [{ schema: 'airk.explorer.search-recipe/1', name: 'damaged', query: 'x', mode: 'shell', limit: 1, maxBytes: 1024, fullChunks: false }]]]);
    const { explorer } = controller(true, stored);
    explorer.open('#search/recipes');
    const panel = panels.at(-1);
    panel.send({ v: 1, type: 'ready', requestId: 'ready' });
    await flush();
    const reply = async (id: string, message: Record<string, unknown>) => {
        panel.send({ v: 1, requestId: id, ...message });
        await flush();
        return panel.posted.find((entry: any) => entry.type === 'result' && entry.requestId === id);
    };
    const recipe = (name: string, extra: Record<string, unknown> = {}) => ({ schema: 'airk.explorer.search-recipe/1', name, query: 'renew', mode: 'lexical', limit: 20, maxBytes: 16384, fullChunks: false, ...extra });
    assert.deepEqual((await reply('l1', { type: 'recipes.list' })).data, [], 'a damaged stored recipe is dropped, never run');
    assert.equal((await reply('import1', { type: 'recipes.save', recipe: recipe('imported'), expected: 'null' })).ok, true);
    const conflict = await reply('import2', { type: 'recipes.save', recipe: recipe('imported', { query: 'replacement' }), expected: 'null' });
    assert.equal(conflict.error.code, 'PROPOSAL_CONFLICT', 'a file import cannot overwrite a recipe added after review');
    await reply('import-delete', { type: 'recipes.delete', name: 'imported' });
    assert.deepEqual((await reply('s1', { type: 'recipes.save', recipe: recipe('b') })).data.map((entry: any) => entry.name), ['b']);
    assert.deepEqual((await reply('s2', { type: 'recipes.save', recipe: recipe('a', { pathPrefix: 'src/' }) })).data.map((entry: any) => entry.name), ['a', 'b']);
    assert.equal((await reply('s3', { type: 'recipes.save', recipe: recipe('a', { query: 'changed' }) })).data.find((entry: any) => entry.name === 'a').query, 'changed');
    assert.equal((await reply('s4', { type: 'recipes.save', recipe: recipe('c', { pathPrefix: '../outside' }) })).error.code, 'INVALID_SCHEMA');
    assert.equal((await reply('s5', { type: 'recipes.save', recipe: recipe('c', { command: 'x' }) })).error.code, 'INVALID_SCHEMA');
    assert.deepEqual((await reply('d1', { type: 'recipes.delete', name: 'b' })).data.map((entry: any) => entry.name), ['a']);
    for (let index = 0; index < 49; index++) { await reply(`f${index}`, { type: 'recipes.save', recipe: recipe(`r${String(index).padStart(2, '0')}`) }); }
    assert.match((await reply('over', { type: 'recipes.save', recipe: recipe('one-too-many') })).error.message, /At most 50 recipes/);
    explorer.dispose();
});

test('the Inbox reaches the webview as summaries; review, resolve and discard go through typed messages', async () => {
    const { explorer, suggestions } = controller();
    explorer.open('#overview');
    const panel = panels.at(-1);
    panel.send({ v: 1, type: 'ready', requestId: 'ready' });
    await flush();
    const admitted = suggestions.submit('search', { schema_version: '1.0', idempotency_key: 'k', title: 'Find renewals', draft: { query: 'renew <script>' } }, { name: 'agent', version: '1' });
    assert.ok(admitted.ok);
    await flush();
    const state = panel.posted.filter((message: any) => message.type === 'state').at(-1).state as ExplorerViewState;
    assert.equal(state.suggestions?.open, 1);
    assert.equal((state.suggestions?.cards[0] as any).body, undefined, 'state carries no draft bodies');
    const reply = async (id: string, message: Record<string, unknown>) => {
        panel.send({ v: 1, requestId: id, ...message });
        await flush();
        return panel.posted.find((entry: any) => entry.type === 'result' && entry.requestId === id);
    };
    const detail = await reply('g1', { type: 'suggestions.get', id: admitted.receipt.proposal_id });
    assert.equal(detail.data.body.search.query, 'renew <script>');
    assert.equal((await reply('r1', { type: 'suggestions.resolve', id: admitted.receipt.proposal_id, status: 'executed' })).error.code, 'INVALID_MESSAGE', 'unknown statuses are refused without changing the proposal');
    assert.equal(suggestions.status(admitted.receipt.proposal_id)?.status, 'pending_review');
    assert.equal((await reply('r2', { type: 'suggestions.resolve', id: admitted.receipt.proposal_id, status: 'deferred' })).ok, true);
    assert.equal(suggestions.status(admitted.receipt.proposal_id)?.status, 'deferred');
    assert.deepEqual((await reply('d1', { type: 'suggestions.discardAll' })).data, { discarded: 1 });
    assert.equal((await reply('r3', { type: 'suggestions.resolve', id: admitted.receipt.proposal_id, status: 'applied' })).error.code, 'PROPOSAL_EXPIRED', 'a rejected draft cannot be applied');
    await reply('v1', { type: 'suggestions.revisions', studio: 3, planner: 7 });
    assert.deepEqual(explorer.draftRevisions, { studio: 3, planner: 7 });
    explorer.showInbox();
    assert.ok(panel.posted.some((message: any) => message.type === 'inbox'));
    explorer.dispose();
});

test('combined Inbox Apply prefills only the host-owned matching Workbench form and never runs a read', async () => {
    const f = controller(), applied: unknown[] = [];
    (f.host as any).prefillCombinedSearch = async (search: unknown) => { applied.push(structuredClone(search)); };
    const projects = ['prj_' + '01'.repeat(16)], indexes = ['idx_' + '02'.repeat(16)];
    const submitted = f.suggestions.submit('search', { schema_version: '1.0', idempotency_key: 'combined-apply', title: 'Find originals',
        draft: { query: 'original', project_ids: projects, index_ids: indexes, member_availability: 'require' } }, { name: 'agent', version: '1' });
    assert.ok(submitted.ok);
    try {
        await (f.explorer as any).dispatch({ type: 'suggestions.resolve', id: submitted.receipt.proposal_id, status: 'applied' });
        assert.equal(applied.length, 1);
        assert.deepEqual((applied[0] as any).project_ids, projects);
        assert.deepEqual((applied[0] as any).index_ids, indexes);
        assert.equal((applied[0] as any).member_availability, 'require');
        assert.equal(f.suggestions.status(submitted.receipt.proposal_id)?.status, 'applied');
        assert.equal(f.suggestions.status(submitted.receipt.proposal_id)?.executed, false);
        assert.deepEqual(f.runs, []);
        await assert.rejects((f.explorer as any).dispatch({ type: 'suggestions.resolve', id: submitted.receipt.proposal_id, status: 'applied' }));
        assert.equal(applied.length, 1, 'a resolved proposal cannot fill a form again');
        const changed = f.suggestions.submit('search', { schema_version: '1.0', idempotency_key: 'changed-combined', title: 'Another search',
            draft: { query: 'original', project_ids: projects } }, { name: 'agent', version: '1' });
        assert.ok(changed.ok);
        (f.host as any).prefillCombinedSearch = async () => { throw new Error('combined_draft_scope_changed'); };
        await assert.rejects((f.explorer as any).dispatch({ type: 'suggestions.resolve', id: changed.receipt.proposal_id, status: 'applied' }), /combined_draft_scope_changed/);
        assert.equal(f.suggestions.status(changed.receipt.proposal_id)?.status, 'conflicted');
        assert.deepEqual(f.runs, []);
    } finally { f.explorer.dispose(); }
});


// Actual console -> event -> ExplorerController -> cloned panel-message path.
// Only the native control transport and evidence boundary are deferred fixtures.
const { ServiceConsoleController } = require('../serviceConsoleController') as typeof import('../serviceConsoleController');
const { ServiceClient } = require('../serviceClient') as typeof import('../serviceClient');
const { EvidenceClient } = require('../explorerEvidence') as typeof import('../explorerEvidence');
function f08Deferred<T>() {
    let resolve!: (value:T)=>void;
    const promise=new Promise<T>(done=>{resolve=done;});
    return {promise,resolve};
}
function promptFeedbackHost(context:any,blockedMethod='resources',savedCounts=false) {
    const other='cd'.repeat(32),namespace='12'.repeat(16),instance='34'.repeat(16),version='1.8.0';
    let trusted=true,retained=0,current!:InstanceType<typeof ServiceConsoleController>;
    let liveInstance=instance,liveNamespace=namespace,namespaceGate:ReturnType<typeof f08Deferred<void>>|undefined;
    const changed=new Set<(state:any)=>void>(),specChanges=new Set<()=>void>(),values=new Map<string,unknown>();
    const clientSignals:Array<ReturnType<typeof f08Deferred<void>>>=[],gates:Array<{entered:ReturnType<typeof f08Deferred<void>>;release:ReturnType<typeof f08Deferred<void>>}>=[];
    const evidenceSignals:Array<ReturnType<typeof f08Deferred<void>>>=[],evidence:Array<{tool:string;project:string;spec:any;result:ReturnType<typeof f08Deferred<any>>}>=[];
    const polls:Promise<void>[]=[],consoleSnapshots:any[]=[];
    let specs=[project,other].map((binding,index)=>({command:'/launcher',root:`/src/prompt-${index}`,dataDir:`/data/${index}`,collection:`prompt-${index}`,name:`prompt-${index}`,service:{installDir:'/install',project:binding}}));
    const page={revision:1,cached:true,observed_unix_ms:1,after:null,projects:specs.map(spec=>({project:{registration:{binding:spec.service.project,root:spec.root,data_root:spec.dataDir,collection:spec.collection},paused:false,fencing_generation:1},progress:null,inspection:savedCounts?{snapshot:{index_state:'available',publication:'saved-old-registry-publication',indexed_files:99}}:null}))};
    const binding={installDir:'/install',controlDir:'/control',executable:'/main',launcher:null,runtimeSha256:'56'.repeat(32),version,lifecycle:'on-demand',verificationBasis:'trusted_distribution'};
    const runtime:any={
        discover:async()=>({bundle:{main:'/main',helper:'/helper',source:'/bundle',digest:binding.runtimeSha256,version,platform:'test'},installDir:'/install',installed:true,location:{},inspection:{control_dir:'/control',registry_namespace:namespace,approval:{version},console:page}}),
        binding:()=>binding,ensure:async()=>binding,snapshot:async()=>({}),repair:async()=>({}),prepareConfiguration:async()=>({}),reviewTransition:async()=>({}),transition:async()=>({}),
        client:()=>{
            const index=gates.length,gate={entered:f08Deferred<void>(),release:f08Deferred<void>()};gates.push(gate);
            (clientSignals[index]??=f08Deferred<void>()).resolve();
            return new ServiceClient({dispose(){},async request(method:any){
                const name=typeof method==='string'?method:Object.keys(method)[0];
                if(name===blockedMethod){gate.entered.resolve();await gate.release.promise;}
                // Routine polls authenticate registry continuity through Events.
                if(name==='events'&&namespaceGate)await namespaceGate.promise;
                switch(name){
                    case 'health':return {status:'health',observation:{instance:liveInstance,version,policy:{accepting:true}}};
                    case 'resources':return {status:'resources',observation:{configuration:{preset:'balanced'}}};
                    case 'embedding_status':return {status:'embedding_status',available:false,loads:0};
                    case 'source_status':return {status:'source_status',available:false};
                    case 'request_epoch':return {status:'request_epoch',namespace:liveNamespace,epoch:1};
                    case 'console':case 'inactive_console':return {status:'console',page};
                    case 'workers':return {status:'workers',observations:page.projects.map(row=>({project:row.project.registration.binding,identity:null,restarting:false,automatic_start_blocked:false})),age_ms:1};
                    case 'events':return {status:'events',page:{namespace:liveNamespace,retained_prefix_lost:false,events:[],next:null,tail:{namespace:liveNamespace,project:null,after:0,through:null}}};
                    case 'logs':return {status:'logs',page:{schema:1,records:[]}};
                    default:throw new Error(`Unexpected prompt-feedback metadata operation ${name}`);
                }
            }});
        },
    };
    const consoleHost:any={version,authority:'local-test',runtime,
        storage:{keys:()=>[...values.keys()],get:(key:string)=>values.get(key),update:async(key:string,value:unknown)=>{values.set(key,value);}},
        trusted:()=>trusted,changed:(state:any)=>{consoleSnapshots.push(structuredClone(state));for(const listener of changed)listener(state);},
        confirm:async()=>true,copy:async()=>{},logs:async()=>{},resources:async()=>undefined,lifecycle:async()=>({})};
    current=new ServiceConsoleController(consoleHost);
    const startPoll=()=>{const poll=current.refresh();polls.push(poll);return poll;};
    const source={get controller(){return current;},
        run:async(action:string)=>{const selected=current;if(!await selected.whenIdle())return;await selected.action({type:'service',revision:selected.state.revision,action});},
        retain:()=>{if(++retained===1)void startPoll();let released=false;return {dispose(){if(!released){released=true;retained--;}}};},
        onDidChangeState:(listener:(state:any)=>void)=>{changed.add(listener);return {dispose(){changed.delete(listener);}};}};
    const suggestions=new SuggestionStore({binding:()=> 'ab12ab12ab12ab12',revisions:()=>({}),publication:()=>undefined,ttlMs:()=>3600_000});
    const host:any={extensionUri:{path:'/ext'},version,trusted:()=>trusted,specs:()=>specs,activeProject:()=>project,
        onDidChangeSpecs:(listener:()=>void)=>{specChanges.add(listener);return {dispose(){specChanges.delete(listener);}};},
        log(){},setStatusBar(){},workspaceState:{get:(_key:string,fallback:unknown)=>fallback,update:async()=>{}},suggestions,control:source};
    context.mock.method(EvidenceClient.prototype,'call',function(this:InstanceType<typeof EvidenceClient>,tool:string){
        assert.ok(tool==='airk_status'||tool==='airk_diagnostics','only scalar readiness or explicit details are expected');
        const index=evidence.length;
        const pending={tool,project:this.project,spec:structuredClone(this.spec),result:f08Deferred<any>()};evidence.push(pending);
        (evidenceSignals[index]??=f08Deferred<void>()).resolve();return pending.result.promise;
    });
    const explorer=new ExplorerController(host);
    const unavailable={payload:{index_state:'unavailable',publication:null,files:null,chunks:null,index_error:{code:'index_status_unavailable'}}};
    return {explorer,host,source,evidence,consoleSnapshots,
        get console(){return current;},
        async blocked(index=0){await(clientSignals[index]??=f08Deferred<void>()).promise;await gates[index].entered.promise;},
        release(index=0){gates[index].release.resolve();},
        async requested(index=0){await(evidenceSignals[index]??=f08Deferred<void>()).promise;return evidence[index];},
        finish(index=0,result:any=unavailable){evidence[index].result.resolve(result);},
        async settleStatus(){
            // Initial unknown ownership may invalidate the first diagnostics;
            // complete each independently admitted replacement, never relabel it.
            for(let turn=0;turn<12&&!explorer.snapshot().status;turn++){
                for(const request of evidence)request.result.resolve(unavailable);
                await flush();
            }
            assert.ok(explorer.snapshot().status,'current identity needs its own completed observation');
        },
        settings(value:string){(page.projects[0] as any).inspection={observed_unix_ms:Date.now(),snapshot:{project,incarnation:'11'.repeat(16),settings_digest:value,vectors_enabled:true}};},
        restartIdentity(){liveInstance='78'.repeat(16);liveNamespace='9a'.repeat(16);namespaceGate=f08Deferred<void>();},
        releaseNamespace(){namespaceGate?.resolve();},
        poll(){return startPoll();},
        trust(value:boolean){trusted=value;},
        changeSpec(){specs=specs.map((spec,index)=>index?spec:{...spec,command:'/new-approved-launcher'});for(const listener of specChanges)listener();},
        reset(){current.dispose();current=new ServiceConsoleController(consoleHost);if(retained)void startPoll();},
        async cleanup(){explorer.dispose();current.dispose();namespaceGate?.resolve();for(const gate of gates)gate.release.resolve();for(const request of evidence)request.result.resolve(unavailable);await Promise.all(polls);},
    };
}

test('actual Explorer publishes authenticated health while optional metadata is blocked without claiming an index',async context=>{
    for(const blocked of ['resources','logs']){
        const f=promptFeedbackHost(context,blocked);
        try{
            f.explorer.open('#search/query');const panel=panels.at(-1);panel.send({v:1,type:'ready',requestId:'ready'});
            await f.blocked();await f.requested();
            const early=panel.posted.filter((entry:any)=>entry.type==='state'&&entry.state.connection==='ready').at(-1)?.state;
            assert.ok(early,`verified connection must reach the real Explorer before ${blocked}`);
            assert.equal(early.selectedProject,project);assert.equal(early.projects.length,0,'saved rows wait for current registry continuity');
            assert.equal(early.status,undefined,'deferred detailed evidence is not fabricated from healthy service');
            assert.ok(early.projects.every((entry:any)=>entry.publication===undefined&&entry.indexedFiles===undefined));
            assert.equal(f.console.state.busy,true);
            const captured=structuredClone(early);
            f.release();await f.console.whenIdle();f.finish();await f.settleStatus();await flush();
            assert.deepEqual(early,captured,'captured postMessage value does not change when later controller fields change');
            const final=panel.posted.filter((entry:any)=>entry.type==='state').at(-1).state;
            assert.equal(final.status.indexState,'unavailable');assert.equal(final.status.publication,undefined);
        }finally{await f.cleanup();context.mock.restoreAll();}
    }
});

test('actual Explorer ignores delayed old health from reset and from revoked trust',async context=>{
    for(const stop of ['reset','trust'] as const){
        const f=promptFeedbackHost(context,'health');
        try{
            f.explorer.open();const panel=panels.at(-1);panel.send({v:1,type:'ready',requestId:'ready'});await f.blocked();
            if(stop==='reset'){f.reset();await f.blocked(1);}else f.trust(false);
            const before=panel.posted.length;f.release(0);await flush();
            assert.ok(!panel.posted.slice(before).some((entry:any)=>entry.type==='state'&&entry.state.connection==='ready'));
            assert.equal(f.console.state.connected,false);
            if(stop==='reset'){f.release(1);await f.console.whenIdle();assert.equal(f.console.state.connected,true);}
            else assert.equal(f.console.state.trusted,false);
        }finally{await f.cleanup();context.mock.restoreAll();}
    }
});

test('actual Explorer fences delayed detailed evidence by current project and immutable launch spec',async context=>{
    for(const change of ['project','spec'] as const){
        const f=promptFeedbackHost(context);
        try{
            f.explorer.open();const panel=panels.at(-1);panel.send({v:1,type:'ready',requestId:'ready'});await f.blocked();const old=await f.requested();
            if(change==='project')panel.send({v:1,type:'project.select',requestId:'select',project:'cd'.repeat(32)});else f.changeSpec();
            f.finish(0,{payload:{index_state:'available',publication:{token:'old-unaccepted-publication'},files:99,chunks:99}});
            const replacement=await f.requested(1);assert.notDeepEqual(replacement.spec,old.spec);
            assert.ok(!panel.posted.some((entry:any)=>entry.type==='state'&&entry.state.status?.publication==='old-unaccepted-publication'));
            f.finish(1);f.release();await f.console.whenIdle();await flush();
            assert.equal(f.explorer.snapshot().status?.indexState,'unavailable');
        }finally{await f.cleanup();context.mock.restoreAll();}
    }
});

test('actual Explorer does not publish old status into recreated panel or revoked trust',async context=>{
    for(const change of ['panel','trust'] as const){
        const f=promptFeedbackHost(context);
        try{
            f.explorer.open();const first=panels.at(-1);first.send({v:1,type:'ready',requestId:'ready'});await f.blocked();await f.requested();
            if(change==='panel'){first.dispose();f.explorer.open();panels.at(-1).send({v:1,type:'ready',requestId:'new-ready'});}else f.trust(false);
            const recipient=panels.at(-1),before=recipient.posted.length;
            f.finish(0,{payload:{index_state:'available',publication:{token:'old-panel-or-trust-publication'},files:99,chunks:99}});await flush();
            assert.ok(!recipient.posted.slice(before).some((entry:any)=>entry.type==='state'&&entry.state.status?.publication==='old-panel-or-trust-publication'));
            if(change==='panel'){await f.requested(1);f.finish(1);}else assert.equal(f.explorer.snapshot().status,undefined);
            f.release();await f.console.whenIdle();
        }finally{await f.cleanup();context.mock.restoreAll();}
    }
});

const r03Request=(requestId:string)=>({v:1,type:'search.run',requestId,query:'x',mode:'lexical',limit:5,maxBytes:8192,fullChunks:false});
const r03Evidence=(text='one-full-evidence-delivery')=>({publication:'airk-r1:t',payload:{truncated:false,
    publication:{token:'airk-r1:t'},hits:[{citation:{path:'a.ts',start_line:2,end_line:4,content_hash:'cd'.repeat(32)},text,truncated:false}]}});
function r03Pending(explorer:InstanceType<typeof ExplorerController>){
    const requests:Array<{resolve:(value:any)=>void;reject:(error:Error)=>void}>=[];
    (explorer as any).session=()=>({call:(tool:string)=>tool==='airk_search'
        ?new Promise((resolve,reject)=>{requests.push({resolve,reject});})
        :Promise.resolve({payload:{index_state:'available',publication:{token:'airk-r1:t'}}})});
    return requests;
}

test('Search posts one complete evidence session and a small correctly correlated acknowledgment',async()=>{
    const {explorer}=controller();
    try{
        explorer.open('#search/query');const panel=panels.at(-1);
        panel.send({v:1,type:'ready',requestId:'ready'});await flush();
        const before=panel.posted.length;
        panel.send(r03Request('r03-complete'));await flush();
        const posted=panel.posted.slice(before),sessions=posted.filter((row:any)=>row.type==='search.session');
        assert.equal(sessions.length,2,'one busy observation and one completed observation');
        assert.equal(sessions[0].session.busy,true);assert.equal(sessions[0].session.result,undefined);
        assert.equal(sessions[1].session.busy,false);
        const result=sessions[1].session.result;
        assert.equal(result.hits[0].text,'x');
        assert.equal((explorer as any).refs.get(result.hits[0].ref,project).path,'a.ts');
        const replies=posted.filter((row:any)=>row.type==='result');
        assert.deepEqual(replies,[{v:1,type:'result',requestId:'r03-complete',ok:true,data:{completed:true}}]);
        assert.equal(posted.filter((row:any)=>row.session?.result||row.data?.hits).length,1);
        assert.equal('hits' in replies[0].data,false);assert.equal('publication' in replies[0].data,false);
        // Reopening is an intentional retained-session delivery, not a duplicate response.
        panel.dispose();explorer.open();const reopened=panels.at(-1);
        reopened.send({v:1,type:'ready',requestId:'ready'});await flush();
        assert.deepEqual(reopened.posted.find((row:any)=>row.type==='search.session').session.result,result);
        assert.ok(!reopened.posted.some((row:any)=>row.type==='result'&&row.requestId==='r03-complete'));
    }finally{explorer.dispose();}
});

test('Search native failure preserves busy/error delivery and a correlated failure without success acknowledgment',async()=>{
    const {explorer}=controller();
    try{
        explorer.open();const panel=panels.at(-1);panel.send({v:1,type:'ready',requestId:'ready'});await flush();
        const pending=r03Pending(explorer),before=panel.posted.length;
        panel.send(r03Request('r03-failed'));pending[0].reject(new Error('Original search failure'));await flush();
        const posted=panel.posted.slice(before),sessions=posted.filter((row:any)=>row.type==='search.session');
        assert.equal(sessions.length,2);assert.equal(sessions[0].session.busy,true);
        assert.equal(sessions[1].session.busy,false);assert.match(sessions[1].session.error,/Original search failure/);
        assert.equal(sessions[1].session.result,undefined);
        const replies=posted.filter((row:any)=>row.type==='result');
        assert.equal(replies.length,1);assert.equal(replies[0].requestId,'r03-failed');assert.equal(replies[0].ok,false);
        assert.equal(replies[0].data,undefined);
    }finally{explorer.dispose();}
});

test('superseded Search keeps one winning evidence delivery and independently correlated outcomes',async()=>{
    const {explorer}=controller();
    try{
        explorer.open();const panel=panels.at(-1);panel.send({v:1,type:'ready',requestId:'ready'});await flush();
        const pending=r03Pending(explorer),before=panel.posted.length;
        panel.send(r03Request('r03-old'));panel.send(r03Request('r03-new'));
        pending[1].resolve(r03Evidence());await flush();
        pending[0].resolve(r03Evidence('stale-evidence-must-not-appear'));await flush();
        const posted=panel.posted.slice(before),complete=posted.filter((row:any)=>row.type==='search.session'&&row.session.result);
        assert.equal(complete.length,1);assert.equal(complete[0].session.result.hits[0].text,'one-full-evidence-delivery');
        assert.equal(JSON.stringify(posted).split('one-full-evidence-delivery').length-1,1);
        assert.ok(!JSON.stringify(posted).includes('stale-evidence-must-not-appear'));
        const replies=posted.filter((row:any)=>row.type==='result');assert.equal(replies.length,2);
        assert.deepEqual(replies.find((row:any)=>row.requestId==='r03-new').data,{completed:true});
        const stale=replies.find((row:any)=>row.requestId==='r03-old');
        assert.equal(stale.ok,false);assert.equal(stale.error.code,'REQUEST_SUPERSEDED');
    }finally{explorer.dispose();}
});

test('late Search cannot deliver evidence or success acknowledgment after project spec or trust scope changes',async()=>{
    for(const change of ['project','spec','trust'] as const){
        const {explorer,host}=controller();
        try{
            explorer.open();const panel=panels.at(-1);panel.send({v:1,type:'ready',requestId:'ready'});await flush();
            const pending=r03Pending(explorer);panel.send(r03Request('r03-scope'));
            if(change==='project')(explorer as any).select(undefined);
            else if(change==='trust')host.trusted=()=>false;
            else{const saved=host.specs();host.specs=()=>saved.map(spec=>({...spec,command:'/changed-launcher'}));}
            const before=panel.posted.length;pending[0].resolve(r03Evidence('wrong-scope-evidence'));await flush();
            const posted=panel.posted.slice(before);
            assert.ok(!posted.some((row:any)=>row.type==='search.session'&&row.session.result));
            assert.ok(!JSON.stringify(posted).includes('wrong-scope-evidence'));
            const reply=posted.find((row:any)=>row.type==='result'&&row.requestId==='r03-scope');
            assert.equal(reply.ok,false);assert.equal(reply.error.code,'REQUEST_SUPERSEDED');
        }finally{explorer.dispose();}
    }
});


test('actual Explorer fences same-spec diagnostics and saved counts across authenticated instance and delayed registry rebind',async context=>{
    const f=promptFeedbackHost(context,'resources',true);
    try{
        f.explorer.open();const panel=panels.at(-1);
        panel.send({v:1,type:'ready',requestId:'ready'});await f.blocked();await f.requested();
        const early=panel.posted.filter((entry:any)=>entry.type==='state'&&entry.state.connection==='ready').at(-1).state;
        assert.equal(early.projects.length,0,'first authenticated health cannot relabel unconfirmed saved registry counts');
        f.release();await f.console.whenIdle();f.finish();await f.settleStatus();
        assert.equal((f.console.state.page?.projects[0].inspection as any)?.snapshot.indexed_files,99,
            'the transport really supplied saved counts before the ownership change');
        const spec=structuredClone(f.host.specs());
        const oldRequest=f.evidence.length;
        panel.send({v:1,type:'status.refresh',requestId:'old-scope-status'});
        await f.requested(oldRequest);await f.console.whenIdle();
        const oldIdentity=f.console.observationIdentity;
        f.restartIdentity();const poll=f.poll();await flush();
        assert.notEqual(f.console.observationIdentity,oldIdentity);
        assert.equal(f.console.state.busy,true,'new registry namespace reply remains independently blocked');
        assert.deepEqual(f.host.specs(),spec,'runtime and registry ownership can change under identical launch specs');
        const boundary=panel.posted.length;
        const current=panel.posted.filter((entry:any)=>entry.type==='state').at(-1).state;
        assert.equal(current.connection,'ready');assert.deepEqual(current.projects,[]);
        assert.equal(current.status,undefined);
        assert.equal(f.explorer.publication,undefined,'Assistance/export publication access shares the same ownership fence');
        f.finish(oldRequest,{payload:{index_state:'available',publication:{token:'old-delayed-diagnostics'},files:777,chunks:888}});
        await flush();
        assert.equal(f.explorer.snapshot().status,undefined);
        assert.ok(!panel.posted.slice(boundary).some((entry:any)=>entry.state?.status?.publication==='old-delayed-diagnostics'));
        const replacement=await f.requested(oldRequest+1);
        assert.deepEqual(replacement.spec,spec[0],'replacement is admitted for the same bytes under new ownership');
        f.releaseNamespace();await poll;
        f.finish(oldRequest+1);await f.settleStatus();
        assert.equal(f.explorer.snapshot().status?.indexState,'unavailable');
        assert.ok(!panel.posted.slice(boundary).some((entry:any)=>entry.state?.status?.files===777));
    }finally{await f.cleanup();context.mock.restoreAll();}
});


test('automatic Explorer observations use scalar status while explicit details retain independently aged fields',async context=>{
    const f=promptFeedbackHost(context);
    try{
        f.explorer.open();const panel=panels.at(-1);panel.send({v:1,type:'ready',requestId:'ready'});
        await f.blocked();await f.requested();f.release();await f.console.whenIdle();await f.settleStatus();
        assert.ok(f.evidence.length>0);assert.ok(f.evidence.every(request=>request.tool==='airk_status'));
        const detailIndex=f.evidence.length;
        panel.send({v:1,type:'status.refresh',requestId:'explicit-details'});
        assert.equal((await f.requested(detailIndex)).tool,'airk_diagnostics');
        f.finish(detailIndex,{payload:{index_state:'available',publication:{token:'same-publication'},files:99,
            vectors_enabled:true,local_model_ready:true,runtime:{versions_match:false},
            semantic_readiness:{state:'partial'},freshness:{state:'running'}}});
        await flush();
        const detailed=f.explorer.snapshot().status!;
        assert.equal(typeof detailed.detailsObservedAt,'number');
        const detailTime=detailed.detailsObservedAt;
        const scalarIndex=f.evidence.length;
        const scalar=(f.explorer as any).loadStatus();
        assert.equal((await f.requested(scalarIndex)).tool,'airk_status');
        f.finish(scalarIndex,{payload:{index_state:'available',publication:{token:'same-publication'},files:101,
            observed_unix_ms:200,semantic_readiness:{state:'partial'},freshness:{state:'idle'}}});
        await scalar;
        const current=f.explorer.snapshot().status!;
        assert.equal(current.files,101);assert.equal(current.freshness,'idle');
        assert.equal(current.observedAt,200);assert.equal(current.detailsObservedAt,detailTime);
        assert.equal(current.vectorsEnabled,true);assert.equal(current.localModelReady,true);
        assert.equal(current.versionsMatch,false);
        let copied='';context.mock.method(vscodeStub.env.clipboard,'writeText',async(value:string)=>{copied=value;});
        await (f.explorer as any).dispatch({v:1,type:'details.copy',requestId:'copy',project,section:'semantic'});
        assert.ok(copied.includes('Published status observed'));
        assert.ok(copied.includes('Detailed observations'));
        assert.ok(copied.includes(new Date(detailTime!).toISOString()));
        assert.ok(panel.posted.some((entry:any)=>entry.type==='result'&&entry.requestId==='explicit-details'&&entry.ok));
    }finally{await f.cleanup();context.mock.restoreAll();}
});

test('two explicit detail waiters behind one scalar share one independent diagnostics request',async context=>{
    const f=promptFeedbackHost(context);
    try{
        f.explorer.open();const panel=panels.at(-1);panel.send({v:1,type:'ready',requestId:'ready'});
        await f.blocked();await f.requested();f.release();await f.console.whenIdle();await f.settleStatus();
        const index=f.evidence.length;
        const scalar=(f.explorer as any).loadStatus();
        assert.equal((await f.requested(index)).tool,'airk_status');
        panel.send({v:1,type:'status.refresh',requestId:'detail-one'});
        panel.send({v:1,type:'status.refresh',requestId:'detail-two'});await flush();
        assert.equal(f.evidence.length,index+1,'details cannot race the existing scalar owner');
        f.finish(index);await scalar;
        assert.equal((await f.requested(index+1)).tool,'airk_diagnostics');
        await flush();assert.equal(f.evidence.length,index+2,'explicit waiters share the details owner');
        f.finish(index+1);await flush();
        for(const requestId of ['detail-one','detail-two']){
            assert.ok(panel.posted.some((entry:any)=>entry.type==='result'&&entry.requestId===requestId&&entry.ok));
        }
    }finally{await f.cleanup();context.mock.restoreAll();}
});

test('same-publication saved settings changes reject old detailed policy while retaining independently sampled runtime',async context=>{
    const f=promptFeedbackHost(context);
    try{
        f.explorer.open();const panel=panels.at(-1);panel.send({v:1,type:'ready',requestId:'ready'});
        await f.blocked();await f.requested();f.release();await f.console.whenIdle();await f.settleStatus();
        f.settings('aa'.repeat(32));await f.poll();
        let index=f.evidence.length;
        const oldDetails=(f.explorer as any).loadStatus('details');
        assert.equal((await f.requested(index)).tool,'airk_diagnostics');
        f.settings('bb'.repeat(32));await f.poll();
        f.finish(index,{payload:{index_state:'available',publication:{token:'same'},vectors_enabled:false,
            local_model_ready:false,runtime:{versions_match:false}}});await oldDetails;
        assert.equal((await f.requested(index+1)).tool,'airk_status');
        f.finish(index+1,{payload:{index_state:'available',publication:{token:'same'},files:5,
            semantic_readiness:{state:'pending'},freshness:{state:'running'},observed_unix_ms:400}});
        await flush();
        assert.equal(f.explorer.snapshot().status?.vectorsEnabled,undefined,'old saved settings cannot be relabeled current');
        index=f.evidence.length;
        const details=(f.explorer as any).loadStatus('details');await f.requested(index);
        f.finish(index,{payload:{index_state:'available',publication:{token:'same'},vectors_enabled:true,
            local_model_ready:true,runtime:{versions_match:false}}});await details;
        const observed=f.explorer.snapshot().status!.detailsObservedAt;
        f.settings('cc'.repeat(32));await f.poll();
        index=f.evidence.length;
        const scalar=(f.explorer as any).loadStatus();await f.requested(index);
        f.finish(index,{payload:{index_state:'available',publication:{token:'same'},files:6,
            semantic_readiness:{state:'partial'},freshness:{state:'idle'},observed_unix_ms:500}});await scalar;
        const status=f.explorer.snapshot().status!;
        assert.equal(status.files,6);assert.equal(status.vectorsEnabled,undefined);
        assert.equal(status.localModelReady,undefined);assert.equal(status.versionsMatch,false);
        assert.equal(status.observedAt,500);assert.equal(status.detailsObservedAt,observed);
    }finally{await f.cleanup();context.mock.restoreAll();}
});
test('publication and control identity changes cannot relabel retained details or migrate queued demand',async context=>{
    const f=promptFeedbackHost(context);
    try{
        f.explorer.open();const panel=panels.at(-1);panel.send({v:1,type:'ready',requestId:'ready'});
        await f.blocked();await f.requested();f.release();await f.console.whenIdle();await f.settleStatus();
        let index=f.evidence.length;
        const details=(f.explorer as any).loadStatus('details');await f.requested(index);
        f.finish(index,{payload:{index_state:'available',publication:{token:'old-publication'},
            vectors_enabled:false,local_model_ready:false,runtime:{versions_match:false}}});await details;
        index=f.evidence.length;
        const scalar=(f.explorer as any).loadStatus();await f.requested(index);
        f.finish(index,{payload:{index_state:'available',publication:{token:'new-publication'},files:7,
            semantic_readiness:{state:'pending'},freshness:{state:'running'},observed_unix_ms:300}});await scalar;
        const published=f.explorer.snapshot().status!;
        assert.equal(published.publication,'new-publication');assert.equal(published.files,7);
        assert.equal(published.vectorsEnabled,undefined);assert.equal(published.localModelReady,undefined);
        assert.equal(published.versionsMatch,false,'same-owner runtime sample retains its own old age');
        index=f.evidence.length;
        const oldScalar=(f.explorer as any).loadStatus();await f.requested(index);
        panel.send({v:1,type:'status.refresh',requestId:'obsolete-detail-demand'});await flush();
        f.restartIdentity();const poll=f.poll();await flush();
        f.finish(index,{payload:{index_state:'available',publication:{token:'obsolete'},files:777}});
        await oldScalar;
        const replacement=await f.requested(index+1);
        assert.equal(replacement.tool,'airk_status','old details demand cannot migrate to new ownership');
        f.releaseNamespace();await poll;f.finish(index+1);await f.settleStatus();
        assert.equal(f.explorer.snapshot().status?.versionsMatch,undefined);
        assert.equal(f.explorer.snapshot().status?.detailsObservedAt,undefined);
    }finally{await f.cleanup();context.mock.restoreAll();}
});
