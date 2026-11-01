// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { newSession, SessionStore, trimSession } from '../workbenchSessions';
import { defaultWorkbenchFormDraft, normalizeWorkbenchFormDraft, parseWorkbenchMessage, WORKBENCH_HELP_ROUTE } from '../workbenchContracts';
import { parseRoute } from '../explorerRoutes';
import { evidenceCards, renderExport } from '../workbenchRender';
import { EvidenceClient } from '../explorerEvidence';
import { assertPortablePlanInputs, planForWorkbenchRerun, strategyFromPlan } from '../workbenchStrategies';

const project='ab'.repeat(32);
test('per-session form choices survive real storage before any query and old sessions load defaults',async()=>{
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-workbench-form-storage-'));
    try{
        const store=new SessionStore(root),session=newSession(project);
        assert.deepEqual(session.formDraft,defaultWorkbenchFormDraft());
        session.formDraft={activeTool:'references',revision:4,scope:'src/',recipe:'component',values:{
            search:{query:'unfinished phrase',limit:'-'},
            references:{name:'',language:'ru',include_count:'false'},
            symbols:{name_match:'suffix',compact:'true'},
        }};
        await store.save(session,false);
        const restarted=new SessionStore(root),loaded=await restarted.load(project,session.id);
        assert.deepEqual(loaded.formDraft,session.formDraft);
        assert.deepEqual(loaded.turns,[],'draft persistence must not manufacture a query or result');
        assert.equal((await restarted.list(project))[0].id,session.id);
        const fresh=newSession(project);
        assert.deepEqual(fresh.formDraft,defaultWorkbenchFormDraft());
        fresh.formDraft!.values.search={query:'separate session'};
        assert.equal(loaded.formDraft!.values.search.query,'unfinished phrase','sessions do not share mutable draft maps');
        const legacy=newSession(project);delete legacy.formDraft;
        legacy.turns=[{id:'legacy',at:1,input:'Old query',format:'structured'}];
        const legacyFile=path.join(root,project,`${legacy.id}.json`),legacyBytes=JSON.stringify(legacy);
        await fs.writeFile(legacyFile,legacyBytes);
        assert.deepEqual((await restarted.load(project,legacy.id)).formDraft,defaultWorkbenchFormDraft());
        assert.equal(await fs.readFile(legacyFile,'utf8'),legacyBytes,'reading old defaults is not a migration write');
        await assert.rejects(restarted.load('cd'.repeat(32),session.id),/ENOENT/,'another project cannot load these choices');
    }finally{await fs.rm(root,{recursive:true,force:true});}
});

