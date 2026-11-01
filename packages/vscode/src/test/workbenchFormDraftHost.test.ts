// SPDX-License-Identifier: Apache-2.0
/** Independent host boundary checks: local per-session choices never become evidence calls. */
import assert from 'node:assert/strict';
import Module from 'node:module';
import test from 'node:test';
import { applyWorkbenchUpdate, defaultWorkbenchFormDraft, normalizeWorkbenchFormDraft, object, parseWorkbenchMessage, Value, WorkbenchSession, WorkbenchState } from '../workbenchContracts';
import { newSession } from '../workbenchSessions';

const PROJECT='ab'.repeat(32),OTHER='cd'.repeat(32);
class Emitter<T>{
    readonly event=(_listener:(event:T)=>unknown)=>({dispose(){}});
    fire(_value:T):void{}
    dispose():void{}
}
function hostFixture(persistSessions=true){
    const records=new Map<string,WorkbenchSession>(),calls:string[]=[],messages:unknown[]=[];
    const saved:WorkbenchSession[]=[];
    const spec={command:'/fictional/airk',root:'/fictional/source',dataDir:'/fictional/data',collection:'fictional',name:'Fixture',service:{installDir:'/fictional/runtime',project:PROJECT}};
    class Client{
        readonly spec=spec;
        call(tool:string):Promise<{payload:Value}>{calls.push(tool);return Promise.resolve({payload:{index_state:'available'}});}
        cancel():void{}dispose():void{}
    }
    const vscode={EventEmitter:Emitter,workspace:{
        getConfiguration:()=>({get:(key:string,fallback:unknown)=>key==='workbench.persistSessions'?persistSessions:fallback,inspect:()=>({globalValue:true})}),
        onDidChangeConfiguration:()=>({dispose(){}}),
    },window:{showWarningMessage:async()=> 'Delete sessions'},commands:{executeCommand:async()=>undefined},
        Uri:{joinPath:(base:{fsPath:string},...parts:string[])=>({fsPath:[base.fsPath,...parts].join('/')}),file:(fsPath:string)=>({fsPath})}};
    const loader=Module as unknown as {_load:(name:string,...args:any[])=>any},original=loader._load;
    loader._load=function(name:string,...args:any[]){
        if(name==='vscode')return vscode;
        if(name==='./explorerEvidence')return {EvidenceClient:Client};
        if(name==='./explorerWorkspaceIO')return {openSource:async()=>undefined,saveStudioInWorkspace:async()=>({saved:false})};
        return original.call(this,name,...args);
    };
    let View:typeof import('../workbenchView').WorkbenchView;
    try{delete require.cache[require.resolve('../workbenchView')];({WorkbenchView:View}=require('../workbenchView'));}finally{loader._load=original;}
    const host={trusted:()=>true,specs:()=>[spec],activeProject:()=>PROJECT,onDidChangeSpecs:()=>({dispose(){}}),version:'1.8.3'};
    const provider=new View!({globalStorageUri:{fsPath:'/unused'},extensionUri:{fsPath:'/fictional/extension'}} as any,host as any);
    type Internal={session?:WorkbenchSession;sessions:unknown;saveQueue:Promise<void>;state():WorkbenchState;message(message:unknown):Promise<void>};
    const state=provider as unknown as Internal;
    const store={
        save:async(session:WorkbenchSession)=>{session.revision++;records.set(session.id,structuredClone(session));saved.push(structuredClone(session));return [];},
        list:async(project:string)=>[...records.values()].filter(record=>record.project===project).map(record=>structuredClone(record)),
        load:async(project:string,id:string)=>{const session=records.get(id);assert.equal(session?.project,project);return structuredClone(session);},
        remove:async(project:string,id?:string)=>{for(const [key,session]of records)if(session.project===project&&(!id||key===id))records.delete(key);},
    };
    state.sessions=store;
    state.session=newSession(PROJECT);
    const send=async(value:unknown)=>{const parsed=parseWorkbenchMessage(value);assert.ok(parsed,'fixture sends an accepted message shape');messages.push(parsed);await state.message(parsed);await state.saveQueue;};
    const choice=(tool:string,values?:Record<string,string>,extra:Value={})=>{
        const current=state.state(),draft=object(object(current.session).formDraft);
        return {type:'formDraft',sessionId:current.session!.id,project:current.session!.project,epoch:current.sessionEpoch,
            revision:Number(draft.revision??0)+1,tool,...(values?{values}:{}),...extra};
    };
    const reset=()=>({...choice(String(object(object(state.session).formDraft).activeTool??'search')),type:'resetForm'});
    return {provider,state,store,records,calls,saved,messages,send,choice,reset};
}

