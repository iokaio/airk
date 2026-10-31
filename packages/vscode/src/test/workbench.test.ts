// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { newSession, SessionStore, trimSession } from '../workbenchSessions';
import { parseWorkbenchMessage, WORKBENCH_HELP_ROUTE } from '../workbenchContracts';
import { parseRoute } from '../explorerRoutes';
import { evidenceCards, renderExport } from '../workbenchRender';
import { EvidenceClient } from '../explorerEvidence';
import { strategyFromPlan } from '../workbenchStrategies';

const project='ab'.repeat(32);
test('Strategy saving preserves compatible inputs and refuses to weaken native ambiguity checks',()=>{
    const document=strategyFromPlan({path_prefix:'src/',steps:[{id:'occurrences',tool:'references',args:{name:'QueueClient',path_prefix:'src/'}}]},'Queue evidence','Collect evidence');
    assert.equal(document.strategy.steps[1].op,'airk.references');assert.equal(document.strategy.steps[1].args.name,'QueueClient');assert.equal(document.pathPrefix,'src/');
    assert.throws(()=>strategyFromPlan({steps:[{id:'s',tool:'symbols',args:{name:'QueueClient'}},{id:'o',tool:'outline',args:{},bind:[{from:'s',arg:'path',select:'unique_definition_path'}]}]},'Definition','Inspect'),/ambiguity checks/);
    assert.throws(()=>strategyFromPlan({steps:[{id:'search',tool:'search',args:{query:'retry',mode:'vector'}}]},'Vector','Find'),/does not support/);
});
test('Workbench message boundary accepts bounded inert plans and refuses unrelated actions',()=>{
    assert.equal(parseRoute(WORKBENCH_HELP_ROUTE)?.focus,'engine-workbench');
    assert.ok(parseWorkbenchMessage({type:'help'}));
    assert.ok(parseWorkbenchMessage({type:'run',input:'/search retry',prepare:false}));
    assert.equal(parseWorkbenchMessage({type:'run',input:'x',prepare:false,command:'shell'}),undefined);
    assert.equal(parseWorkbenchMessage({type:'open',turn:'t',card:'r',path:'../../outside'}),undefined);
    assert.equal(parseWorkbenchMessage({type:'run',input:'x'.repeat(4097),prepare:false}),undefined);
});
test('session retention, optimistic revisions and result privacy preserve unrelated future files',async()=>{
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-workbench-'));
    try{
        const store=new SessionStore(root);
        for(let i=0;i<12;i++){const session=newSession(project);session.updatedAt=i;session.turns=[{id:'t',at:0,input:'query',format:'structured',envelope:{source:'sensitive'}}];await store.save(session);}
        assert.equal((await store.list(project)).length,10);
        const session=(await store.list(project))[0],old=structuredClone(session);session.title='changed';await store.save(session);
        await assert.rejects(store.save(old),/another window/);
        await store.clearResults(project);assert.ok((await store.list(project)).every(s=>s.turns.every(t=>!t.envelope&&t.trimmed)));
        const future=newSession(project);const futurePath=path.join(root,project,`${future.id}.json`);await fs.writeFile(futurePath,JSON.stringify({...future,schema:999}));
        const next=newSession(project);next.turns=[{id:'t',at:0,input:'next',format:'structured'}];await store.save(next);assert.ok((await fs.readFile(futurePath,'utf8')).includes('999'));
        await store.remember(project,'  Inspect policy ',{catalog:'airk.evidence-plans/1',path_prefix:'docs/',publication:'old',digest:'old',steps:[{id:'read',tool:'read_file',args:{path:'docs/policy.md',start_line:1,end_line:2,known_hash:'old',revision:'old'}}]});
        const shortcut=await store.findShortcut(project,'Inspect policy','docs/');assert.ok(shortcut);assert.equal(shortcut.publication,undefined);assert.equal(shortcut.digest,undefined);assert.equal((shortcut.steps as any[])[0].args.known_hash,undefined);
        assert.equal(await store.findShortcut(project,'inspect policy','docs/'),undefined,'case-sensitive query identity is preserved');
        assert.equal(await store.findShortcut(project,'inspect policy','src/'),undefined);
        await assert.rejects(store.load(project,'../../outside'),/Invalid session/);
        await store.remove(project);assert.equal((await store.list(project)).length,0);
    }finally{await fs.rm(root,{recursive:true,force:true});}
});
test('result privacy controls reach saved excerpts in every project, not only the selected one',async()=>{
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-workbench-'));
    try{
        const store=new SessionStore(root),other='cd'.repeat(32);
        for(const id of [project,other]){const session=newSession(id);session.turns=[{id:'t',at:0,input:'query',format:'structured',envelope:{source:'fictional excerpt'}}];await store.save(session);}
        await fs.mkdir(path.join(root,'not-a-project'));
        assert.deepEqual((await store.projects()).sort(),[project,other].sort());
        for(const id of await store.projects()){await store.clearResults(id);}
        for(const id of [project,other]){assert.ok((await store.list(id)).every(s=>s.turns.every(t=>!t.envelope&&t.trimmed)));}
        for(const id of await store.projects()){await store.remove(id);}
        for(const id of [project,other]){assert.equal((await store.list(id)).length,0);}
        assert.deepEqual(await new SessionStore(path.join(root,'missing')).projects(),[]);
    }finally{await fs.rm(root,{recursive:true,force:true});}
});
test('session limits count UTF-8 and trim envelopes before old inputs',()=>{
    const session=newSession(project);session.turns=Array.from({length:202},(_,i)=>({id:String(i),at:i,input:`request ${i}`,format:'structured',envelope:{text:'界'.repeat(50000)}}));
    trimSession(session);assert.equal(session.turns.length,200);assert.equal(session.omitted,2);assert.ok(Buffer.byteLength(JSON.stringify(session))<=4*1024*1024);assert.ok(session.turns.some(t=>t.trimmed));
});
test('exports keep citations, source locations and warnings, and use valid fences and JSON',()=>{
    const session=newSession(project);const citation={path:'docs/rules.md',start_line:2,end_line:4,content_hash:'12'.repeat(32),location:{Block:{number:2}}};
    const turn={id:'turn',at:0,input:'requirements',format:'structured' as const,envelope:{publication:'token',status:'partial',truncated:true,warnings:['not resolved'],completeness:['upstream truncation'],evidence:[{citation,text:'```\nunsafe <script>\n```',step:'s'}]}};
    session.turns.push(turn);const cards=evidenceCards(turn.envelope);assert.equal(cards.length,1);
    const json=JSON.parse(renderExport(session,'json'));assert.equal(json.turns[0].envelope.truncated,true);
    const md=renderExport(session,'markdown');assert.match(md,/````/);assert.match(md,/upstream truncation/);assert.match(md,/Block/);assert.match(md,/Verify relevant live files/);
    const selected=JSON.parse(renderExport(session,'json',turn.id,[]));assert.equal(selected.turns[0].envelope.evidence.length,0);
    assert.doesNotMatch(renderExport(session,'markdown',turn.id,[]),/unsafe/,'empty selection must not fall back to the full result');
    delete (citation as {location?:unknown}).location;assert.doesNotThrow(()=>renderExport(session,'markdown'));
});
test('Workbench has separate identity and allowlist; execute requests are never replayed automatically',async()=>{
    let calls=0,label='';const spec={command:path.resolve('airk-launcher'),root:path.resolve('source'),dataDir:path.resolve('state'),collection:'fixture',name:'fixture',service:{installDir:path.resolve('runtime'),project}};
    const open=async(options:any)=>{label=options.clientName;return {session:{closed:false,close(){},request:async()=>{calls++;return {isError:true,content:[{type:'text',text:JSON.stringify({status:'retry',retryable:true,action:'repeat_same_request',code:'publication_busy',retry_after_ms:1})}]};}} as any};};
    const explorer=new EvidenceClient(spec,'test',{open});await assert.rejects(explorer.call('airk_parse',{}));assert.equal(calls,0);
    const workbench=new EvidenceClient(spec,'test',{open,workbench:true,clientName:'airk-workbench'});await assert.rejects(workbench.call('airk_parse',{}));assert.equal(calls,1);assert.equal(label,'airk-workbench');workbench.dispose();explorer.dispose();
});