test('form storage and messages validate bounds while retaining incomplete inert inputs',()=>{
    const draft={...defaultWorkbenchFormDraft(),revision:1,values:{search:{query:'',limit:'-',max_bytes:'not yet a number'}}};
    assert.deepEqual(normalizeWorkbenchFormDraft(draft),draft);
    const fence={sessionId:'draft-session',project,epoch:2,revision:1,tool:'search'};
    assert.ok(parseWorkbenchMessage({type:'formDraft',...fence,values:draft.values.search}));
    assert.ok(parseWorkbenchMessage({type:'formDraft',...fence,scope:'../still typing',recipe:'definition'}));
    assert.ok(parseWorkbenchMessage({type:'resetForm',...fence}));
    for(const invalid of [
        {...draft,activeTool:'runtime-purge'},
        {...draft,values:{search:{query:3}}},
        {...draft,values:{search:[]}},
        {...draft,values:{search:new Date()}},
        {...draft,values:{search:{constructor:'x'}}},
        {...draft,values:{search:{query:'x'.repeat(32769)}}},
        {...draft,values:{search:{a:'x'.repeat(32768),b:'x'.repeat(32768),c:'x'.repeat(32768),d:'x'.repeat(32768)}}},
        {...draft,recipe:'execute-shell'},
    ])assert.equal(normalizeWorkbenchFormDraft(invalid),undefined);
    assert.equal(parseWorkbenchMessage({type:'formDraft',...fence,values:{query:4}}),undefined);
    assert.equal(parseWorkbenchMessage({type:'formDraft',...fence,command:'shell'}),undefined);
    assert.equal(parseWorkbenchMessage({type:'resetForm',...fence,values:{}}),undefined);
});
test('Strategy saving preserves compatible inputs and refuses to weaken native ambiguity checks',()=>{
    const document=strategyFromPlan({path_prefix:'src/',steps:[{id:'occurrences',tool:'references',args:{name:'QueueClient',path_prefix:'src/'}}]},'Queue evidence','Collect evidence');
    assert.equal(document.strategy.steps[1].op,'airk.references');assert.equal(document.strategy.steps[1].args.name,'QueueClient');assert.equal(document.pathPrefix,'src/');
    assert.throws(()=>strategyFromPlan({steps:[{id:'s',tool:'symbols',args:{name:'QueueClient'}},{id:'o',tool:'outline',args:{},bind:[{from:'s',arg:'path',select:'unique_definition_path'}]}]},'Definition','Inspect'),/ambiguity checks/);
    assert.throws(()=>strategyFromPlan({steps:[{id:'search',tool:'search',args:{query:'retry',mode:'vector'}}]},'Vector','Find'),/does not support/);
});

test('revision-bound literal symbol IDs cannot lose their publication through reuse or Strategy conversion',()=>{
    for(const id of ['syntax:src/session.rs:10:30:struct_item','lsp:fixture:src/session.rs:10:30','ctags:fixture:src/session.rs:2:Session','scip:fixture:local 1']){
        for(const field of ['name','symbol']){
            const plan={publication:'original-publication',digest:'original-digest',steps:[{
                id:'inspect',tool:field==='name'?'symbols':'relationships',args:{[field]:id,path_prefix:'src/'},
            }]};
            const before=structuredClone(plan);
            assert.throws(()=>planForWorkbenchRerun(plan),/revision-bound symbol ID/);
            assert.throws(()=>strategyFromPlan(plan,'Inspect Session','Collect evidence'),/revision-bound symbol ID/);
            assert.deepEqual(plan,before,'rejection must retain the original exact export inputs/publication');
        }
    }
    for(const name of ['Session','syntax::Session','lsp::Session']){
        const plan={publication:'old',digest:'old',collection:'old',steps:[{id:'named',tool:'symbols',args:{name}}]};
        const fresh=planForWorkbenchRerun(plan);
        assert.equal(fresh.publication,undefined);
        assert.equal((fresh.steps as {args:{name:string}}[])[0].args.name,name);
        assert.equal(strategyFromPlan(plan,'Named definition','Collect evidence').strategy.steps[1].args.name,name);
        assert.equal(plan.publication,'old');
    }
    const key='airk-c1:durable-continuity-fixture';
    const durable={publication:'old',steps:[{id:'durable',tool:'code_facts',args:{symbol_key:key}}]};
    assert.doesNotThrow(()=>assertPortablePlanInputs(durable));
    assert.equal((planForWorkbenchRerun(durable).steps as {args:{symbol_key:string}}[])[0].args.symbol_key,key);
    const bound={publication:'old',steps:[
        {id:'definitions',tool:'symbols',args:{name:'Session'}},
        {id:'callers',tool:'relationships',args:{direction:'incoming'},bind:[{arg:'symbol',from:'definitions',select:'selected_symbol_ids'}]},
    ]};
    assert.doesNotThrow(()=>assertPortablePlanInputs(bound));
    assert.equal(strategyFromPlan(bound,'Fresh callers','Bind newly collected IDs').strategy.steps[2].input,'definitions');
});