test('host saves local form choices per operator and resets only the active tool values',async()=>{
    const f=hostFixture();
    try{
        await f.send(f.choice('search',{query:'Session',language:'rust'},{scope:'src/session/',recipe:'component'}));
        await f.send(f.choice('symbols',{name:'Session',name_match:'suffix',compact:'false'}));
        const draft=object(object(f.state.session).formDraft),values=object(draft.values);
        assert.equal(draft.activeTool,'symbols');
        assert.equal(object(values.search).query,'Session');
        assert.equal(object(values.symbols).name_match,'suffix');
        assert.equal(draft.scope,'src/session/');assert.equal(draft.recipe,'component');
        await f.send(f.reset());
        const reset=object(object(f.state.session).formDraft);
        assert.equal(reset.activeTool,'symbols');
        assert.deepEqual(object(object(reset.values).symbols),{});
        assert.equal(object(object(reset.values).search).query,'Session');
        assert.equal(reset.scope,'src/session/');assert.equal(reset.recipe,'component');
        assert.deepEqual(f.calls,[],'changes and reset cannot create or call a native evidence client');
        assert.ok(f.saved.length>=3,'accepted local choices use session persistence');
    }finally{f.provider.dispose();}
});

test('host ignores stale project, session, epoch and revision messages without query execution',async()=>{
    const f=hostFixture();
    try{
        await f.send(f.choice('search',{query:'Current'}));
        await f.send({type:'recent',id:f.state.session!.id});
        const before=structuredClone(object(f.state.session).formDraft);
        const valid=f.choice('search',{query:'Stale must not attach'});
        for(const change of [{project:OTHER},{sessionId:'old-session'},{epoch:Number(valid.epoch)-1},{revision:Number(object(before).revision)}]){
            await f.send({...valid,...change});
            assert.deepEqual(object(f.state.session).formDraft,before);
        }
        await f.send({...f.reset(),tool:'symbols'});
        assert.deepEqual(object(f.state.session).formDraft,before,'reset cannot target a different saved operator');
        assert.deepEqual(f.calls,[]);
    }finally{f.provider.dispose();}
});

test('New, Clear and Fork start with default form preferences; history restores its own choices',async()=>{
    for(const action of ['new','clear','fork']){
        const f=hostFixture();
        try{
            await f.send(f.choice('symbols',{name:'Session',name_match:'contains'},{scope:'src/a/',recipe:'impact'}));
            const original=structuredClone(f.state.session!);
            await f.send({type:action});
            assert.notEqual(f.state.session!.id,original.id);
            const draft=object(object(f.state.session).formDraft);
            assert.equal(draft.activeTool??'search','search');
            assert.deepEqual(object(draft.values),{});
            assert.equal(draft.scope??'','');assert.equal(draft.recipe??'','');
            if(action!=='clear'){
                await f.send({type:'recent',id:original.id});
                assert.equal(f.state.session!.id,original.id);
                assert.deepEqual(object(f.state.session).formDraft,object(original).formDraft);
            }
        }finally{f.provider.dispose();}
    }
});

