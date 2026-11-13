// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import Module from 'node:module';
import * as path from 'node:path';
import { test } from 'node:test';
import type { ExplorerViewState } from '../explorerContracts';
import { explorerRequestPolicy, parseWebviewMessage } from '../explorerContracts';
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
const { ServiceError } = require('../serviceClient') as typeof import('../serviceClient');
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

test('management actions and UI-local completions retain their outcomes across evidence changes',async t=>{
    for(const type of ['admin.open','manage.command','notice.dismiss','details.copy','workbench.toggle','extension.reload']){
        const f=controller(),instance=f.explorer as any;
        const change=async()=>{instance.select(undefined);return {committed:type};};
        if(type==='admin.open')(f.host as any).openAdmin=async()=>{await change();};
        if(type==='manage.command')t.mock.method(vscodeStub.commands,'executeCommand',change);
        if(type==='notice.dismiss')instance.dismissNotice=change;
        if(type==='details.copy')t.mock.method(vscodeStub.env.clipboard,'writeText',change);
        if(type==='workbench.toggle')(f.host as any).toggleWorkbench=change;
        if(type==='extension.reload'){(f.host as any).extensionUpdate=()=>({version:'new'});(f.host as any).reloadUpdatedExtension=change;}
        try{
            f.explorer.open('#indexing/library');const panel=panels.at(-1);panel.send({v:1,type:'ready',requestId:'ready'});await flush();const epoch=f.explorer.snapshot().selectionEpoch;
            await instance.handle({v:1,type,requestId:'committed',page:'overview',command:'airk.reconnectAgents',section:'service',key:'notice'});
            assert.notEqual(f.explorer.snapshot().selectionEpoch,epoch,type);
            const reply=panel.posted.find((message:any)=>message.type==='result'&&message.requestId==='committed');assert.equal(reply?.ok,true,type);
            if(type==='manage.command'||type==='notice.dismiss')assert.deepEqual(reply.data,{committed:type});
        }finally{f.explorer.dispose();t.mock.restoreAll();}
    }
});

test('clearing evidence selection rejects late advice before storage while retaining the native management generation',async()=>{
    const f=controller(),native=f.host.control.controller as any,indexing=(f.explorer as any).indexing;
    Object.assign(native,{observationIdentity:'same-native-owner',catalogRead:async()=>{throw Error('unexpected catalog read');},catalogPrepare:async()=>{throw Error('unexpected preview');},catalogApply:async()=>{throw Error('unexpected mutation');},catalogReconcile:async()=>[],catalogPending:()=>[]});
    const advice={schema:'airk.recommendations/1',selected:{project_ids:['prj_'+'11'.repeat(16)],index_ids:['idx_'+'22'.repeat(16)]},future_native_advice:{retained:'complete'}};
    let release!:(value:unknown)=>void,entered!:()=>void,reads=0;const reading=new Promise<void>(resolve=>{entered=resolve;});
    (f.explorer as any).session=()=>({call:async(tool:string)=>{assert.equal(tool,'airk_query_scope');reads++;entered();return new Promise(resolve=>{release=resolve;});},dispose(){}});
    try{
        await indexing.recommendations(async()=>advice);const generation=indexing.snapshot().generation;
        const request=(f.explorer as any).dispatch({type:'indexing.recommendations',arguments:{}});const rejected=assert.rejects(request,/stale_catalog_view/i);await reading;
        (f.explorer as any).select(undefined);assert.equal(indexing.snapshot().recommendations,undefined);assert.equal(indexing.snapshot().generation,generation);assert.equal(indexing.snapshot().busy,false,'advice does not own management admission');
        release({payload:advice});await rejected;assert.equal(reads,1);assert.equal(indexing.snapshot().recommendations,undefined,'Old source-scope advice cannot appear under a replacement evidence selection');assert.equal(indexing.snapshot().generation,generation);
    }finally{f.explorer.dispose();}
});

/** A window search scope host (U77) recording its calls; `available` follows `window-scope/1`. */
function windowScope(available=true,overrides:Record<string,unknown>={}){
    const calls:unknown[]=[];
    const scope={generation:0,calls,available:()=>available,identity:()=>undefined as string|undefined,searchScopeChoices:()=>undefined,
        searchScopeSummary:()=>({available,active:false,outOfDate:false,remembered:false}),
        readSearchScope:async(kind?:string,next?:boolean)=>{calls.push(['read',kind,next]);},
        applySearchScope:async(selection:unknown)=>{calls.push(['apply',selection]);return {status:'scope'};},
        clearSearchScope:async()=>{calls.push(['clear']);},onDidChange:()=>disposable,...overrides};
    return scope;
}

