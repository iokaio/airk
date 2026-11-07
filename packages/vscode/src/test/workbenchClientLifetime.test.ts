// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import Module from 'node:module';
import { test } from 'node:test';
import type { LaunchSpec } from '../model';
import { applyWorkbenchUpdate, defaultWorkbenchFormDraft, object, type WorkbenchState, type WorkbenchUpdate, type WorkbenchSession, type Value } from '../workbenchContracts';
import { newSession, SessionStore } from '../workbenchSessions';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { renderExport } from '../workbenchRender';
import { EvidenceClient, EvidenceFailure } from '../explorerEvidence';
import { explorerError } from '../explorerErrors';
import { combinedScopeSelectionIdentity, scopeSelectionIdentity, type CatalogSessionSelector, type CombinedCatalogSessionSelector } from '../scopeSelection';
import { sharedCapabilityDigest } from '../sharedCapabilityFields';

function deferred<T>() { let resolve!: (value:T)=>void,reject!: (error:Error)=>void; const promise=new Promise<T>((done,fail)=>{resolve=done;reject=fail;}); return {promise,resolve,reject}; }
const tick=()=>new Promise<void>(resolve=>setImmediate(resolve));
class Emitter<T> {
    readonly listeners=new Set<(event:T)=>unknown>();
    readonly event=(listener:(event:T)=>unknown)=>{this.listeners.add(listener);return {dispose:()=>this.listeners.delete(listener)};};
    fire(event:T):void{for(const listener of this.listeners){listener(event);}}
    dispose():void{this.listeners.clear();}
}
const A='aa'.repeat(32),B='bb'.repeat(32);
const spec=(project:string):LaunchSpec=>({command:'/fixture/airk',root:`/fixture/${project}`,dataDir:`/fixture/data-${project}`,collection:'fixture',name:project===A?'A':'B',service:{installDir:'/fixture/runtime',project}});
function fixture(attach=false) {
    let specs=[spec(A),spec(B)],trusted=true,native=true;
    const changed=new Emitter<void>(),configuration=new Emitter<{affectsConfiguration(key:string):boolean}>();
    const created:FakeClient[]=[],messages:unknown[]=[],removed:string[]=[],saved:WorkbenchSession[]=[],exports:Buffer[]=[],opened:Value[]=[];
    let response:(tool:string,client:FakeClient)=>Promise<{payload:Value}> = async tool=>({payload:{tag:tool}});
    let confirm=()=>Promise.resolve('Delete sessions');
    const store={
        list:async (_project:string):Promise<WorkbenchSession[]>=>[],
        remove:async (project:string)=>{removed.push(project);},
        save:async (session:WorkbenchSession)=>{session.revision++;saved.push(structuredClone(session));},
        findShortcut:async():Promise<Value|undefined>=>undefined,
    };
    class FakeClient {
        readonly calls:string[]=[];readonly requests:{tool:string;args:Value}[]=[];disposed=0;canceled=0;
        constructor(readonly spec:LaunchSpec){created.push(this);}
        call(tool:string,args:Value={}):Promise<{payload:Value}>{this.calls.push(tool);this.requests.push({tool,args:structuredClone(args)});return response(tool,this);}
        cancel():void{this.canceled++;}
        dispose():void{this.disposed++;}
    }
    const stub={env:{clipboard:{writeText:async(value:string)=>{exports.push(Buffer.from(value));}}},EventEmitter:Emitter,ViewColumn:{Active:1},workspace:{
        getConfiguration:()=>({get:(_key:string,fallback:unknown)=>fallback,inspect:()=>({globalValue:native})}),
        onDidChangeConfiguration:configuration.event,fs:{writeFile:async(_uri:unknown,bytes:Uint8Array)=>{exports.push(Buffer.from(bytes));}},
    },window:{showWarningMessage:()=>confirm(),showSaveDialog:async()=>({fsPath:'/fixture/export.json'})},commands:{executeCommand:async()=>undefined},
        Uri:{file:(fsPath:string)=>({fsPath}),joinPath:(base:{fsPath:string},...parts:string[])=>({fsPath:[base.fsPath,...parts].join('/')} )}};
    const loader=Module as unknown as {_load:(name:string,...args:any[])=>any},original=loader._load;
    loader._load=function(name:string,...args:any[]){
        if(name==='vscode'){return stub;}
        if(name==='./explorerEvidence'){return {...original.call(this,name,...args),EvidenceClient:FakeClient};}
        if(name==='./explorerWorkspaceIO'){return {openSource:async(location:Value)=>{opened.push(location);return {message:'opened'};},saveStudioInWorkspace:async()=>({saved:false})};}
        return original.call(this,name,...args);
    };
    let WorkbenchView:typeof import('../workbenchView').WorkbenchView;
    try{delete require.cache[require.resolve('../workbenchView')];({WorkbenchView}=require('../workbenchView'));}finally{loader._load=original;}
    const host={trusted:()=>trusted,specs:()=>specs,activeProject:()=>undefined,onDidChangeSpecs:changed.event,version:'1.8.0'};
    const provider=new WorkbenchView({globalStorageUri:{fsPath:'/unused'},extensionUri:{fsPath:'/fixture/extension'}} as any,host as any);
    type Internal={sessions:typeof store;session?:WorkbenchSession;status?:Value;notice?:string;recent:unknown[];saveQueue:Promise<void>;
        select(id:string):Promise<void>;run(input:string):Promise<void>;save(session:WorkbenchSession):void;message(value:unknown):Promise<void>};
    const state=provider as unknown as Internal;state.sessions=store;
    const release=new Emitter<void>(),visibility=new Emitter<void>();
    const view={visible:true,webview:{options:{},html:'',cspSource:'fixture',asWebviewUri:(uri:{fsPath:string})=>uri.fsPath,
        postMessage:(message:unknown)=>{messages.push(structuredClone(message));return Promise.resolve(true);},onDidReceiveMessage:()=>({dispose(){}})},
        onDidChangeVisibility:visibility.event,onDidDispose:release.event};
    if(attach){provider.resolveWebviewView(view as any);}
    return {provider,state,store,created,messages,removed,saved,exports,opened,
        respond:(handler:typeof response)=>{response=handler;},confirm:(handler:typeof confirm)=>{confirm=handler;},
        specs:()=>specs,replace:(next:LaunchSpec[])=>{specs=next;changed.fire();},
        trust:(value:boolean)=>{trusted=value;changed.fire();},native:(value:boolean)=>{native=value;configuration.fire({affectsConfiguration:key=>key==='airk.runtime.nativeExecution'});},
        disposeView:()=>release.fire(),
        visible:(value:boolean)=>{view.visible=value;visibility.fire();},
    };
}