test('session persistence off retains local choices without touching storage or native clients',async()=>{
    const f=hostFixture(false);
    try{
        await f.send(f.choice('references',{name:'Session',include_files:'false'},{scope:'src/local/',recipe:'impact'}));
        const draft=object(object(f.state.session).formDraft);
        assert.equal(draft.activeTool,'references');
        assert.equal(object(object(draft.values).references).include_files,'false');
        assert.equal(draft.scope,'src/local/');assert.equal(draft.recipe,'impact');
        await f.send(f.reset());
        assert.deepEqual(object(object(object(object(f.state.session).formDraft).values).references),{});
        assert.equal(f.saved.length,0);assert.equal(f.records.size,0);assert.deepEqual(f.calls,[]);
    }finally{f.provider.dispose();}
});

test('same-session history loading retires old UI edits before asynchronous storage completes',async()=>{
    const f=hostFixture();
    try{
        await f.send(f.choice('search',{query:'Saved before navigation'},{scope:'src/a/',recipe:'component'}));
        const retained=structuredClone(f.state.session!),stale=f.choice('search',{query:'Old UI must not attach'});
        let release!:(session:WorkbenchSession)=>void;
        const held=new Promise<WorkbenchSession>(resolve=>{release=resolve;});
        f.store.load=async()=>held;
        const recent=parseWorkbenchMessage({type:'recent',id:retained.id});assert.ok(recent);
        const loading=f.state.message(recent);
        await new Promise<void>(resolve=>setImmediate(resolve));
        assert.ok(Number(f.state.state().sessionEpoch)>Number(stale.epoch));
        await f.send(stale);
        assert.deepEqual(object(f.state.session).formDraft,object(retained).formDraft);
        release(retained);await loading;
        assert.equal(f.state.session!.id,retained.id);
        assert.deepEqual(object(f.state.session).formDraft,object(retained).formDraft);
        assert.deepEqual(f.calls,[]);
    }finally{f.provider.dispose();}
});

test('webview rejects stale scalar acknowledgements and full snapshots without leaking preferences into new sessions',()=>{
    const session=newSession(PROJECT);
    session.formDraft={activeTool:'symbols',values:{symbols:{name:'SessionA'}},scope:'src/a/',recipe:'component',revision:5};
    const state:WorkbenchState={projects:[],project:PROJECT,session,recent:[],cards:{},busy:false,save:'local edit',persistence:true,sessionEpoch:8};
    const ack={type:'formDraft' as const,sessionId:session.id,project:PROJECT,epoch:8,formDraft:{...session.formDraft,revision:4,values:{symbols:{name:'stale'}}},save:'stale'};
    for(const change of [{},{sessionId:'different-session'},{project:OTHER},{epoch:7},{epoch:9}]){
        assert.equal(applyWorkbenchUpdate(state,{...ack,...change}),state);
    }
    const oldSnapshot={...state,sessionEpoch:7};
    assert.equal(applyWorkbenchUpdate(state,{type:'state',state:oldSnapshot}),state);
    const delayedSave={...state,status:{marker:'new status'},session:{...session,formDraft:{...defaultWorkbenchFormDraft(),revision:2}}};
    const merged=applyWorkbenchUpdate(state,{type:'state',state:delayedSave});
    assert.deepEqual(merged.session!.formDraft,session.formDraft,'result/status snapshot cannot roll back current form revision');
    assert.equal(merged.status?.marker,'new status','independent display observations may still update');
    const fresh=newSession(PROJECT);delete fresh.formDraft;
    const replacement={...state,sessionEpoch:9,session:fresh};
    const next=applyWorkbenchUpdate(state,{type:'state',state:replacement});
    assert.equal(next,replacement);
    assert.deepEqual(normalizeWorkbenchFormDraft(next.session!.formDraft),defaultWorkbenchFormDraft());
    const first=defaultWorkbenchFormDraft(),second=defaultWorkbenchFormDraft();
    first.values.search={query:'must stay local'};
    assert.deepEqual(second.values,{},'new/legacy defaults cannot share a mutable values map');
});