test('explicit scope and index-set operations acknowledge their own selection change through the actual panel',async()=>{
    for(const [type,status]of [['indexing.scope.apply','scope_released']] as const){
        const {explorer,host}=controller();
        (host as any).windowScope=windowScope(true,{applySearchScope:async()=>{(explorer as any).select(undefined);return {status};}});
        try{
            explorer.open('#indexing/window');const panel=panels.at(-1);panel.send({v:1,type:'ready',requestId:'ready'});await flush();
            const generation=(explorer as any).indexing.snapshot().generation;
            panel.send({v:1,type,requestId:'own-scope-change',generation,selection:{index_ids:[],project_ids:[]}});await flush();
            const reply=panel.posted.find((message:any)=>message.type==='result'&&message.requestId==='own-scope-change');
            assert.equal(reply?.ok,true,type);assert.equal(reply.data.status,status);
        }finally{explorer.dispose();}
    }
});

test('the window search scope is read and applied through the host session path, and an absent capability is CAPABILITY_DISABLED',async()=>{
    const f=controller(),instance=f.explorer as any,scope=windowScope(true,{searchScopeChoices:()=>({indexes:[],projects:[],selectedIndexIds:[],selectedProjectIds:[],managedProjectIds:[],active:false,responses:[]})});
    try{
        await assert.rejects(instance.dispatch({type:'indexing.scope.read'}),(error:any)=>error.error?.code==='CAPABILITY_DISABLED','no host scope');
        (f.host as any).windowScope=windowScope(false);
        await assert.rejects(instance.dispatch({type:'indexing.scope.read'}),(error:any)=>error.error?.code==='CAPABILITY_DISABLED'&&/window-scope\/1/.test(error.message),'capability absent');
        await assert.rejects(instance.dispatch({type:'indexing.scope.apply',generation:0,selection:{index_ids:[],project_ids:[]}}),(error:any)=>error.error?.code==='CAPABILITY_DISABLED');
        (f.host as any).windowScope=scope;
        const view=await instance.dispatch({type:'indexing.scope.read',kind:'projects',next:true});
        assert.deepEqual(view.searchScope.selectedProjectIds,[],'the reply carries the host choices');
        const selection={index_ids:[],project_ids:['prj_'+'01'.repeat(16)]};
        assert.deepEqual(await instance.dispatch({type:'indexing.scope.apply',generation:0,selection}),{status:'scope'});
        assert.deepEqual(scope.calls,[['read','projects',true],['apply',selection]]);
        assert.ok(f.explorer.snapshot().searchScope,'the summary is published');
        (f.host as any).windowScope=windowScope(true,{applySearchScope:async()=>{throw Object.assign(new Error('Search scope out of date — reselect.'),{code:'scoped_read_unavailable'});}});
        await assert.rejects(instance.dispatch({type:'indexing.scope.apply',generation:0,selection}),(error:any)=>error.error?.code==='scoped_read_unavailable'&&/out of date/.test(error.message),'a refusal keeps its native code');
    }finally{f.explorer.dispose();}
});

test('a changed window search scope resets evidence, status and search state',()=>{
    const f=controller();let identity:string|undefined,changed=()=>{};
    (f.host as any).windowScope=windowScope(true,{identity:()=>identity,onDidChange:(listener:()=>void)=>{changed=listener;return disposable;}});
    const explorer=new (f.explorer.constructor as any)(f.host);
    try{
        const before=explorer.snapshot().selectionEpoch;
        changed();assert.equal(explorer.snapshot().selectionEpoch,before,'an unchanged scope keeps evidence');
        identity='granted';changed();assert.equal(explorer.snapshot().selectionEpoch,before+1);
        identity=undefined;changed();assert.equal(explorer.snapshot().selectionEpoch,before+2,'releasing the scope also resets evidence');
    }finally{explorer.dispose();f.explorer.dispose();}
});