test('host draft updates are session-fenced, inert, coalesced and cannot be rolled back by queued result saves',async()=>{
    const f=fixture(true),gate=deferred<void>();
    try{
        await f.state.select(A);await tick();
        const session=f.state.session!,snapshot=()=> (f.provider as unknown as {state():WorkbenchState}).state();
        session.turns.push({id:'retained',at:1,input:'Retained query',format:'structured',envelope:{text:'private retained body'}});
        const fence={sessionId:session.id,project:A,epoch:snapshot().sessionEpoch!},nativeCalls=f.created[0].calls.length;
        f.state.saveQueue=gate.promise;f.messages.length=0;
        const pending:Promise<void>[]=[];
        for(let revision=1;revision<=10;revision++)pending.push(f.state.message({type:'formDraft',...fence,revision,tool:'search',values:{query:`draft-${revision}`,limit:'-'}}));
        await Promise.all(pending);
        f.state.save(session); // Older result snapshot is waiting behind the form write.
        await f.state.message({type:'formDraft',...fence,revision:11,tool:'references',values:{name:''},scope:'src/',recipe:'component'});
        assert.equal(f.created[0].calls.length,nativeCalls,'drafts never observe or execute native work');
        assert.ok(f.messages.every(raw=>(raw as {type:string}).type==='formDraft'));
        assert.ok(!JSON.stringify(f.messages).includes('private retained body'),'typing must not resend the transcript');
        gate.resolve();await f.state.saveQueue;
        assert.equal(f.saved.length,2,'one coalesced form write and the explicit result save share the existing queue');
        assert.ok(f.saved.every(saved=>saved.formDraft?.revision===11),'queued result writes merge current choices');
        assert.equal(session.formDraft?.values.search.query,'draft-10');
        assert.equal(session.formDraft?.activeTool,'references');
        const before=structuredClone(session.formDraft);
        await f.state.message({type:'formDraft',...fence,revision:12,project:B,tool:'search',values:{query:'wrong project'}});
        await f.state.message({type:'formDraft',...fence,revision:12,epoch:fence.epoch-1,tool:'search',values:{query:'stale epoch'}});
        await f.state.message({type:'formDraft',...fence,revision:10,tool:'search',values:{query:'stale revision'}});
        await f.state.message({type:'resetForm',...fence,revision:12,tool:'search'});
        assert.deepEqual(session.formDraft,before,'stale/foreign/inactive reset messages cannot change stored inputs');
        await f.state.message({type:'resetForm',...fence,revision:12,tool:'references'});
        assert.equal(session.formDraft?.activeTool,'references');
        assert.equal(session.formDraft?.values.references,undefined);
        assert.equal(session.formDraft?.values.search.query,'draft-10');
        assert.equal(session.formDraft?.scope,'src/');assert.equal(session.formDraft?.recipe,'component');
        await f.state.message({type:'fork'});await f.state.saveQueue;
        assert.notEqual(f.state.session?.id,session.id);
        assert.deepEqual(f.state.session?.formDraft,defaultWorkbenchFormDraft(),'a fork is a new session with default input choices');
        assert.equal(f.state.session?.turns.length,1,'forked transcript history remains available');
        await f.state.message({type:'formDraft',...fence,revision:99,tool:'search',values:{query:'old session'}});
        assert.deepEqual(f.state.session?.formDraft,defaultWorkbenchFormDraft());
        await f.provider.newSession();
        assert.deepEqual(f.state.session?.formDraft,defaultWorkbenchFormDraft());
    }finally{gate.resolve();f.provider.dispose();}
});

test('actual host rerun rejects exact symbol IDs before dropping publication or submitting another read',async()=>{
    for(const field of ['name','symbol'])for(const source of ['request','parsed']){
        const f=fixture();
        try{
            await f.state.select(A);await tick();
            const plan={publication:'old-exact-publication',digest:'old-exact-digest',steps:[{
                id:'inspect',tool:field==='name'?'symbols':'relationships',args:{[field]:'syntax:src/session.rs:10:30:struct_item'},
            }]};
            const turn={id:'old-inspect',at:1,input:'Inspect Session',format:'structured' as const,
                request:{schema:'airk.parse.request/1',mode:'execute',...(source==='request'?{plan}:{text:'/symbols syntax:src/session.rs:10:30:struct_item'})},
                ...(source==='parsed'?{envelope:{status:'complete',executed:true,plan}}:{}),
            };
            f.state.session!.turns.push(turn);
            const before=structuredClone(turn),client=f.created[0],calls=client.calls.length;
            await assert.rejects(f.state.message({type:'rerun',turn:turn.id}),/revision-bound symbol ID/);
            assert.equal(client.calls.length,calls,'no native request is submitted for a stale literal ID');
            assert.deepEqual(turn,before,'original publication-bound inputs remain available for export');
            assert.equal(f.state.session!.turns.length,1);
        }finally{f.provider.dispose();}
    }
});

test('stored literal-ID form inputs need an original publication before host Prepare or Run',async()=>{
    const f=fixture();
    try{
        await f.state.select(A);await tick();
        const session=f.state.session!,snapshot=(f.provider as unknown as {state():WorkbenchState}).state();
        const id='syntax:src/session.rs:0:17:struct_item';
        await f.state.message({type:'formDraft',sessionId:session.id,project:A,epoch:snapshot.sessionEpoch,revision:1,tool:'symbols',values:{name:id,path:'src/session.rs'}});
        await f.state.saveQueue;
        const draft=structuredClone(session.formDraft),calls=f.created[0].calls.length,turns=session.turns.length;
        const plan={steps:[{id:'form',tool:'symbols',args:{name:id,path:'src/session.rs'}}],path_prefix:'src/'};
        for(const prepare of [true,false]){
            await assert.rejects(f.state.message({type:'run',input:'symbols form',prepare,plan}),/original publication/);
            assert.equal(f.created[0].calls.length,calls,'unbound saved ID must not submit any native query');
            assert.equal(session.turns.length,turns,'rejection precedes turn/client admission');
            assert.deepEqual(session.formDraft,draft,'saved choices are not silently deleted or renamed');
        }
        const token=`airk-r1:${'a'.repeat(32)}:1:${'b'.repeat(64)}`;
        await f.state.message({type:'run',input:'Inspect retained Session',prepare:false,plan:{...plan,publication:token}});
        let request=f.created[0].requests.filter(value=>value.tool==='airk_parse').at(-1)?.args;
        assert.equal((request!.plan as Value).publication,token);
        assert.equal(((request!.plan as Value).steps as {args:Value}[])[0].args.name,id);
        await f.state.message({type:'run',input:'symbols form',prepare:false,plan:{steps:[{id:'form',tool:'symbols',args:{name:'Session'}}]}});
        request=f.created[0].requests.filter(value=>value.tool==='airk_parse').at(-1)?.args;
        assert.equal(((request!.plan as Value).steps as {args:Value}[])[0].args.name,'Session');
        assert.deepEqual(session.formDraft,draft);
    }finally{f.provider.dispose();}
});

test('actual host rerun refreshes ordinary selectors and retains durable keys without mutating saved inputs',async()=>{
    const f=fixture();
    try{
        await f.state.select(A);await tick();
        const plan={publication:'old-exact-publication',digest:'old-exact-digest',catalog:'old',collection:'old',steps:[
            {id:'named',tool:'symbols',args:{name:'Session',known_hash:'old',revision:'old',cursor:'old'}},
            {id:'durable',tool:'code_facts',args:{symbol_key:'airk-c1:durable-fixture'}},
        ]};
        const turn={id:'portable',at:1,input:'Find Session',format:'structured' as const,request:{schema:'airk.parse.request/1',mode:'execute',plan}};
        f.state.session!.turns.push(turn);
        const before=structuredClone(turn);
        await f.state.message({type:'rerun',turn:turn.id});
        const request=f.created[0].requests.filter(value=>value.tool==='airk_parse').at(-1)?.args;
        assert.ok(request);
        const fresh=request.plan as Value;
        for(const key of ['publication','digest','catalog','collection'])assert.equal(fresh[key],undefined);
        const steps=fresh.steps as {args:Value}[];
        assert.deepEqual(steps[0].args,{name:'Session'});
        assert.deepEqual(steps[1].args,{symbol_key:'airk-c1:durable-fixture'});
        assert.deepEqual(turn,before);
    }finally{f.provider.dispose();}
});

