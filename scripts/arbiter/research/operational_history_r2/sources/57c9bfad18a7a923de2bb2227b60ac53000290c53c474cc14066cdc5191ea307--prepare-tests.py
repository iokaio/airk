# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f17-draft/display-updates/source/packages/vscode/src/test')
f=p/'workbenchDisplay.test.ts';f.write_text('''// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import {applyWorkbenchUpdate, WorkbenchState, WorkbenchTurn} from '../workbenchContracts';
import {CurrentTurnProjections, DISPLAY_PROJECTION_TURNS, continuations, evidenceCards, renderExport} from '../workbenchRender';
import {newSession} from '../workbenchSessions';
function turn(id:string):WorkbenchTurn{return {id,at:1,input:'full input 雪',format:'structured',envelope:{status:'partial',publication:'original-head',calls:1,
    plan:{budgets:{calls:6},steps:[{id:'s',tool:'search'}]},steps:[{status:'complete',tool:'search',result:{cursor:'original cursor'}}],
    evidence:[{step:'s',text:'complete retained body',citation:{path:'docs/a.md',start_line:2,end_line:3,content_hash:'ab'.repeat(32),location:{Lines:{start:2,end:3}}}}]}};}
test('display projection reuses exact results, invalidates envelope replacement and evicts beyond bounded ownership',()=>{
    const cache=new CurrentTurnProjections(),first=turn('__proto__'),session=newSession('aa'.repeat(32));session.turns=[first];
    const before=renderExport(session,'json'),a=cache.project(session.turns),b=cache.project(session.turns);
    assert.deepEqual(a.cards[first.id],evidenceCards(first.envelope!));assert.deepEqual(a.continuations[first.id],continuations(first.envelope!));
    assert.equal(a.cards[first.id],b.cards[first.id]);assert.equal(a.continuations[first.id],b.continuations[first.id]);
    assert.ok(Object.hasOwn(a.cards,'__proto__'),'preserve Object.fromEntries key behavior');
    assert.equal(renderExport(session,'json'),before,'projection cannot trim or rewrite authoritative exports');
    first.envelope={...first.envelope,evidence:[],steps:[]};const changed=cache.project(session.turns);
    assert.notEqual(changed.cards[first.id],a.cards[first.id]);assert.deepEqual(changed.cards[first.id],[]);assert.deepEqual(changed.continuations[first.id],[]);
    const many=Array.from({length:DISPLAY_PROJECTION_TURNS+1},(_,i)=>turn(String(i)));
    const full=cache.project(many),again=cache.project(many);
    assert.equal(Object.keys(full.cards).length,many.length,'cache bound is not output truncation');
    assert.notEqual(full.cards['0'],again.cards['0'],'oldest uncached projection is recomputed rather than retained');
    assert.equal(full.cards['1'],again.cards['1']);
    cache.clear();assert.notEqual(cache.project(many).cards['1'],again.cards['1']);
});
test('actual webview update consumer retains exact bodies/cursors and rejects cross-session scalar completion',()=>{
    const session=newSession('aa'.repeat(32));session.turns=[turn('retained')];
    const projected=new CurrentTurnProjections().project(session.turns);
    const state:WorkbenchState={projects:[],project:session.project,session,recent:[],...projected,busy:false,save:'Saving…',persistence:true};
    const before=renderExport(session,'json');
    const ready=applyWorkbenchUpdate(state,{type:'status',sessionId:session.id,project:session.project,status:{publication:{token:'new-head'}}});
    assert.equal(ready.session,session);assert.equal(ready.cards,state.cards);assert.equal(ready.continuations,state.continuations);
    const saved=applyWorkbenchUpdate(ready,{type:'saved',sessionId:session.id,project:session.project,revision:7,recent:[],save:'Auto-saved'});
    assert.equal(saved.session!.turns,session.turns);assert.equal(saved.cards,state.cards);assert.equal(saved.session!.revision,7);
    assert.equal(renderExport(saved.session!,'json'),before);assert.equal(saved.continuations?.retained[0].id,'page-0');
    assert.equal(applyWorkbenchUpdate(saved,{type:'status',sessionId:'old',project:session.project,status:{wrong:true}}),saved);
    assert.equal(applyWorkbenchUpdate(saved,{type:'saved',sessionId:session.id,project:'foreign',revision:100,recent:[],save:'wrong'}),saved);
    const replacement={...state,session:newSession(session.project)};
    assert.equal(applyWorkbenchUpdate(saved,{type:'state',state:replacement}),replacement);
});
''',encoding='utf-8',newline='\n')
f=p/'workbenchClientLifetime.test.ts';s=f.read_text(encoding='utf-8').replace("import type { WorkbenchSession, Value }", "import { applyWorkbenchUpdate, type WorkbenchState, type WorkbenchUpdate, type WorkbenchSession, type Value }")
s+='''
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
        const statusMessages=f.messages as WorkbenchUpdate[];
        assert.equal(statusMessages.length,1);assert.equal(statusMessages[0].type,'status');
        assert.ok(!JSON.stringify(statusMessages).includes('retained-0-'));
        let rendered=statusMessages.reduce(applyWorkbenchUpdate,baseline);
        assert.deepEqual(rendered,structuredClone(hostState()));
        const fullBytes=Buffer.byteLength(JSON.stringify({type:'state',state:hostState()})),statusBytes=Buffer.byteLength(JSON.stringify(statusMessages[0]));
        assert.ok(statusBytes<fullBytes/20,'count actual encoded posted bytes; do not infer latency');
        f.messages.length=0;
        f.state.save(session);await f.state.saveQueue;
        const saveMessages=f.messages as WorkbenchUpdate[];
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
''';f.write_text(s,encoding='utf-8',newline='\n')