test('read-only library and AIRK Admin requests relay complete native data and accept only this window folders',async()=>{
    const f=controller(),instance=f.explorer as any,methods:unknown[]=[],opened:unknown[]=[],index='idx_'+'12'.repeat(16);
    const response={status:'catalog_page',page:{namespace:'aa'.repeat(16),revision:3,items:[],continuation:null},capabilities:{future_native_capability:true}};
    (f.host as any).catalogControl=async(method:unknown)=>{methods.push(method);return response;};
    (f.host as any).openAdmin=async(target:unknown,choose:boolean)=>{opened.push([target,choose]);};
    (vscodeStub.workspace as any).workspaceFolders=[{uri:{scheme:'file',fsPath:'/src/demo'}},{uri:{scheme:'untitled',fsPath:'/virtual'}}];
    try{
        const reply=await instance.dispatch({type:'library.read',method:'page',kind:'indexes'});
        assert.deepEqual(methods,[{catalog_page:{request:{kind:'indexes',index_id:null,project_id:null,after:null,expected_catalog_revision:null,limit:32,max_bytes:49152}}}]);
        assert.deepEqual(reply.response,response,'the complete native response reaches the page');assert.equal(reply.error,undefined);
        (f.host as any).catalogControl=async()=>{throw Object.assign(new Error('scope gone'),{code:'scoped_read_unavailable',retryable:true});};
        const failed=await instance.dispatch({type:'library.read',method:'detail',id:index});
        assert.deepEqual(failed.error,{code:'scoped_read_unavailable',message:'scope gone',retryable:true});assert.equal(failed.response,undefined,'a failed read is not an empty library');
        assert.deepEqual(f.explorer.snapshot().workspaceFolders,['/src/demo']);
        await instance.dispatch({type:'admin.open',page:'index-library',folder:'/src/demo'});
        await assert.rejects(instance.dispatch({type:'admin.open',page:'index-library',folder:'/elsewhere'}),/folders open in this window/);
        await assert.rejects(instance.dispatch({type:'admin.open',page:'index-library',folder:'/virtual'}),/folders open in this window/);
        await instance.dispatch({type:'admin.open',page:'index-library',index,choose:true});
        assert.deepEqual(opened,[[{page:'index-library',forFolder:'/src/demo'},false],[{page:'index-library',showIndex:index},true]]);
        assert.equal(parseWebviewMessage({v:1,type:'admin.open',requestId:'a',page:'index-library',url:'https://127.0.0.1/'}),undefined,'the webview cannot name a URL');
        assert.equal(parseWebviewMessage({v:1,type:'library.read',requestId:'a',method:'page',kind:'grants'}),undefined,'grants and contexts are not part of the library');
        delete (f.host as any).catalogControl;
        await assert.rejects(instance.dispatch({type:'library.read',method:'page',kind:'indexes'}),/not available in this window/);
    }finally{delete (vscodeStub.workspace as any).workspaceFolders;f.explorer.dispose();}
});

test('notice dismissal accepts only exact bounded UTF-8 keys', () => {
    const message = { v: 1, type: 'notice.dismiss', requestId: 'dismiss', key: 'é'.repeat(4096) };
    assert.ok(parseWebviewMessage(message));
    for (const key of ['', 'é'.repeat(4097), 'x'.repeat(8193), '\u0000', 12, undefined]) {
        assert.equal(parseWebviewMessage({ ...message, key }), undefined);
    }
    assert.equal(parseWebviewMessage({ ...message, extra: true }), undefined);
});

test('notice dismissal persists across controllers and remains available without workspace trust', async () => {
    const stored = new Map<string, unknown>(), key = 'airk.explorer.dismissedNotices';
    const f = controller(false, stored), commandCount = executedCommands.length;
    try {
        await (f.explorer as any).dispatch({ type: 'notice.dismiss', key: 'runtime:1.8.10:1.8.11:ready' });
        await (f.explorer as any).dispatch({ type: 'notice.dismiss', key: 'runtime:1.8.10:1.8.11:ready' });
        assert.deepEqual(stored.get(key), ['runtime:1.8.10:1.8.11:ready']);
        assert.deepEqual(f.explorer.snapshot().dismissedNotices, stored.get(key));
        assert.equal(f.explorer.snapshot().trusted, false);
        assert.deepEqual(f.runs, [], 'presentation preferences submit no native work');
        assert.equal(executedCommands.length, commandCount);
    } finally { f.explorer.dispose(); }
    const reopened = controller(false, stored);
    try { assert.deepEqual(reopened.explorer.snapshot().dismissedNotices, stored.get(key)); }
    finally { reopened.explorer.dispose(); }
});

test('rapid notice dismissals retain every key and failed persistence is reported without hiding the notice', async () => {
    const stored = new Map<string, unknown>(), key = 'airk.explorer.dismissedNotices', f = controller(false, stored);
    const writes: string[][] = [];
    let release!: () => void, fail = false;
    const gate = new Promise<void>(resolve => { release = resolve; });
    f.host.workspaceState.update = async (_key: string, value: unknown) => {
        writes.push(value as string[]);
        if (writes.length === 1) { await gate; }
        if (fail) { throw new Error('Preference storage failed'); }
        stored.set(_key, value);
    };
    try {
        const first = (f.explorer as any).dispatch({ type: 'notice.dismiss', key: 'first' });
        const second = (f.explorer as any).dispatch({ type: 'notice.dismiss', key: 'second' });
        await Promise.resolve();
        assert.deepEqual(f.explorer.snapshot().dismissedNotices, [], 'pending storage is not a successful dismissal');
        release(); await Promise.all([first, second]);
        assert.deepEqual(stored.get(key), ['first', 'second']);
        assert.deepEqual(writes, [['first'], ['first', 'second']]);
        fail = true;
        await assert.rejects((f.explorer as any).dispatch({ type: 'notice.dismiss', key: 'failed' }), /Preference storage failed/);
        assert.deepEqual(f.explorer.snapshot().dismissedNotices, ['first', 'second']);
        fail = false;
        await (f.explorer as any).dispatch({ type: 'notice.dismiss', key: 'after-failure' });
        assert.deepEqual(stored.get(key), ['first', 'second', 'after-failure']);
    } finally { release(); f.explorer.dispose(); }
});

