// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import Module from 'node:module';
import { test } from 'node:test';
import type { LaunchSpec } from '../model';
import { applyWorkbenchUpdate, defaultWorkbenchFormDraft, type WorkbenchState, type WorkbenchUpdate, type WorkbenchSession, type Value } from '../workbenchContracts';
import { newSession } from '../workbenchSessions';
import { renderExport } from '../workbenchRender';

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
    const created:FakeClient[]=[],messages:unknown[]=[],removed:string[]=[],saved:WorkbenchSession[]=[],exports:Buffer[]=[];
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
    const stub={EventEmitter:Emitter,workspace:{
        getConfiguration:()=>({get:(_key:string,fallback:unknown)=>fallback,inspect:()=>({globalValue:native})}),
        onDidChangeConfiguration:configuration.event,fs:{writeFile:async(_uri:unknown,bytes:Uint8Array)=>{exports.push(Buffer.from(bytes));}},
    },window:{showWarningMessage:()=>confirm(),showSaveDialog:async()=>({fsPath:'/fixture/export.json'})},commands:{executeCommand:async()=>undefined},
        Uri:{file:(fsPath:string)=>({fsPath}),joinPath:(base:{fsPath:string},...parts:string[])=>({fsPath:[base.fsPath,...parts].join('/')} )}};
    const loader=Module as unknown as {_load:(name:string,...args:any[])=>any},original=loader._load;
    loader._load=function(name:string,...args:any[]){
        if(name==='vscode'){return stub;}
        if(name==='./explorerEvidence'){return {EvidenceClient:FakeClient};}
        if(name==='./explorerWorkspaceIO'){return {openSource:async()=>undefined,saveStudioInWorkspace:async()=>({saved:false})};}
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
    return {provider,state,store,created,messages,removed,saved,exports,
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