test('returning to the retained Workbench reconciles hidden results without recreating its document or client',async()=>{
    const f=fixture(true);await f.state.select(A);await tick();
    const client=f.created[0],session=f.state.session;
    f.visible(false);
    await f.state.run('/files');await tick();
    f.messages.length=0;f.visible(true);
    const update=f.messages.at(-1) as {type:string;state:WorkbenchState};
    assert.equal(update.type,'state');assert.equal(update.state.session?.id,session?.id);
    assert.ok(update.state.session?.turns.at(-1)?.envelope);
    assert.equal(f.created.length,1);assert.equal(f.created[0],client);
    f.provider.dispose();
});

test('New and Clear keep one exact client while held old query and first status cannot attach to the new transcript',async()=>{
    for(const reset of ['new','clear']){
    const f=fixture(true),status=deferred<{payload:Value}>(),query=deferred<{payload:Value}>();let statuses=0;
    f.respond(tool=>tool==='airk_parse'?query.promise:++statuses===1?status.promise:Promise.resolve({payload:{tag:'new-status'}}));
    try{
        await f.state.select(A);const previous=f.state.session!;
        const running=f.state.run('/search old');await tick();
        assert.equal(f.created[0].calls.filter(tool=>tool==='airk_parse').length,1);
        if(reset==='new'){await f.provider.newSession();}else{await f.provider.clear();}await tick();
        assert.notEqual(f.state.session!.id,previous.id);assert.equal(f.created.length,1);assert.equal(f.created[0].disposed,0);
        assert.equal(statuses,2);assert.equal(f.state.status?.tag,'new-status');
        status.resolve({payload:{tag:'old-status'}});query.resolve({payload:{status:'complete',evidence:[{text:'old body'}]}});
        await running;await tick();
        assert.equal(f.state.status?.tag,'new-status');assert.equal(f.state.session!.turns.length,0);assert.equal(previous.turns[0].envelope,undefined);
        assert.match(previous.turns[0].error!,/stopped/);assert.equal(f.saved.length,0);
    }finally{f.provider.dispose();}
    assert.equal(f.created[0].disposed,1);
    }
});

test('a held remembered shortcut cannot label a New transcript or submit its old request',async()=>{
    const f=fixture(true),shortcut=deferred<Value|undefined>();
    try{
        await f.state.select(A);const previous=f.state.session!;
        f.store.findShortcut=()=>shortcut.promise;
        const running=f.state.run('/search old remembered request');await tick();
        await f.provider.newSession();
        assert.notEqual(f.state.session!.id,previous.id);assert.equal(f.state.notice,undefined);
        shortcut.resolve({schema:'airk.parse.plan/1',steps:[]});await running;
        assert.equal(f.state.notice,undefined);
        assert.equal(f.state.session!.turns.length,0);
        assert.equal(f.created.length,1);
        assert.equal(f.created[0].calls.filter(tool=>tool==='airk_parse').length,0);
    }finally{shortcut.resolve(undefined);f.provider.dispose();}
});

test('Clear on the same project keeps its exact client and still probes status for the new transcript',async()=>{
    const f=fixture();try{
        await f.state.select(A);const previous=f.state.session!.id;
        await f.provider.clear();await tick();
        assert.deepEqual(f.removed,[A]);assert.notEqual(f.state.session!.id,previous);
        assert.equal(f.created.length,1);assert.equal(f.created[0].disposed,0);
        assert.equal(f.created[0].calls.filter(tool=>tool==='airk_status').length,2);
    }finally{f.provider.dispose();}
    assert.equal(f.created[0].disposed,1);
});

test('a late session list cannot restore a superseded project or repeat its early status',async()=>{
    const f=fixture(),listing=deferred<WorkbenchSession[]>();
    f.store.list=project=>project===A?listing.promise:Promise.resolve([]);
    try{
        const selecting=f.state.select(A);await f.state.select(B);
        listing.resolve([]);await selecting;await tick();
        assert.equal(f.state.session!.project,B);assert.equal(f.created.length,2);
        assert.equal(f.created[0].disposed,1);assert.deepEqual(f.created[0].calls,['airk_status']);
        assert.equal(f.created[1].spec.service!.project,B);assert.deepEqual(f.created[1].calls,['airk_status']);
    }finally{f.provider.dispose();}
});

test('a rejected old list is ignored after supersession without repeating its early status',async()=>{
    const f=fixture(),listing=deferred<WorkbenchSession[]>();f.store.list=project=>project===A?listing.promise:Promise.resolve([]);
    try{
        const selecting=f.state.select(A);await f.state.select(B);
        listing.reject(new Error('old list failure'));await selecting;
        assert.equal(f.state.session!.project,B);assert.equal(f.created.length,2);
        assert.equal(f.created[0].disposed,1);assert.deepEqual(f.created[0].calls,['airk_status']);
        assert.equal(f.created[1].spec.service!.project,B);assert.deepEqual(f.created[1].calls,['airk_status']);
    }finally{f.provider.dispose();}
});

test('accepted Clear drains its original deletion after save/remove waits without restoring an old selection',async()=>{
    for(const boundary of ['save','remove']){
        const f=fixture(),gate=deferred<void>();
        try{
            await f.state.select(A);
            if(boundary==='save'){f.state.saveQueue=gate.promise;}
            else{f.store.remove=async project=>{f.removed.push(project);await gate.promise;};}
            const clearing=f.provider.clear();await tick();
            await f.state.select(B);gate.resolve();await clearing;await tick();
            assert.deepEqual(f.removed,[A]);assert.equal(f.state.session!.project,B);
            assert.equal(f.created.length,2);assert.equal(f.created[0].disposed,1);
            assert.deepEqual(f.created[1].calls,['airk_status']);
        }finally{gate.resolve();f.provider.dispose();}
    }
});

// Empty saves still list history; that completion belongs to its exact session.
test('delayed old empty-save list cannot replace a New transcript recent projection',async()=>{
    const f=fixture(true),listing=deferred<WorkbenchSession[]>();
    try{
        await f.state.select(A);const previous=f.state.session!;
        const stale=newSession(A);stale.title='old saved summary';
        const fresh=newSession(A);fresh.title='current saved summary';
        let lists=0;f.store.list=()=>++lists===1?listing.promise:Promise.resolve([fresh]);
        f.state.save(previous);await tick();
        assert.equal(lists,1);
        await f.provider.newSession();const recent=structuredClone(f.state.recent);
        assert.equal((recent[0] as {title:string}).title,'current saved summary');
        listing.resolve([stale]);await f.state.saveQueue;
        assert.notEqual(f.state.session!.id,previous.id);
        assert.deepEqual(f.state.recent,recent);
    }finally{listing.resolve([]);f.provider.dispose();}
});

test('Clear confirmation cannot delete after the transcript or selected project is superseded',async()=>{
    const f=fixture(),confirmation=deferred<string>();f.confirm(()=>confirmation.promise);
    try{
        await f.state.select(A);const clearing=f.provider.clear();
        await f.state.select(B);confirmation.resolve('Delete sessions');await clearing;
        assert.deepEqual(f.removed,[]);assert.equal(f.state.session!.project,B);
    }finally{confirmation.resolve('Cancel');f.provider.dispose();}
});