test('notice preferences validate stored keys and reject capacity exhaustion without evicting dismissals', async () => {
    const key = 'airk.explorer.dismissedNotices', stored = new Map<string, unknown>([[key, ['kept', null, '', '\u0000', 'é'.repeat(4097), 'kept']]]);
    const f = controller(false, stored);
    try {
        assert.deepEqual(f.explorer.snapshot().dismissedNotices, ['kept']);
        for (const invalid of ['', 'é'.repeat(4097), '\u0000']) {
            await assert.rejects((f.explorer as any).dispatch({ type: 'notice.dismiss', key: invalid }), /1–8192 UTF-8 bytes/);
        }
        const full = Array.from({ length: 512 }, (_, index) => `notice-${index}`); stored.set(key, full);
        await assert.rejects((f.explorer as any).dispatch({ type: 'notice.dismiss', key: 'new' }), /limit of 512/);
        assert.deepEqual(stored.get(key), full);
        await (f.explorer as any).dispatch({ type: 'notice.dismiss', key: 'notice-1' });
        assert.deepEqual(stored.get(key), full, 'already dismissed keys stay idempotent at capacity');
    } finally { f.explorer.dispose(); }
});

async function contextMicrotasks() { for (let index = 0; index < 12; index++) { await Promise.resolve(); } }
function automaticContextFixture() {
    const f = controller(), host = f.host as any, instance = f.explorer as any;
    let windowGeneration = 1, observation: { generation: number; response: Record<string, unknown> } | undefined;
    host.control.controller = { ...host.control.controller, observationIdentity: 'connected-runtime',
        catalogRead: async () => { throw new Error('no automatic library query'); },
        catalogPrepare: async () => { throw new Error('no automatic mutation'); }, catalogApply: async () => { throw new Error('no automatic mutation'); },
        catalogReconcile: async () => [], catalogPending: () => [],
        catalogSplitDraft: async () => { throw new Error('no automatic split'); }, catalogSplitApply: async () => { throw new Error('no automatic split'); },
        catalogSplitSourceState: async () => { throw new Error('no automatic split'); } };
    host.windowContextIdentity = () => String(windowGeneration);
    host.windowContextObservation = () => observation?.generation === windowGeneration ? observation.response : undefined;
    const observe = (generation: number) => {
        observation = { generation, response: { status: 'catalog_host_context', capabilities: { policy_contract: 3 },
            descriptor: { schema: 1, namespace: generation.toString(16).padStart(32, '0'), host: '22'.repeat(32), key: '33'.repeat(32), catalog_revision: 1, context: null } } };
    };
    return { ...f, host, instance, observe, generation: () => windowGeneration,
        nextWindow: () => { windowGeneration++; instance.observedControl(); } };
}

test('Explorer automatically observes native capabilities once per connected runtime without reconnecting',async()=>{
    const f=controller(),instance=f.explorer as any,host=f.host as any;
    let reads=0,observed=false;
    host.control.controller.observationIdentity='runtime-one';
    host.prepareWindowContext=async()=>{reads++;observed=true;};
    host.windowContextObservation=()=>observed?{status:'catalog_host_context'}:undefined;
    try{
        instance.observedControl();await flush();assert.equal(reads,1);
        instance.observedControl();instance.observeConnectedContext();await flush();assert.equal(reads,1,'routine scalar updates cannot repeat the read');
        assert.deepEqual(f.runs,[],'capability discovery must not reconnect or start native work');
        host.control.controller.observationIdentity='runtime-two';observed=false;
        instance.observedControl();await flush();assert.equal(reads,2,'activation automatically observes the replacement runtime');
        host.nativeExecutionEnabled=()=>false;host.control.controller.observationIdentity='runtime-three';
        instance.observedControl();await flush();assert.equal(reads,2,'native execution opt-out forbids observation');
    }finally{f.explorer.dispose();}
});

test('an open Explorer automatically retries known read-only capability refusals and enables splitting', async t => {
    t.mock.timers.enable({ apis: ['setTimeout', 'Date'] });
    try {
        for (const code of ['console_queue_full', 'catalog_control_connection_unavailable']) {
            const f = automaticContextFixture(); let reads = 0;
            f.host.prepareWindowContext = async () => { reads++; if (reads === 1) { throw new ServiceError(code); } f.observe(f.generation()); };
            try {
                f.explorer.open('#indexing/window'); await contextMicrotasks();
                assert.equal(reads, 1); assert.equal(f.explorer.snapshot().indexing?.canReviewSplit, false);
                f.instance.observedControl(); f.instance.observedControl(); await contextMicrotasks();
                assert.equal(reads, 1, 'routine events cannot bypass backoff or duplicate a read');
                t.mock.timers.tick(250); await contextMicrotasks();
                assert.equal(reads, 2, code); assert.equal(f.explorer.snapshot().indexing?.canReviewSplit, true, code);
                f.instance.selectionEpoch++;f.instance.observedControl();await contextMicrotasks();
                assert.equal(reads,2,'an evidence selection change cannot replace the stable native window owner');
                f.instance.observedControl(); t.mock.timers.tick(4000); await contextMicrotasks();
                assert.equal(reads, 2, 'success remains once per stable owner'); assert.deepEqual(f.runs, []);
            } finally { f.explorer.dispose(); }
        }
    } finally { t.mock.timers.reset(); }
});

