# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f08c2-draft/implementation/source/packages/vscode/src/test/serviceConsole.test.ts')
s=p.read_text(encoding='utf-8')
s=s.replace("import { RecordValue, ServiceClient, ServiceError } from '../serviceClient';", "import { RecordValue, ServiceClient, ServiceError } from '../serviceClient';\nimport { IntentJournal } from '../serviceIntents';")
s=s.replace("['resources','embedding_status','source_status','logs','request_epoch','console','workers','events']", "['resources','embedding_status','source_status','logs','console','workers','events']")
s+='''
test('periodic observation uses Events namespace before page without RequestEpoch; actual keyed action still obtains one',async()=>{
    const f=fixture();
    try{
        await f.controller.refresh();
        const names=()=>f.sent.map(method=>typeof method==='string'?method:Object.keys(method as object)[0]);
        assert.equal(names().filter(name=>name==='request_epoch').length,0);
        assert.equal(names().filter(name=>name==='events').length,1);
        assert.ok(names().indexOf('events')<names().indexOf('console'));
        await f.action('index',{project});
        assert.equal(names().filter(name=>name==='request_epoch').length,1,'action key() keeps native admission/epoch producer');
        assert.equal(names().filter(name=>name==='enqueue').length,1);
    }finally{f.controller.dispose();}
});
test('same service replacement namespace resets one old cursor, keeps first failure and never joins old history or replays intent',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),replacement='ab'.repeat(16),newProject='cd'.repeat(32);
    let changed=false;const seen:unknown[]=[],diagnostics:string[]=[];
    const journal=new IntentJournal(f.host.storage,'/control',namespace);
    await journal.prepare('retained uncertain request',{enqueue:{project,caller:'89'.repeat(16),key:{namespace,epoch:7,nonce:'90'.repeat(16)},kind:'index'}});
    f.host.diagnosticFailure=async operation=>{diagnostics.push(operation);};
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        seen.push(structuredClone(method));const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(name==='events'){
            const cursor=(method as any).events.cursor,current=changed?replacement:namespace;
            if(cursor&&cursor.namespace!==current)return {status:'failure',code:'event_scope_changed'};
            return {status:'events',page:{namespace:current,oldest_sequence:1,snapshot_through:1,retained_prefix_lost:false,
                events:[{sequence:1,observed_unix_ms:changed?222:111,project:changed?newProject:project,kind:'registered'}],next:null,tail:{namespace:current,project:null,after:1,through:null}}};
        }
        if(name==='request_receipt')return {status:'request_receipt',job:null};
        if(changed&&name==='console')return {status:'console',page:{revision:2,cached:true,observed_unix_ms:222,after:null,projects:[{project:{registration:{binding:newProject,root:'/new-source',data_root:'/new-data',collection:'fixture'},paused:false,fencing_generation:1}}]}};
        if(changed&&name==='workers')return {status:'workers',observations:[{project:newProject,identity:null,restarting:false,automatic_start_blocked:false}],age_ms:0};
        return base.transport.request(method);
    }});
    try{
        await f.controller.refresh();assert.equal(f.controller.state.events[0].observed_unix_ms,111);
        const saved=JSON.stringify([...f.values.entries()]);f.controller.state.selected.add(project);
        f.controller.state.plan={value:{id:'ef'.repeat(32)},items:[],after:null};
        seen.length=0;changed=true;await f.controller.refresh();
        const events=seen.filter(value=>value&&typeof value==='object'&&'events' in value) as any[];
        assert.equal(events.length,2);assert.equal(events[0].events.cursor.namespace,namespace);assert.equal(events[1].events.cursor,null);
        assert.equal(f.controller.state.message,'event_scope_changed');assert.equal(f.controller.state.historyOmitted,true);
        assert.equal(f.controller.state.events.length,1);assert.equal(f.controller.state.events[0].project,newProject);assert.equal(f.controller.state.events[0].observed_unix_ms,222);
        assert.equal(f.controller.state.selected.size,0);assert.equal(f.controller.state.plan,undefined);
        assert.equal((f.controller.state.page!.projects[0].project as any).registration.binding,newProject);
        assert.equal(JSON.stringify([...f.values.entries()]),saved,'old uncertain journal stays retained, never rewritten or submitted');
        assert.equal(seen.some(value=>value==='request_epoch'||value&&typeof value==='object'&&('enqueue' in value||'request_receipt' in value)),false);
        assert.deepEqual(f.controller.state.pending,[]);
    }finally{f.controller.dispose();}
});
test('fresh Events reset failure preserves first refusal and cannot adopt a page or retry a mutation',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),seen:string[]=[],failures:string[]=[];
    let broken=false,eventReads=0;
    f.host.diagnosticFailure=async(operation,error)=>{failures.push(`${operation}:${(error as ServiceError).code}`);};
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];seen.push(name);
        if(broken&&name==='events')return {status:'failure',code:++eventReads===1?'event_scope_changed':'registry_unavailable'};
        return base.transport.request(method);
    }});
    try{
        await f.controller.refresh();seen.length=0;broken=true;await f.controller.refresh();
        assert.equal(eventReads,2);assert.equal(f.controller.state.message,'event_scope_changed');
        assert.ok(failures.includes('event_cursor_reset:registry_unavailable'));
        assert.equal(f.controller.state.page,undefined);assert.deepEqual(f.controller.state.events,[]);
        assert.equal(f.controller.state.historyOmitted,true);
        assert.ok(!seen.some(name=>['console','workers','request_epoch','enqueue'].includes(name)));
    }finally{f.controller.dispose();}
});
test('trust loss while Events namespace is held prevents namespace/page adoption',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any);let hold=false,release!:()=>void,entered!:()=>void;
    const gate=new Promise<void>(resolve=>{release=resolve;}),waiting=new Promise<void>(resolve=>{entered=resolve;});
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(hold&&name==='events'){entered();await gate;}
        return base.transport.request(method);
    }});
    try{
        await f.controller.refresh();const identity=f.controller.observationIdentity;
        hold=true;const pending=f.controller.refresh();await waiting;f.setTrusted(false);release();await pending;
        assert.equal(f.controller.observationIdentity,identity);assert.equal(f.controller.state.trusted,false);
        assert.equal(f.controller.state.message,'runtime_client_no_longer_authorized');
    }finally{release();f.controller.dispose();}
});
''';p.write_text(s,encoding='utf-8',newline='\n')