test('hidden provider retains unrelated specs but synchronously closes changed selected specs, trust and native opt-out',async()=>{
    for(const loss of ['selected','removed','trust','native']){
        const f=fixture();try{
            await f.state.select(A);const client=f.created[0];
            f.replace([f.specs()[0],{...f.specs()[1],command:'/fixture/unrelated-replacement'}]);
            assert.equal(client.disposed,0);
            if(loss==='selected'){f.replace([{...f.specs()[0],command:'/fixture/selected-replacement'},f.specs()[1]]);}
            else if(loss==='removed'){f.replace([f.specs()[1]]);}
            else if(loss==='trust'){f.trust(false);}
            else{f.native(false);}
            assert.equal(client.disposed,1);assert.equal(f.state.status,undefined);
            if(loss==='removed'){await f.provider.refresh();assert.equal(f.state.session,undefined);}
            if(loss==='selected'){
                await f.provider.newSession();assert.equal(f.created.length,2);
                assert.equal(f.created[1].spec.command,'/fixture/selected-replacement');
            }
        }finally{f.provider.dispose();}
        assert.ok(f.created.every(client=>client.disposed===1));
    }
});

test('selected spec replacement during a held list disposes and fences its already-started status',async()=>{
    const f=fixture(),listing=deferred<WorkbenchSession[]>(),status=deferred<{payload:Value}>();f.store.list=()=>listing.promise;f.respond(()=>status.promise);
    try{
        const selecting=f.state.select(A);
        f.replace([{...f.specs()[0],command:'/fixture/replaced'},f.specs()[1]]);
        listing.resolve([]);await selecting;
        status.resolve({payload:{tag:'superseded'}});await tick();
        assert.equal(f.created.length,1);assert.equal(f.created[0].disposed,1);
        assert.deepEqual(f.created[0].calls,['airk_status']);assert.equal(f.state.status,undefined);
    }finally{listing.resolve([]);status.resolve({payload:{}});f.provider.dispose();}
});

test('selected readiness resolves while history remains held and starts exactly one status request', {timeout:30000}, async()=>{
    const f=fixture(true),listing=deferred<WorkbenchSession[]>(),entered=deferred<void>(),status=deferred<{payload:Value}>();
    f.store.list=()=>{entered.resolve();return listing.promise;};f.respond(()=>status.promise);
    try{
        let finished=false;const selecting=f.state.select(A).then(()=>{finished=true;});await entered.promise;
        assert.equal(f.state.session?.project,A);assert.deepEqual(f.state.recent,[]);
        assert.equal(f.created.length,1);assert.deepEqual(f.created[0].calls,['airk_status']);
        assert.ok(f.messages.some(raw=>{const value=raw as WorkbenchUpdate;return value.type==='state'&&value.state.session?.project===A;}));
        status.resolve({payload:{tag:'ready-before-history'}});await tick();
        assert.equal(finished,false);assert.equal(f.state.status?.tag,'ready-before-history');
        assert.ok(f.messages.some(raw=>{const value=raw as WorkbenchUpdate;return value.type==='status'&&value.status?.tag==='ready-before-history';}));
        const previous=newSession(A);listing.resolve([previous]);await selecting;
        assert.equal(f.state.recent.length,1);assert.equal(f.state.status?.tag,'ready-before-history');
        assert.deepEqual(f.created[0].calls,['airk_status']);
    }finally{listing.resolve([]);status.resolve({payload:{}});f.provider.dispose();}
});

test('view and provider disposal close once and discard a held status; new results retain full export values',async()=>{
    const f=fixture(true),status=deferred<{payload:Value}>();
    f.respond(()=>status.promise);
    await f.state.select(A);f.disposeView();f.provider.dispose();
    status.resolve({payload:{tag:'late'}});await tick();
    assert.equal(f.created[0].disposed,1);assert.notEqual(f.state.status?.tag,'late');
    const current=fixture(true);
    const payload:Value={status:'partial',publication:'airk-r1:fixture',truncated:true,warnings:['complete retained warning'],
        evidence:[{step:'s',citation:{path:'docs/note.md',start_line:1,end_line:1,content_hash:'ab'.repeat(32)},text:'full original evidence'}]};
    current.respond(async tool=>({payload:tool==='airk_parse'?payload:{tag:'ready'}}));
    try{
        await current.state.select(A);await current.state.run('/search new');await tick();
        assert.deepEqual(current.state.session!.turns[0].envelope,payload);
        const exported=JSON.parse(renderExport(current.state.session!,'json'));
        assert.deepEqual(exported.turns[0].envelope,payload);
        assert.ok(current.messages.length>0);
    }finally{current.provider.dispose();}
});


test('native opt-out closes native admission while local export and Clear remain available',async()=>{
    const f=fixture();
    const payload:Value={status:'complete',evidence:[{text:'retained complete value'}]};
    f.respond(async tool=>({payload:tool==='airk_parse'?payload:{tag:'ready'}}));
    try{
        await f.state.select(A);await f.state.run('/search retained');await f.state.saveQueue;
        const calls=f.created[0].calls.length;f.native(false);
        assert.equal(f.created[0].disposed,1);
        await f.state.message({type:'export',format:'json'});
        assert.deepEqual(JSON.parse(f.exports[0].toString('utf8')).turns[0].envelope,payload);
        await f.provider.clear();
        assert.deepEqual(f.removed,[A]);assert.equal(f.state.session!.turns.length,0);
        assert.equal(f.created.length,1);assert.equal(f.created[0].calls.length,calls);
        await assert.rejects(f.state.run('/search forbidden'),/Native execution is disabled/);
        assert.equal(f.created.length,1);
    }finally{f.provider.dispose();}
});

test('actual provider status/save messages omit retained bodies and replay through the real webview consumer exactly',async()=>{
    const f=fixture(true),session=newSession(A);
    session.turns=Array.from({length:200},(_,i)=>({id:`turn-${i}`,at:i,input:`request ${i}`,format:'structured' as const,envelope:{status:'partial',publication:'retained-head',calls:1,
        plan:{budgets:{calls:6}},steps:[{tool:'search',status:'complete',result:{cursor:`original-${i}`}}],
        evidence:[{step:'s',text:`retained-${i}-`+'x'.repeat(2048),citation:{path:`docs/${i}.md`,start_line:1,end_line:2,content_hash:'ab'.repeat(32)}}]}}));
    try{
        await f.state.select(A);await tick();
        Object.assign(f.store,{load:async()=>session});
        await f.state.message({type:'recent',id:session.id});
        const hostState=()=> (f.provider as unknown as {state():WorkbenchState}).state();
        const baseline=structuredClone(hostState()),exported=renderExport(session,'json');
        const status=deferred<{payload:Value}>(),entered=deferred<void>();
        f.respond(tool=>{assert.equal(tool,'airk_status');entered.resolve();return status.promise;});
        const refreshing=f.state.message({type:'refresh'});await entered.promise;
        f.messages.length=0;
        status.resolve({payload:{publication:{token:'new-status-head'},semantic_readiness:{state:'ready'}}});await refreshing;
        const statusMessages=f.messages.slice() as WorkbenchUpdate[];
        assert.equal(statusMessages.length,1);assert.equal(statusMessages[0].type,'status');
        assert.ok(!JSON.stringify(statusMessages).includes('retained-0-'));
        let rendered=statusMessages.reduce(applyWorkbenchUpdate,baseline);
        assert.deepEqual(rendered,structuredClone(hostState()));
        const fullBytes=Buffer.byteLength(JSON.stringify({type:'state',state:hostState()})),statusBytes=Buffer.byteLength(JSON.stringify(statusMessages[0]));
        assert.ok(statusBytes<fullBytes/20,'count actual encoded posted bytes; do not infer latency');
        f.messages.length=0;
        f.state.save(session);await f.state.saveQueue;
        const saveMessages=f.messages.slice() as WorkbenchUpdate[];
        assert.equal(saveMessages.length,1);assert.equal(saveMessages[0].type,'saved');
        assert.ok(!JSON.stringify(saveMessages).includes('retained-0-'));
        rendered=saveMessages.reduce(applyWorkbenchUpdate,rendered);
        assert.deepEqual(rendered,structuredClone(hostState()));
        assert.equal(renderExport(rendered.session!,'json'),exported);
        assert.equal(rendered.continuations?.['turn-199'][0].id,'page-0');
        assert.equal(f.saved[0].turns.length,200);assert.equal(f.saved[0].turns[199].envelope!.steps!==undefined,true);
        const lateStatus=structuredClone(statusMessages[0]),lateSaved=structuredClone(saveMessages[0]);
        await f.provider.newSession();const fresh=structuredClone(hostState());
        assert.equal(applyWorkbenchUpdate(fresh,lateStatus),fresh);assert.equal(applyWorkbenchUpdate(fresh,lateSaved),fresh);
    }finally{f.provider.dispose();}
});