test('window-generation replacement fences a late capability retry even when the control identity stays the same', async t => {
    t.mock.timers.enable({ apis: ['setTimeout', 'Date'] });
    const f = automaticContextFixture(); let reads = 0, release!: () => void;
    const held = new Promise<void>(resolve => { release = resolve; });
    f.host.prepareWindowContext = async () => {
        const generation = f.generation(); reads++;
        if (reads === 1) { throw new ServiceError('console_queue_full'); }
        if (reads === 2) { await held; }
        // This is the existing producer fence in ServiceExtension.prepareWindowContext.
        if (generation !== f.generation()) { throw new ServiceError('window_context_changed'); }
        f.observe(generation);
    };
    try {
        f.explorer.open(); await contextMicrotasks(); t.mock.timers.tick(250); await contextMicrotasks();
        assert.equal(reads, 2);
        f.nextWindow(); await contextMicrotasks();
        assert.equal(reads, 2, 'replacement waits for the existing read to settle');
        assert.equal(f.explorer.snapshot().indexing?.canReviewSplit, false);
        release(); await contextMicrotasks();
        assert.equal(reads, 3, 'the replacement window gets a fresh automatic read');
        assert.equal(f.explorer.snapshot().indexing?.canReviewSplit, true);
        assert.equal(f.explorer.snapshot().indexing?.capabilities?.policy_contract, 3);
        t.mock.timers.tick(4000); await contextMicrotasks(); assert.equal(reads, 3); assert.deepEqual(f.runs, []);
    } finally { release(); f.explorer.dispose(); t.mock.timers.reset(); }
});

test('capability retry timers stop while hidden, on disposal and after their bounded attempt budget', async t => {
    t.mock.timers.enable({ apis: ['setTimeout', 'Date'] });
    const f = automaticContextFixture(); let reads = 0;
    f.host.prepareWindowContext = async () => { reads++; throw new ServiceError('console_queue_full'); };
    try {
        f.explorer.open(); await contextMicrotasks(); f.instance.visibility(false);
        t.mock.timers.tick(4000); await contextMicrotasks(); assert.equal(reads, 1, 'hidden views have no retry timer');
        f.instance.visibility(true); await contextMicrotasks(); assert.equal(reads, 2);
        // Every read remains refused; routine updates must not create a new budget.
        for (const delay of [500, 1000, 2000, 4000, 4000, 4000]) {
            t.mock.timers.tick(delay); await contextMicrotasks(); f.instance.observedControl();
        }
        assert.equal(reads, 8); t.mock.timers.tick(60000); await contextMicrotasks();
        f.instance.observedControl(); await contextMicrotasks(); assert.equal(reads, 8);
        f.nextWindow(); await contextMicrotasks(); assert.equal(reads, 9);
        f.explorer.dispose(); t.mock.timers.tick(4000); await contextMicrotasks(); assert.equal(reads, 9);
    } finally { f.explorer.dispose(); t.mock.timers.reset(); }
});

test('capability observation never retries unknown outcomes, terminal codes or lookalike error text', async t => {
    t.mock.timers.enable({ apis: ['setTimeout', 'Date'] });
    try {
        for (const error of [new ServiceError('registry_busy'), new ServiceError('registry_busy', true), new ServiceError('invalid_saved_window_context_key'), new Error('registry_busy')]) {
            const f = automaticContextFixture(); let reads = 0;
            f.host.prepareWindowContext = async () => { reads++; throw error; };
            try {
                f.explorer.open(); await contextMicrotasks(); t.mock.timers.tick(10000); await contextMicrotasks();
                f.instance.observedControl(); await contextMicrotasks(); assert.equal(reads, 1, String(error));
            } finally { f.explorer.dispose(); }
        }
    } finally { t.mock.timers.reset(); }
});


function completeAutomaticWindow(f:ReturnType<typeof automaticContextFixture>){
    f.host.windowContext=()=>{const native=f.host.windowContextObservation()?.descriptor;return native?{host:native.host,key:native.key,kind:'folder',name:'Fixture'}:undefined;};
}

test('already complete current native owner does not start an automatic capability read',async()=>{
    const f=automaticContextFixture();completeAutomaticWindow(f);let reads=0;
    f.observe(f.generation());f.host.prepareWindowContext=async()=>{reads++;throw new Error('complete owner must not refetch');};
    try{f.instance.observedControl();await contextMicrotasks();assert.equal(reads,0);assert.equal(f.explorer.snapshot().indexing?.canReviewSplit,true);}
    finally{f.explorer.dispose();}
});