test('Remember rejects unsafe IDs before writing and legacy shortcut loading cannot bypass that guard',async()=>{
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-workbench-portability-'));
    try{
        const store=new SessionStore(root);
        const unsafe={publication:'retained-publication',steps:[{id:'inspect',tool:'symbols',args:{name:'syntax:src/session.rs:10:30:struct_item'}}]};
        const before=structuredClone(unsafe);
        await assert.rejects(store.remember(project,'Inspect Session',unsafe),/revision-bound symbol ID/);
        assert.deepEqual(unsafe,before);
        const directory=path.join(root,project),file=path.join(directory,'shortcuts.json');
        await fs.mkdir(directory);
        const legacy=JSON.stringify({schema:1,entries:[{input:'Inspect Session',scope:'src/',plan:unsafe}]});
        await fs.writeFile(file,legacy);
        await assert.rejects(store.findShortcut(project,'Inspect Session','src/'),/revision-bound symbol ID/);
        assert.equal(await fs.readFile(file,'utf8'),legacy,'unsafe legacy inputs remain intact for explicit review/export');
        const portable={publication:'old',path_prefix:'src/',steps:[
            {id:'definitions',tool:'symbols',args:{name:'Session'}},
            {id:'callers',tool:'relationships',args:{direction:'incoming'},bind:[{arg:'symbol',from:'definitions',select:'selected_symbol_ids'}]},
        ]};
        await store.remember(project,'Fresh callers',portable);
        const saved=await store.findShortcut(project,'Fresh callers','src/');
        assert.ok(saved);
        assert.equal(saved.publication,undefined);
        assert.deepEqual((saved.steps as unknown[])[1],portable.steps[1]);
        await assert.rejects(store.remember(project,'Written target',{steps:[{id:'call',tool:'relationships',args:{symbol:'Session::new'}}]}),/direct symbol ID cannot be remembered/,'existing Remember restriction stays intact');
    }finally{await fs.rm(root,{recursive:true,force:true});}
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

// Count actual filesystem reads across the owned save and retention scan.
test('save returns bounded recent summaries from its existing prune scan',async()=>{
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-workbench-save-'));
    class CountingStore extends SessionStore {
        loads=0;
        override async load(project:string,id:string){this.loads++;return super.load(project,id);}
    }
    try{
        const store=new CountingStore(root),directory=path.join(root,project);
        await fs.mkdir(directory,{recursive:true});
        const sessions=Array.from({length:10},(_,index)=>{
            const session=newSession(project);session.updatedAt=index;
            session.turns=Array.from({length:200},(_,turn)=>({id:String(turn),at:turn,input:'fixture query',format:'structured' as const,envelope:{text:'fictional'.repeat(2170)}}));
            return session;
        });
        let bytes=0;
        for(const session of sessions){const text=JSON.stringify(session);bytes+=Buffer.byteLength(text);assert.ok(Buffer.byteLength(text)<4*1024*1024);await fs.writeFile(path.join(directory,`${session.id}.json`),text);}
        assert.ok(bytes>36*1024*1024,'fixture keeps ten near-limit envelopes');
        const saved=sessions[9];saved.title='Updated fixture';saved.updatedAt=100;
        const recent=await store.save(saved);
        assert.equal(store.loads,11,'one optimistic revision read and one complete ten-file scan');
        assert.equal(saved.revision,1);
        assert.equal(recent?.length,10);
        assert.deepEqual(recent?.[0],{id:saved.id,title:'Updated fixture',updatedAt:100,turns:200});
        assert.ok(Buffer.byteLength(JSON.stringify(recent))<4096,'summary contains no retained excerpts');
        store.loads=0;
        assert.equal(await store.save(newSession(project)),undefined);
        assert.equal(store.loads,0,'an empty session remains a no-op');
        assert.deepEqual(recent,(await store.list(project)).map(value=>({id:value.id,title:value.title,updatedAt:value.updatedAt,turns:value.turns.length})));
    }finally{await fs.rm(root,{recursive:true,force:true});}
});