test('history loading cannot restore an old transcript across either save-queue or load waits', {timeout:30000}, async()=>{
    for(const boundary of ['queue','load']){
        const f=fixture(true),queue=deferred<void>(),load=deferred<WorkbenchSession>(),entered=deferred<void>();
        const saved=newSession(A);let loads=0;
        Object.assign(f.store,{load:async(project:string)=>{assert.equal(project,A);loads++;entered.resolve();return load.promise;}});
        try{
            await f.state.select(A);await tick();
            if(boundary==='queue'){f.state.saveQueue=queue.promise;}
            const pending=f.state.message({type:'recent',id:saved.id});
            if(boundary==='load'){await entered.promise;}
            await f.provider.newSession();const fresh=f.state.session;
            queue.resolve();load.resolve(saved);await pending;
            assert.equal(f.state.session,fresh);assert.notEqual(f.state.session?.id,saved.id);
            assert.equal(loads,boundary==='queue'?0:1);
        }finally{queue.resolve();load.resolve(saved);f.provider.dispose();}
    }
});

test('accepted history deletion keeps its exact project and cannot reset a superseding transcript', {timeout:30000}, async()=>{
    for(const boundary of ['queue','remove']){
        const f=fixture(true),queue=deferred<void>(),remove=deferred<void>(),entered=deferred<void>();
        const removed:Array<{project:string;id:string}>=[];
        Object.assign(f.store,{remove:async(project:string,id:string)=>{removed.push({project,id});entered.resolve();if(boundary==='remove'){await remove.promise;}}});
        try{
            await f.state.select(A);await tick();const old=f.state.session!;
            if(boundary==='queue'){f.state.saveQueue=queue.promise;}
            const pending=f.state.message({type:'delete',id:old.id});
            if(boundary==='remove'){await entered.promise;await f.provider.newSession();}
            else{await f.state.select(B);}
            const fresh=f.state.session;queue.resolve();remove.resolve();await pending;
            assert.deepEqual(removed,[{project:A,id:old.id}]);assert.equal(f.state.session,fresh);
        }finally{queue.resolve();remove.resolve();f.provider.dispose();}
    }
});

test('history load errors remain visible only to the transcript that requested them', {timeout:30000}, async()=>{
    for(const superseded of [false,true]){
        const f=fixture(true),load=deferred<WorkbenchSession>(),entered=deferred<void>();
        Object.assign(f.store,{load:async()=>{entered.resolve();return load.promise;}});
        try{
            await f.state.select(A);await tick();
            const pending=f.state.message({type:'recent',id:newSession(A).id});await entered.promise;
            if(superseded){await f.provider.newSession();}
            const notice=f.state.notice;
            const observed=superseded?pending:assert.rejects(pending,/owned history read failed/);
            load.reject(new Error('owned history read failed'));await observed;
            assert.equal(f.state.notice,notice);
        }finally{f.provider.dispose();}
    }
});

const publicationToken=(generation:number,digest='cd'.repeat(32))=>`airk-r1:${'ab'.repeat(16)}:${generation}:${digest}`;
test('live host publication relations preserve separately observed status and immutable query evidence',async()=>{
    const f=fixture(true);let generation=2;
    const payload:Value={status:'complete',collection:'fixture',publication:publicationToken(3),plan:{collection:'fixture'},evidence:[]};
    f.respond(async tool=>({payload:tool==='airk_parse'?payload:{collection:'fixture',publication:{token:publicationToken(generation)},semantic_readiness:{state:'partial'},observed_unix_ms:123}}));
    const snapshot=()=> (f.provider as unknown as {state():WorkbenchState}).state();
    try{
        await f.state.select(A);await tick();const originalStatus=structuredClone(f.state.status);
        await f.state.run('/search observed');await f.state.saveQueue;
        const turn=f.state.session!.turns[0];
        assert.equal(snapshot().publicationRelations?.[turn.id],'newer');
        assert.deepEqual(f.state.status,originalStatus,'query publication never promotes status, readiness or observation time');
        assert.deepEqual(turn.envelope,payload);
        const exported=renderExport(f.state.session!,'json');
        generation=4;const calls=f.created[0].calls.length;f.messages.length=0;
        await f.state.message({type:'refresh'});
        assert.equal(f.created[0].calls.length,calls+1,'only the explicit refresh observes status');
        assert.equal(snapshot().publicationRelations?.[turn.id],'older');
        const delta=f.messages.find(raw=>(raw as WorkbenchUpdate).type==='status') as Extract<WorkbenchUpdate,{type:'status'}>;
        assert.equal(delta.publicationRelations?.[turn.id],'older');
        assert.ok(!JSON.stringify(delta).includes('/fixture/'),'captured launcher authority never reaches the webview');
        assert.equal(renderExport(f.state.session!,'json'),exported,'observation metadata is outside saved evidence and exports');
        generation=3;await f.state.message({type:'refresh'});
        assert.equal(snapshot().publicationRelations?.[turn.id],'equal');
        assert.deepEqual(turn.envelope,payload);
        assert.deepEqual(f.created[0].calls,['airk_status','airk_parse','airk_status','airk_status']);
    }finally{f.provider.dispose();}
});

test('saved and forked turns lack authority; changing a live launcher never fabricates publication age',async()=>{
    const f=fixture(true),snapshot=()=> (f.provider as unknown as {state():WorkbenchState}).state();
    f.respond(async tool=>({payload:tool==='airk_parse'?{status:'complete',collection:'fixture',publication:publicationToken(1)}:{collection:'fixture',publication:{token:publicationToken(2)}}}));
    try{
        await f.state.select(A);await tick();await f.state.run('/search retained');await f.state.saveQueue;
        const turn=f.state.session!.turns[0];assert.equal(snapshot().publicationRelations?.[turn.id],'older');
        const saved=structuredClone(f.state.session!);
        await f.state.message({type:'fork'});await f.state.saveQueue;
        assert.equal(snapshot().publicationRelations?.[turn.id],'unknown');
        Object.assign(f.store,{load:async()=>saved});await f.state.message({type:'recent',id:saved.id});
        assert.equal(snapshot().publicationRelations?.[turn.id],'unknown');
        await f.state.run('/search live');await f.state.saveQueue;
        const live=f.state.session!.turns.at(-1)!;assert.equal(snapshot().publicationRelations?.[live.id],'older');
        const changed={...f.specs()[0],dataDir:'/fixture/changed-data'};
        f.replace([changed,f.specs()[1]]);await tick();
        assert.equal(snapshot().publicationRelations?.[live.id],'unknown','old status is invalidated when authority changes');
        await f.state.message({type:'refresh'});
        assert.equal(snapshot().publicationRelations?.[live.id],'different','equal token shapes under a changed authority have no order');
    }finally{f.provider.dispose();}
});