test('retired window and control observations cannot satisfy a replacement native owner',async t=>{
    t.mock.timers.enable({apis:['setTimeout','Date']});
    try{
        for(const replacement of ['window','control'] as const){
            const f=automaticContextFixture();let reads=0,release!:()=>void,payload:any;
            const held=new Promise<void>(resolve=>{release=resolve;}),owner=()=>JSON.stringify([f.generation(),f.host.control.controller.observationIdentity]);
            f.host.windowContextObservation=()=>payload?.owner===owner()?payload.response:undefined;completeAutomaticWindow(f);
            f.host.prepareWindowContext=async()=>{
                reads++;const producer=owner();if(reads===1)throw new ServiceError('console_queue_full');if(reads===2)await held;
                payload={owner:producer,response:{status:'catalog_host_context',capabilities:{policy_contract:3},descriptor:{schema:1,namespace:f.generation().toString(16).padStart(32,'0'),host:'22'.repeat(32),key:'33'.repeat(32),catalog_revision:1,context:null}}};
                f.instance.observeConnectedContext();
            };
            try{
                f.explorer.open();await contextMicrotasks();t.mock.timers.tick(250);await contextMicrotasks();assert.equal(reads,2);
                if(replacement==='window')f.nextWindow();else{f.host.control.controller.observationIdentity='replacement';f.instance.observedControl();}
                assert.equal(f.explorer.snapshot().indexing?.canReviewSplit,false);release();await contextMicrotasks();
                assert.equal(reads,3,replacement);assert.equal(payload.owner,owner());assert.equal(f.explorer.snapshot().indexing?.canReviewSplit,true);
                t.mock.timers.tick(4000);await contextMicrotasks();assert.equal(reads,3);assert.deepEqual(f.runs,[]);
            }finally{release();f.explorer.dispose();}
        }
    }finally{t.mock.timers.reset();}
});

test('external missing malformed mismatched or unknown metadata cannot satisfy a waiting observer',async t=>{
    t.mock.timers.enable({apis:['setTimeout','Date']});
    try{
        for(const malformed of ['missing','descriptor','mismatch','capability-missing','capability-array','capability-negative','capability-fraction'] as const){
            const f=automaticContextFixture();completeAutomaticWindow(f);let reads=0;
            f.host.prepareWindowContext=async()=>{reads++;throw new ServiceError('console_queue_full');};
            try{
                f.explorer.open();await contextMicrotasks();assert.equal(reads,1);
                if(malformed!=='missing'){
                    f.observe(f.generation());const observation=f.host.windowContextObservation();
                    if(malformed==='descriptor')observation.descriptor.schema=99;
                    else if(malformed==='mismatch')f.host.windowContext=()=>({host:'44'.repeat(32),key:'33'.repeat(32),kind:'folder',name:'Fixture'});
                    else if(malformed==='capability-missing')delete observation.capabilities;
                    else if(malformed==='capability-array')observation.capabilities=[];
                    else if(malformed==='capability-negative')observation.capabilities.policy_contract=-1;
                    else observation.capabilities.policy_contract=1.5;
                }
                f.instance.observeConnectedContext();assert.equal(f.instance.contextObservation.state,'waiting',malformed);
                t.mock.timers.tick(250);await contextMicrotasks();assert.equal(reads,2,malformed);assert.deepEqual(f.runs,[]);
            }finally{f.explorer.dispose();}
        }
    }finally{t.mock.timers.reset();}
});

test('Recommendations copy and temporary scope use revalidate native scope and never mutate catalog or configuration',async t=>{
    const f=controller(),instance=f.explorer as any,calls:unknown[]=[],copied:string[]=[],applied:unknown[]=[];
    const projects=['prj_'+'01'.repeat(16)],indexes=['idx_'+'02'.repeat(16)];
    let current={schema:'airk.recommendations/1',scope_identity:'scope',catalog_revision:2,selected:{project_ids:projects,index_ids:indexes,member_availability:'require'},instructions:{status:'available',text:'inert native scope instructions'}};
    instance.indexing=new IndexingCatalogController({identity:()=> 'one',authorized:()=>f.host.trusted(),changed(){},control:{catalogRead:async()=>({}),catalogPrepare:async()=>{throw new Error('unexpected mutation');},catalogApply:async()=>{throw new Error('unexpected mutation');},catalogReconcile:async()=>[],catalogPending:()=>[]}});
    instance.session=()=>({call:async(tool:string,args:unknown)=>{assert.equal(tool,'airk_query_scope');calls.push(structuredClone(args));return {payload:structuredClone(current)};}});
    (f.host as any).prefillRecommendedSearch=async(search:unknown)=>{applied.push(search);};
    (f.host as any).windowScope=windowScope(true);
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
        f.host.trusted=()=>true;current={...current,catalog_revision:2};(f.host as any).windowScope=windowScope(false);
        await instance.dispatch({type:'indexing.recommendations',arguments:{situation:'shared_dependencies'}});
        await assert.rejects(instance.dispatch({type:'indexing.recommendations.use'}),(error:any)=>error.error?.code==='CAPABILITY_DISABLED','no window scope: nothing is prefilled');
        assert.equal(applied.length,1);
    }finally{f.explorer.dispose();}
});

