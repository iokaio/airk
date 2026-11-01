// SPDX-License-Identifier: Apache-2.0
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