test('explicit held status replies remain fenced across same-project launcher replacement',async()=>{
    const f=fixture(),held=deferred<{payload:Value}>(),entered=deferred<void>();
    f.respond(async()=>({payload:{collection:'fixture',publication:{token:publicationToken(1)}}}));
    try{
        await f.state.select(A);await tick();
        f.respond(()=>{entered.resolve();return held.promise;});
        const refreshing=f.state.message({type:'refresh'});await entered.promise;
        f.replace([{...f.specs()[0],dataDir:'/fixture/replaced'},f.specs()[1]]);
        held.resolve({payload:{collection:'fixture',publication:{token:publicationToken(99)},tag:'late-foreign-authority'}});
        await refreshing;assert.notEqual(f.state.status?.tag,'late-foreign-authority');
    }finally{held.resolve({payload:{}});f.provider.dispose();}
});


test('error-only turns retain canonical native refusals in full copies and actual export handlers',async()=>{
    const f=fixture();const original={isError:true,content:[{type:'text',text:'{"error":{"code":"scope_revoked","message":"Access changed"},"scope_observation":{"schema":1},"retry":{"allowed":false}}'}],extra:{untouched:'```\nembedded'}};
    try{
        await f.state.select(A);await tick();
        f.respond(async()=>{throw new EvidenceFailure(explorerError('scope_revoked','Access changed'),original);});
        await f.state.run('/status');await f.state.saveQueue;
        const session=f.state.session!,turn=session.turns.at(-1)!;
        assert.equal(turn.error,explorerError('scope_revoked').message);assert.equal(turn.envelope,undefined);assert.deepEqual(turn.nativeFailure,original);
        const json=JSON.parse(renderExport(session,'json',turn.id));assert.deepEqual(json.turns[0].nativeFailure,original);
        const md=renderExport(session,'markdown',turn.id);assert.match(md,/Complete original native refusal/);assert.ok(md.includes('untouched'));
        await f.state.message({type:'export',turn:turn.id,format:'json'});
        assert.deepEqual(JSON.parse(f.exports.at(-1)!.toString()).turns[0].nativeFailure,original);
        f.respond(async()=>{throw new Error('No native response');});await f.state.run('/status');
        assert.equal(session.turns.at(-1)!.nativeFailure,undefined);
    }finally{f.provider.dispose();}
});

const scopedSelector=(project:string):CatalogSessionSelector=>({schema:1,context_key:'12'.repeat(32),namespace:'34'.repeat(16),expected_catalog_revision:8,grant_id:'grt_'+ '11'.repeat(16),expected_grant_revision:7,index_id:'idx_'+ '22'.repeat(16),project_ids:['prj_'+project.repeat(16)]});
const combinedSelector=():CombinedCatalogSessionSelector=>({schema:1,context_key:'12'.repeat(32),namespace:'34'.repeat(16),expected_catalog_revision:8,grant_id:'grt_'+'11'.repeat(16),expected_grant_revision:7,project_ids:['prj_'+'44'.repeat(16)],index_ids:['idx_'+'22'.repeat(16),'idx_'+'33'.repeat(16)]});

test('explicit recommended subset only prefills the matching Workbench form and cannot widen its connection',async()=>{
    const f=fixture(),selected={...spec(A),combinedSession:combinedSelector()};
    const draft={query:'',mode:'lexical' as const,limit:12,maxBytes:8192,fullChunks:false,project_ids:selected.combinedSession.project_ids,index_ids:[selected.combinedSession.index_ids![0]],member_availability:'report' as const};
    try{
        f.replace([selected]);
        await f.provider.prefillSearchForm(selected,{...draft,query:'Keep this task',mode:'hybrid',limit:9,maxBytes:4096,pathPrefix:'docs/',fullChunks:true,index_ids:selected.combinedSession.index_ids,member_availability:'require'});
        await f.provider.prefillSearchForm(selected,draft,true);await f.state.saveQueue;
        assert.equal(f.created.length,0,'prefilling starts no native client');
        assert.equal(f.state.session!.formDraft?.values.search.index_ids,JSON.stringify(draft.index_ids));
        const values=f.state.session!.formDraft!.values.search;
        assert.equal(values.query,'Keep this task');assert.equal(values.mode,'hybrid');assert.equal(values.path_prefix,'docs/');
        assert.equal(values.limit,'9');assert.equal(values.max_bytes,'4096');assert.equal(values.full_chunks,'true');assert.equal(values.member_availability,'require');
        assert.deepEqual(selected.combinedSession.index_ids,combinedSelector().index_ids,'the frozen connection is unchanged');
        await assert.rejects(f.provider.prefillSearchForm(selected,{...draft,index_ids:['idx_'+'99'.repeat(16)]},true),/outside/);
        await assert.rejects(f.provider.prefillSearchForm(selected,draft),/does not match/,'ordinary Inbox drafts still require an exact session match');
    }finally{f.provider.dispose();}
});

test('combined search Apply stores exact editable inputs and launches no evidence process',async()=>{
    const f=fixture(),selected={...spec(A),combinedSession:combinedSelector()};
    try{
        f.replace([selected]);
        await f.provider.prefillSearchForm(selected,{query:'review retry',mode:'lexical',limit:8,maxBytes:4096,fullChunks:false,pathPrefix:'src/',
            project_ids:selected.combinedSession.project_ids,index_ids:selected.combinedSession.index_ids,member_availability:'require'});
        await f.state.saveQueue;
        assert.equal(f.created.length,0,'Apply never opens an MCP session, reads readiness, prepares a plan or runs evidence');
        assert.equal(f.state.session!.project,combinedScopeSelectionIdentity(selected.combinedSession));
        assert.deepEqual(f.state.session!.formDraft!.values.search,{query:'review retry',mode:'lexical',limit:'8',max_bytes:'4096',full_chunks:'false',path_prefix:'src/',
            project_ids:JSON.stringify(selected.combinedSession.project_ids),index_ids:JSON.stringify(selected.combinedSession.index_ids),member_availability:'require'});
        assert.equal(f.state.session!.turns.length,0);
        await assert.rejects(f.provider.prefillSearchForm({...selected,combinedSession:{...selected.combinedSession,project_ids:['prj_'+'55'.repeat(16)]}},
            {query:'changed',mode:'policy',limit:8,maxBytes:4096,fullChunks:false}),/scope changed/);
        assert.equal(f.created.length,0);
    }finally{f.provider.dispose();}
});