test('advice result is delivered after an unrelated host snapshot during the read',async()=>{
    const f=controller(),instance=f.explorer as any,indexing=instance.indexing;
    const advice={schema:'airk.recommendations/1',future_native_advice:'retained'};let release!:(value:any)=>void;
    try{
        f.explorer.open('#indexing/window');const panel=panels.at(-1);panel.send({v:1,type:'ready',requestId:'ready'});await flush();
        const pending=indexing.recommendations(()=>new Promise(resolve=>{release=resolve;}));
        instance.publish();const generation=indexing.snapshot().generation;
        release(advice);await pending;
        assert.deepEqual(panel.posted.filter((message:any)=>message.type==='indexing.session').at(-1)?.session.recommendations,advice);
        assert.equal(indexing.snapshot().generation,generation);
    }finally{f.explorer.dispose();}
});

test('new advice requests invalidate copy and use even when the same response object is returned',async t=>{
    for(const type of ['indexing.recommendations.copy','indexing.recommendations.use']){
        const f=controller(),instance=f.explorer as any,indexing=instance.indexing;let actions=0,release!:(value:any)=>void;
        const advice={schema:'airk.recommendations/1',scope_identity:'same',catalog_revision:1,selected:{project_ids:[],index_ids:[],member_availability:'require'},instructions:{status:'available',text:'scope'}};
        t.mock.method(vscodeStub.env.clipboard,'writeText',async()=>{actions++;});(f.host as any).prefillRecommendedSearch=async()=>{actions++;};
        instance.session=()=>({call:()=>new Promise(resolve=>{release=resolve;})});
        try{
            await indexing.recommendations(async()=>advice);const pending=instance.dispatch({type});const rejected=assert.rejects(pending,/stale_catalog_view/);
            await indexing.recommendations(async()=>advice);release({payload:advice});await rejected;assert.equal(actions,0);
        }finally{f.explorer.dispose();t.mock.restoreAll();}
    }
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
        await assert.rejects(instance.dispatch({type:'library.read',method:'page',kind:'indexes'}),/Restricted Mode/);
        const before=executedCommands.length;await instance.dispatch({type:'workspace.manageTrust'});
        assert.deepEqual(executedCommands.slice(before),['workbench.trust.manage']);assert.deepEqual(f.runs,[]);
        f.host.workspaceTrusted=()=>true;f.host.trusted=()=>false;
        assert.equal(f.explorer.snapshot().connection,'disconnected');
        await assert.rejects(instance.dispatch({type:'library.read',method:'page',kind:'indexes'}),/client is no longer authorized/);
        await assert.rejects(instance.dispatch({type:'workspace.manageTrust'}),/client is no longer authorized/);
        f.host.trusted=()=>true;(f.host as any).nativeExecutionEnabled=()=>false;
        await assert.rejects(instance.dispatch({type:'library.read',method:'page',kind:'indexes'}),/native_execution_disabled/);
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
        host:{trusted:()=>true},control:{state:{connected:false},projects:()=>[{project,publication:'same'}]},loadStatus:async()=>{reads++;},publish:()=>{publishes++;}});
    instance.observedControl();instance.observedControl();assert.equal(reads,1);
    instance.lastAutomaticStatusCheck=Date.now()-6000;instance.observedControl();assert.equal(reads,2);
    instance.retained=undefined;instance.lastAutomaticStatusCheck=0;instance.observedControl();assert.equal(reads,2);
    assert.equal(publishes,4);
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
    assert.equal(results.m1.error.code, 'INVALID_MESSAGE', 'management actions left the Explorer (U6, U97) and are refused without dispatch');
    assert.deepEqual(runs, [], 'no console action runs from the Explorer');
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
    (f.host as any).windowScope = windowScope(true);
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

