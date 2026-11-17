# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f08c2-draft/implementation/source/packages/vscode/src/serviceConsoleController.ts')
s=p.read_text(encoding='utf-8');start=s.index('        let eventFailure:string|undefined;');end=s.index('        this.pending();\n        try{await this.page(false);}',start)
s=s[:start]+'''        let eventFailure:string|undefined;
        try{await this.events();}catch(error){
            eventFailure=code(error);this.state.message=eventFailure;
            void this.host.diagnosticFailure?.('events_namespace',error,this.bootstrap).catch(()=>{});
            const reset=error instanceof ServiceError&&['event_cursor_expired','event_scope_changed'].includes(error.code);
            let confirmed=false;
            if(reset){
                if(error.code==='event_scope_changed'){this.observationToken=randomUUID();this.clearRegistryObservation(true);}
                this.eventCursor=null;this.state.historyOmitted=true;
                try{await this.events(true);confirmed=true;}catch(resetError){
                    void this.host.diagnosticFailure?.('event_cursor_reset',resetError,this.bootstrap).catch(()=>{});
                }
            }
            if(!confirmed){
                // Preserve old page availability when optional Events cannot
                // confirm namespace. This is the original native epoch operation,
                // not a replay of any keyed/uncertain mutation.
                try{
                    const epoch=await this.rpc('request_epoch','request_epoch');
                    this.restoreJournal(identifier(epoch.namespace,16));
                    if(reset)this.state.historyOmitted=true;
                }catch(confirmError){
                    void this.host.diagnosticFailure?.('event_namespace_fallback',confirmError,this.bootstrap).catch(()=>{});
                    // The shared deadline still owns transport cleanup/reconnect;
                    // the original Events diagnostic has already been retained.
                    if(code(confirmError)==='status_poll_timed_out')throw confirmError;
                    throw error;
                }
            }
        }
'''+s[end:]
p.write_text(s,encoding='utf-8',newline='\n')
p=Path('artifacts/optimizations/f08c2-draft/implementation/source/packages/vscode/src/test/serviceConsole.test.ts');s=p.read_text(encoding='utf-8')
s=s.replace("        if(broken&&name==='events')return {status:'failure',code:++eventReads===1?'event_scope_changed':'registry_unavailable'};", "        if(broken&&name==='events')return {status:'failure',code:++eventReads===1?'event_scope_changed':'registry_unavailable'};\n        if(broken&&name==='request_epoch')return {status:'failure',code:'registry_unavailable'};")
s=s.replace("        assert.ok(!seen.some(name=>['console','workers','request_epoch','enqueue'].includes(name)));", "        assert.ok(!seen.some(name=>['console','workers','enqueue'].includes(name)));\n        assert.equal(seen.filter(name=>name==='request_epoch').length,1);\n        assert.ok(failures.includes('event_namespace_fallback:registry_unavailable'));")
s+='''
test('optional Events failure falls back to original namespace confirmation and keeps healthy project/worker observations',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),seen:string[]=[],failures:string[]=[];
    let unavailable=false;
    f.host.diagnosticFailure=async(operation,error)=>{failures.push(`${operation}:${(error as ServiceError).code}`);};
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];seen.push(name);
        if(unavailable&&name==='events')return {status:'failure',code:'unsupported_service_method'};
        const response=await base.transport.request(method);
        if(unavailable&&name==='console')return {...response,page:{...(response as any).page,revision:2,observed_unix_ms:222}};
        return response;
    }});
    try{
        await f.controller.refresh();seen.length=0;unavailable=true;await f.controller.refresh();
        assert.equal(f.controller.state.connected,true);assert.equal(f.controller.state.health.state,'live');
        assert.equal(f.controller.state.page?.revision,2);assert.equal(f.controller.state.page?.observed_unix_ms,222);
        assert.ok(f.controller.state.workers);assert.equal(f.controller.state.message,'unsupported_service_method');
        assert.deepEqual(seen.filter(name=>['events','request_epoch','console','workers'].includes(name)),['events','request_epoch','console','workers']);
        assert.ok(failures.includes('events_namespace:unsupported_service_method'));
        assert.equal(seen.some(name=>['enqueue','configure_project','retry','plan_execute'].includes(name)),false);
    }finally{f.controller.dispose();}
});
''';p.write_text(s,encoding='utf-8',newline='\n')