test('actual Workbench host verifies shared capability display while durable/native copies retain exact references',async()=>{
    const f=fixture(),fields={adapters:[{id:'original'}],workbench:{operations:{search:{type:'object'}}}},id=sharedCapabilityDigest(fields),stepId='__proto__';
    const member={request_admission:{limit:16},$airk_shared_capability_fields:id},combined={schema:'airk.combined-read/1',shared_values:{schema:'airk.shared-capability-fields/1',values:{[id]:fields}},
        members:[{index_id:'idx_'+'12'.repeat(16),outcome:'complete',result:{content:[{type:'text',text:JSON.stringify(member)}],structuredContent:member,isError:false}}],ranking:[]};
    try{
        await f.state.select(A);await tick();
        f.respond(async()=>({payload:{status:'complete',executed:true,steps:[{id:stepId,tool:'capabilities',status:'complete',result:combined}]}}));
        await f.state.run('/capabilities');await f.state.saveQueue;
        const turn=f.state.session!.turns[0],raw=structuredClone(turn.envelope),snapshot=()=> (f.provider as unknown as {state():WorkbenchState}).state();
        const display=snapshot().capabilityDetails![turn.id][stepId];assert.equal(display.state,'verified');if(display.state==='verified')assert.deepEqual(display.values[id],fields);
        assert.equal(JSON.parse(JSON.stringify(snapshot())).capabilityDetails[turn.id][stepId].state,'verified','valid native step IDs survive the actual host state message');
        assert.deepEqual(turn.envelope,raw);const returned=object((turn.envelope!.steps as Value[])[0].result),nativeMember=object((returned.members as Value[])[0].result);
        assert.equal(object(nativeMember.structuredContent).$airk_shared_capability_fields,id);
        await f.state.message({type:'export',turn:turn.id,format:'json'});assert.deepEqual(JSON.parse(f.exports.at(-1)!.toString()).turns[0].envelope,raw);
        assert.ok(!JSON.stringify(f.saved.at(-1)).includes('capabilityDetails'),'verified display tables are not persisted as native evidence');
        const altered=structuredClone(raw!);object(object(object((altered.steps as Value[])[0].result).shared_values).values)[id]={adapters:[]};turn.envelope=altered;
        const calls=f.created[0].calls.length;assert.equal(snapshot().capabilityDetails![turn.id][stepId].state,'unavailable');assert.equal(f.created[0].calls.length,calls,'invalid saved capability data never starts native work');
    }finally{f.provider.dispose();}
});

test('combined card context retains originating index, Projects and member publication through the real host run path',async()=>{
    const f=fixture(),selected={...spec(A),combinedSession:combinedSelector()},originIndex=selected.combinedSession.index_ids![1],projects=selected.combinedSession.project_ids;
    try{
        f.replace([selected]);await f.provider.prefillSearchForm(selected,{query:'search',mode:'lexical',limit:8,maxBytes:4096,fullChunks:false,project_ids:projects,index_ids:selected.combinedSession.index_ids});
        f.state.session!.turns.push({id:'qualified',at:1,input:'search',format:'structured',envelope:{evidence:[{step:'search',text:'original',publication:publicationToken(4),
            citation:{path:'same.rs',content_hash:'66'.repeat(32),start_line:1,end_line:1,location:{Lines:{start:1,end:1}}},
            origin:{index_id:originIndex,project_ids:projects,source_root_id:'src_'+'77'.repeat(16),source_relative_path:'same.rs',publication:{token:publicationToken(4)}}}]}});
        await f.state.message({type:'context',turn:'qualified',card:'record-0'});
        const request=f.created[0].requests.find(value=>value.tool==='airk_parse')!.args,plan=object(request.plan),step=object((plan.steps as unknown[])[0]),args=object(step.args);
        assert.deepEqual(request.project_ids,projects);assert.deepEqual(request.index_ids,[originIndex]);assert.deepEqual(plan.query_scope,{project_ids:projects,index_ids:[originIndex]});
        assert.equal(plan.publication,undefined,'a combined plan has no invented scalar publication');assert.equal(args.revision,publicationToken(4));assert.equal(args.known_hash,'66'.repeat(32));
        assert.equal(args.before,undefined,'context actions use the canonical native radius default');assert.equal(args.after,undefined);
        assert.equal(args.path,'same.rs');assert.equal(f.created[0].spec.combinedSession?.index_ids!.length,2,'the temporary follow-up never changes the frozen window session');
    }finally{f.provider.dispose();}
});

test('combined host publication observations compare member vectors and preserve captured result identity',async()=>{
    const f=fixture(),selected={...spec(A),combinedSession:combinedSelector()};let generations=[2,2];
    const vector=()=>({authority:'88'.repeat(32),members:selected.combinedSession.index_ids!.map((index_id,i)=>({index_id,publication:{collection:'fixture',token:publicationToken(generations[i])}}))});
    f.respond(async tool=>({payload:tool==='airk_parse'?{executed:true,status:'complete',steps:[{id:'search',tool:'search',status:'complete',result:{schema:'airk.combined-read/1',publication_vector:vector(),members:[],ranking:[]}}]}:{schema:'airk.combined-read/1',publication_vector:vector(),members:[],ranking:[]}}));
    const snapshot=()=> (f.provider as unknown as {state():WorkbenchState}).state();
    try{
        f.replace([selected]);await f.provider.useScope(selected);await tick();await f.state.run('/status');
        const turn=f.state.session!.turns[0];assert.equal(snapshot().publicationRelations?.[turn.id],'equal');
        const original=structuredClone(turn.envelope);generations=[3,1];await f.state.message({type:'refresh'});assert.equal(snapshot().publicationRelations?.[turn.id],'mixed');
        assert.deepEqual(turn.envelope,original);generations=[3,3];await f.state.message({type:'refresh'});assert.equal(snapshot().publicationRelations?.[turn.id],'older');
        const current=f.state.session;const refreshed={...selected,combinedSession:{...selected.combinedSession,expected_catalog_revision:9}};f.replace([refreshed]);await f.provider.useScope(refreshed);await f.state.message({type:'refresh'});
        assert.equal(f.state.session,current);assert.equal(snapshot().publicationRelations?.[turn.id],'older','a metadata clock refresh does not change causal query authority');
    }finally{f.provider.dispose();}
});

test('combined source Open checks the held canonical root and relative path against native metadata ranking',async()=>{
    const f=fixture(),selected={...spec(A),combinedSession:combinedSelector()},indexId=selected.combinedSession.index_ids![0],projects=selected.combinedSession.project_ids,rootId='src_'+'77'.repeat(16);let returnedRoot=rootId;
    try{
        f.replace([selected]);await f.provider.prefillSearchForm(selected,{query:'search',mode:'lexical',limit:8,maxBytes:4096,fullChunks:false,project_ids:projects,index_ids:selected.combinedSession.index_ids});
        const origin={index_id:indexId,project_ids:projects,source_root_id:rootId,source_relative_path:'same.rs',publication:{token:publicationToken(4)}};
        f.state.session!.turns.push({id:'qualified-open',at:1,input:'search',format:'structured',envelope:{evidence:[{step:'search',text:'original',publication:publicationToken(4),
            citation:{path:'same.rs',content_hash:'66'.repeat(32),start_line:1,end_line:1,location:{Lines:{start:1,end:1}}},origin}]}});
        f.respond(async tool=>({payload:tool==='airk_file_info'?{schema:'airk.combined-read/1',members:[{index_id:indexId,outcome:'complete',result:{content:[],structuredContent:{path:'same.rs',content_hash:'66'.repeat(32),source:{root:'/native/original',relative_path:'same.rs'},line_basis:'source'}}}],
            ranking:[{index_id:indexId,result_pointer:'/members/0/result/structuredContent',origin:{...origin,source_root_id:returnedRoot}}]}:{}}));
        await f.state.message({type:'open',turn:'qualified-open',card:'record-0'});assert.equal(f.opened.length,1);assert.equal(f.opened[0].root,'/native/original');
        returnedRoot='src_'+'88'.repeat(16);
        await assert.rejects(f.state.message({type:'open',turn:'qualified-open',card:'record-0'}),/source identity changed/);assert.equal(f.opened.length,1,'changed canonical root cannot open a same-named file');
    }finally{f.provider.dispose();}
});
test('scoped histories separate same backing subsets, preserve old turns, and fence late replies',async()=>{
    const f=fixture(),one={...spec(A),catalogSession:scopedSelector('44')},two={...spec(A),catalogSession:scopedSelector('55')};
    const keyOne=scopeSelectionIdentity(one.catalogSession),keyTwo=scopeSelectionIdentity(two.catalogSession);
    const pending=deferred<{payload:Value}>(),entered=deferred<void>();
    try{
        f.replace([one,two]);await f.state.select(keyOne);await tick();
        f.respond(async()=>({payload:{status:'complete',captured:'old',scope_observation:{fingerprint:'old'}}}));
        await f.state.run('/status');await f.state.saveQueue;
        const oldSession=f.state.session!,oldTurn=structuredClone(oldSession.turns[0]);
        f.respond(async tool=>{if(tool==='airk_parse'){entered.resolve();return pending.promise;}return {payload:{}};});
        const running=f.state.run('/status');await entered.promise;const oldClient=f.created.at(-1)!;
        await f.provider.useScope(two);assert.equal(f.state.session!.project,keyTwo);assert.ok(oldClient.disposed>0);
        pending.resolve({payload:{captured:'late'}});await running;
        assert.deepEqual(oldSession.turns[0],oldTurn);assert.equal(oldSession.turns.at(-1)!.envelope,undefined);
        assert.equal(f.state.session!.turns.length,0);assert.equal(f.removed.length,0);
        const same=f.state.session!;const refreshed={...two,catalogSession:{...two.catalogSession,expected_catalog_revision:9}};
        f.replace([one,refreshed]);await f.provider.useScope(refreshed);
        assert.equal(f.state.session,same,'catalog clock reconnects without renaming or deleting transcript');
        assert.notEqual(keyOne,keyTwo);assert.equal(f.created[0].spec.service!.project,A,'native backing remains unchanged');
    }finally{pending.resolve({payload:{}});f.provider.dispose();}
});