test('U103 index actions reach the host with the selected index and return the complete outcome; retry and cancel need a job', async () => {
    const f = controller(), calls: unknown[][] = [];
    const outcome = { ok: false, argv: ['jobs', 'retry', 'cd'.repeat(32), '--service-project', project, '--output', 'json'], class: 'refused', exitCode: 3, error: { code: 'job_not_retryable', message: 'Not failed.' } };
    (f.host as any).workAction = async (...args: unknown[]) => { calls.push(args); return outcome; };
    try {
        assert.deepEqual(await (f.explorer as any).dispatch({ type: 'work.action', action: 'retry', project, job: 'cd'.repeat(32) }), outcome, 'a refused action returns its complete native outcome');
        assert.deepEqual(await (f.explorer as any).dispatch({ type: 'work.action', action: 'pause', project }), outcome);
        assert.deepEqual(calls, [['retry', project, 'cd'.repeat(32)], ['pause', project, undefined]]);
        await assert.rejects((f.explorer as any).dispatch({ type: 'work.action', action: 'cancel', project }), (error: any) => error.error.code === 'INVALID_MESSAGE');
        await assert.rejects((f.explorer as any).dispatch({ type: 'work.action', action: 'pause', project: 'ef'.repeat(32) }), (error: any) => error.error.code === 'NO_PROJECT');
        assert.equal(calls.length, 2);
        assert.ok(parseWebviewMessage({ v: 1, type: 'work.action', requestId: 'w', action: 'watch_on', project }));
        assert.equal(parseWebviewMessage({ v: 1, type: 'work.action', requestId: 'w', action: 'restart', project }), undefined);
        assert.equal(parseWebviewMessage({ v: 1, type: 'work.action', requestId: 'w', action: 'retry', project, job: 'short' }), undefined);
    } finally { f.explorer.dispose(); }
});

test('U104 Management › Agents and Save as index set reach their host hooks; their messages are strictly typed', async () => {
    const f = controller(), calls: unknown[][] = [];
    (f.host as any).agents = {
        read: async () => { calls.push(['read']); return { folder: '/w', entries: [], report: { entries: [] } }; },
        connectAgentToScope: async (client: string) => { calls.push(['connect', client]); return { status: 'pending', client, message: 'Coming in the next step.' }; },
        disconnect: async (id: string) => { calls.push(['disconnect', id]); throw Object.assign(new Error('Removal refused.'), { code: 'agents_remove_refused' }); },
    };
    const id = '0a'.repeat(16);
    try {
        assert.deepEqual(await (f.explorer as any).dispatch({ type: 'agents.read' }), { folder: '/w', entries: [], report: { entries: [] } });
        assert.deepEqual(await (f.explorer as any).dispatch({ type: 'agents.connect', client: 'vscode' }), { status: 'pending', client: 'vscode', message: 'Coming in the next step.' });
        await assert.rejects((f.explorer as any).dispatch({ type: 'agents.disconnect', connectionId: id }), (error: any) => error.error.code === 'agents_remove_refused');
        assert.deepEqual(calls, [['read'], ['connect', 'vscode'], ['disconnect', id]]);
        assert.ok(parseWebviewMessage({ v: 1, type: 'agents.connect', requestId: 'a', client: 'codex' }));
        assert.equal(parseWebviewMessage({ v: 1, type: 'agents.connect', requestId: 'a', client: 'gemini' }), undefined);
        assert.equal(parseWebviewMessage({ v: 1, type: 'agents.disconnect', requestId: 'a', connectionId: 'short' }), undefined);
        assert.ok(parseWebviewMessage({ v: 1, type: 'indexing.scope.saveSet', requestId: 'a', name: 'Set', selection: { index_ids: [], project_ids: [] } }));
        assert.equal(parseWebviewMessage({ v: 1, type: 'indexing.scope.saveSet', requestId: 'a', name: '', selection: {} }), undefined);
        assert.equal(explorerRequestPolicy('agents.disconnect').uncertain, true);
        assert.equal(explorerRequestPolicy('indexing.scope.saveSet').uncertain, true);
        assert.equal(explorerRequestPolicy('agents.read').uncertain, false);
    } finally { f.explorer.dispose(); }
    const plain = controller();
    try { await assert.rejects((plain.explorer as any).dispatch({ type: 'agents.read' }), (error: any) => error.error.code === 'CAPABILITY_DISABLED'); }
    finally { plain.explorer.dispose(); }
});

test('U103 an index in no index set is never read through the binding route; the header names the cause', async () => {
    const f = controller(); f.explorer.dispose();
    const route = { state: 'not_readable', indexId: 'idx_' + '12'.repeat(16), indexName: 'demo', code: 'index_read_unavailable', message: 'demo is in no index set, so it is not readable yet.' };
    const host = { ...f.host, readRoute: () => route, windowScope: undefined };
    const plain = new ExplorerController(host as any);
    try {
        await assert.rejects((plain as any).dispatch({ type: 'search.run', query: 'x', mode: 'lexical', limit: 5, maxBytes: 8192, fullChunks: false }),
            (error: any) => error.error.code === 'index_read_unavailable' && /in no index set/.test(error.error.message));
        await (plain as any).loadStatus();
        assert.equal(plain.snapshot().status, undefined, 'no refused status read is sent or shown as a result');
        (plain as any).publish();
        assert.equal(f.statusBar.at(-1), 'AIRK: not readable: no index set');
    } finally { plain.dispose(); }
});