test('scoped freshness comparisons require matching native scope observations and fingerprint',async()=>{
    const f=fixture(),selected={...spec(A),catalogSession:scopedSelector('44')};let fingerprint='66'.repeat(32),include=true;
    const observation=()=>({schema:1,qualifier:'selected_subset',provenance:'user_reviewed_project_grant',fingerprint,
        grant:{id:selected.catalogSession.grant_id,revision:selected.catalogSession.expected_grant_revision},
        members:[{index_id:selected.catalogSession.index_id,memberships:selected.catalogSession.project_ids.map(project_id=>({project_id}))}]});
    f.respond(async()=>({payload:{collection:'fixture',publication:{token:publicationToken(1)},...(include?{scope_observation:observation()}:{})}}));
    const snapshot=()=> (f.provider as unknown as {state():WorkbenchState}).state();
    try{
        f.replace([selected]);await f.provider.useScope(selected);await tick();await f.state.run('/status');
        const turn=f.state.session!.turns[0];assert.equal(snapshot().publicationRelations?.[turn.id],'equal');
        fingerprint='77'.repeat(32);await f.state.message({type:'refresh'});
        assert.equal(snapshot().publicationRelations?.[turn.id],'different','same token under a different native scope has no freshness order');
        include=false;await f.state.message({type:'refresh'});assert.equal(snapshot().publicationRelations?.[turn.id],'unknown');
        assert.equal((turn.envelope!.scope_observation as Value).fingerprint,'66'.repeat(32),'captured scope remains immutable');
    }finally{f.provider.dispose();}
});


test('actual evidence failure through Workbench and durable privacy paths never saves native details as guidance',async()=>{
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-native-refusal-privacy-'));
    const marker='PRIVATE_NATIVE_SOURCE_MARKER';
    try{
        for(const body of [{error:{code:'invalid_argument',message:marker}},{error:{code:'invalid_argument'},details:{source:marker}},marker]){
            const f=fixture(),store=new SessionStore(root);
            const original={isError:true,content:[{type:'text',text:typeof body==='string'?body:JSON.stringify(body)}]};
            const actual=new EvidenceClient(spec(A),'privacy-test',{workbench:true,open:async()=>({session:{closed:false,close(){},request:async()=>original} as any})});
            Object.assign(f.state,{sessions:store});
            f.respond((tool)=>actual.call(tool as 'airk_status'|'airk_parse',{}));
            try{
                await f.state.select(A);await tick();await f.state.run('/status');await f.state.saveQueue;
                const session=f.state.session!,turn=session.turns[0];
                assert.deepEqual(turn.nativeFailure,original,'full native result remains available when results are retained');
                assert.ok(!turn.error?.includes(marker));assert.ok(turn.error);
                assert.ok(JSON.stringify(await store.load(A,session.id)).includes(marker));
                await store.save(session,false);
                const inputsOnly=await store.load(A,session.id);
                assert.ok(!JSON.stringify(inputsOnly).includes(marker));assert.equal(inputsOnly.turns[0].error,undefined);assert.equal(inputsOnly.turns[0].input,'/status');
                // Older saved sessions may already have copied raw native detail
                // into error. Privacy cleanup must remove that historical copy.
                session.turns[0].error=marker;await store.save(session,true);await store.clearResults(A);
                const cleared=await store.load(A,session.id);assert.ok(!JSON.stringify(cleared).includes(marker));
            }finally{actual.dispose();f.provider.dispose();}
        }
    }finally{await fs.rm(root,{recursive:true,force:true});}
});

test('actual host forwards opt-in and preserves flat native and historical nested reports',async()=>{
    const report={schema:'airk.profile-families/1',file:{path:'api/policy.md',publication:'one-head',source_hash:'ab'.repeat(32)},applied_file:{adapter_id:'markdown',effective_profile_digest:'cd'.repeat(32)},configured_collection_policy:{state:'unavailable',reason:'selected_scope'},published_collection_policy:{state:'unavailable',reason:'selected_scope'},pending_collection_reindex:null,
        families:Array.from({length:18},(_,i)=>({family:`family-${i}`,support:'unknown',retention:'selected',reply_inclusion:'omitted'})),future_native_field:{complete:'keep-all-fields'}};
    for(const shape of ['flat','historical_nested'] as const){
    const envelope={status:'complete',executed:true,publication:'one-head',calls:1,steps:[{id:'info',tool:'file_info',status:'complete',result:shape==='flat'?{read:{status:'content'},path:'api/policy.md',profile_families:report}:{read:{status:'content'},content:{path:'api/policy.md',profile_families:report}}}],evidence:[{step:'info',text:'selected evidence',publication:'one-head',citation:{path:'api/policy.md',start_line:1,end_line:1,content_hash:'ab'.repeat(32)}}]};
    const f=fixture(true);
    try{
        f.respond(async tool=>({payload:tool==='airk_parse'?envelope:{}}));
        await f.state.select(A);
        await f.state.message({type:'run',input:'Inspect profile families',prepare:false,plan:{steps:[{id:'info',tool:'file_info',args:{path:'api/policy.md',include_profile_families:true}}]}});
        const call=f.created.flatMap(client=>client.requests).find(call=>call.tool==='airk_parse'&&object(call.args.plan).steps);
        assert.equal(object((object(call?.args.plan).steps as Value[])[0].args).include_profile_families,true);
        const turn=f.state.session!.turns.at(-1)!;
        const result=object((turn.envelope!.steps as Value[])[0].result);
        assert.equal(Object.hasOwn(result,'content'),shape==='historical_nested');
        assert.deepEqual((shape==='flat'?result:object(result.content)).profile_families,report);
        for(const type of ['copy','export'])for(const format of ['json','markdown'])for(const cards of [undefined,['record-0']])await f.state.message({type,turn:turn.id,format,cards});
        assert.equal(f.exports.length,8);
        for(const output of f.exports){assert.ok(output.includes('keep-all-fields'));assert.ok(output.includes('family-17'));assert.ok(output.includes('selected_scope'));}
    }finally{f.provider.dispose();}
    }
});
