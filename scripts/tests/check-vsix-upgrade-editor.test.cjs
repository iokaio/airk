// SPDX-License-Identifier: Apache-2.0
const test=require('node:test'),assert=require('node:assert/strict');
const {toolBody,assertEvidence,completeStop,inside,OLD_SHA,OLD_MAIN_SHA,failedActivation,qualifyOldLauncherCopy,gatePrivateProfile,waitOwnedExit,boundedJsonProcess,boundedSafeRead}=require('../check-vsix-upgrade-editor.cjs');
test('old recovery must reproduce authenticated copied-main invalid Info.plist defect',()=>{
    const path=require('node:path'),install=path.resolve('fixture/native'),launcher=path.join(install,'airk-launcher');
    const proof={install,launcher,canonical:launcher,regular:true,nlink:1,actualHash:OLD_MAIN_SHA,authenticatedMainHash:OLD_MAIN_SHA,inventoryMainHash:OLD_MAIN_SHA,strict:{command:'/usr/bin/codesign',args:['--verify','--strict','--verbose=4',launcher],status:1,stdout:'',stderr:'airk-launcher: invalid Info.plist (plist or signature have been modified)'}};
    assert.equal(qualifyOldLauncherCopy(proof).qualified,true);
    for(const mutate of [v=>v.actualHash='foreign',v=>v.authenticatedMainHash='foreign',v=>v.inventoryMainHash='foreign',v=>v.canonical=path.join(install,'elsewhere'),v=>v.regular=false,v=>v.nlink=2,v=>v.strict.status=0,v=>v.strict.stderr='timeout',v=>v.strict.stderr='code object is not signed at all',v=>v.strict.args=['--verify',launcher]]){
        const changed=structuredClone(proof);mutate(changed);assert.throws(()=>qualifyOldLauncherCopy(changed));
    }
});
test('safe read retries share one elapsed deadline and never renew a late request budget',async()=>{
    let clock=100;const timeouts=[];
    const result=await boundedSafeRead({deadline:100000,now:()=>clock,sleep:async ms=>{clock+=ms;},retryDelay:wire=>wire.retry,request:async timeout=>{
        timeouts.push(timeout);if(timeouts.length===1){clock+=29000;return {retry:500};}clock+=100;return {complete:true};
    }});
    assert.deepEqual(result,{complete:true});assert.deepEqual(timeouts,[30000,500]);
    let calls=0;
    await assert.rejects(boundedSafeRead({deadline:clock,now:()=>clock,retryDelay:()=>undefined,request:async()=>{calls++;}}),/before dispatch/);assert.equal(calls,0);
    calls=0;
    await assert.rejects(boundedSafeRead({deadline:100000,now:()=>clock,sleep:async()=>{},retryDelay:()=>1,request:async()=>{calls++;clock+=30000;return {};}}),/retry exceeded/);assert.equal(calls,1);
});
test('importing qualification is inert; public historical checksum is pinned',()=>{
    assert.equal(OLD_SHA,'71518c9d89fd0fb4f54e0d046b2f85948a361c7303ad7c49a60df568dfb4219d');
    assert.equal(inside('/fixture','/fixture-other/source'),false);
    assert.equal(inside('/fixture','/fixture/source'),true);
});
test('recovery accepts only fresh typed old-runtime activation failure, never text or a timeout',()=>{
    const receipt={schema:1,install:'/fixture/native',candidate:'old',preceding:null,outcome:'aborted_or_failed',observed_unix_ms:11};
    assert.equal(failedActivation(receipt,'/fixture/native','old',10),receipt);
    for(const value of [undefined,{error:'installation failed'}, {...receipt,install:'/ambient'}, {...receipt,candidate:'other'}, {...receipt,outcome:'selected'}, {...receipt,observed_unix_ms:9}])assert.throws(()=>failedActivation(value,'/fixture/native','old',10));
});
test('between-phase public setting gate preserves exact install and unrelated settings',async()=>{
    const fs=require('node:fs/promises'),path=require('node:path'),os=require('node:os');
    const folder=await fs.mkdtemp(path.join(os.tmpdir(),'airk-vsix-gate-')),filename=path.join(folder,'User/settings.json'),install=path.join(folder,'install');
    try{
        await fs.mkdir(path.dirname(filename));
        const settings={'airk.service.installDir':install,'airk.runtime.nativeExecution':false,'airk.cli.autoInstall':true,'arbitrary.user.choice':'retain'};
        await fs.writeFile(filename,JSON.stringify(settings));
        // Actual setting persistence models first public enable, guarded second
        // launch, and second public enable without rewriting first-use state.
        const first=JSON.parse(await fs.readFile(filename,'utf8'));first['airk.runtime.nativeExecution']=true;await fs.writeFile(filename,JSON.stringify(first));
        await gatePrivateProfile(folder,install);assert.deepEqual(JSON.parse(await fs.readFile(filename,'utf8')),settings);
        const second=JSON.parse(await fs.readFile(filename,'utf8'));second['airk.runtime.nativeExecution']=true;await fs.writeFile(filename,JSON.stringify(second));
        assert.equal(JSON.parse(await fs.readFile(filename,'utf8'))['airk.runtime.nativeExecution'],true);
        await assert.rejects(gatePrivateProfile(folder,path.join(folder,'foreign')),/installation setting changed/);
    }finally{assert.equal(path.dirname(path.resolve(folder)),path.resolve(os.tmpdir()));assert.match(path.basename(folder),/^airk-vsix-gate-/);await fs.rm(folder,{recursive:true});}
});
test('transport teardown waits for real owned OS exit, including bounded forced termination',async()=>{
    const {spawn}=require('node:child_process');
    const child=spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore'}),closed=new Promise(resolve=>child.once('close',(code,signal)=>resolve({code,signal})));
    try{const receipt=await waitOwnedExit(child,closed,10);assert.equal(receipt.closed,true);assert.equal(receipt.pid,child.pid);assert.ok(child.exitCode!==null||child.signalCode!==null);}finally{if(child.exitCode===null&&child.signalCode===null)child.kill();}
});
test('native JSON subprocess byte/deadline failures retain observed OS exit receipts',async()=>{
    for(const [program,options]of [["process.stdin.resume();console.log(JSON.stringify({ok:true}))",{}],["process.stdin.resume();process.stdout.write('x'.repeat(512));setInterval(()=>{},1000)",{maxBytes:64}],["process.stdin.resume();setInterval(()=>{},1000)",{timeout:20}]]){
        const receipts=[],invoke=()=>boundedJsonProcess(process.execPath,['-e',program],{command:'health'},{cwd:process.cwd(),env:process.env,timeout:2000,...options,onExit:async value=>receipts.push(value)});
        if(options.maxBytes||options.timeout)await assert.rejects(invoke(),/byte bound|deadline/);else assert.deepEqual(await invoke(),{ok:true});
        assert.equal(receipts.length,1);assert.equal(receipts[0].closed,true);assert.ok(Number.isSafeInteger(receipts[0].pid));
    }
});
test('MCP full contract rejects errors and divergent text/structured data',()=>{
    const body={items:[{citation:{path:'fixture.ts',content_hash:'abc'}}]};
    assert.deepEqual(toolBody({content:[{type:'text',text:JSON.stringify(body)}],structuredContent:body}),body);
    assert.throws(()=>toolBody({isError:true,content:[{type:'text',text:'native refusal'}]}),/MCP returned an error/);
    assert.throws(()=>toolBody({content:[{type:'text',text:'{"retained":1}'}],structuredContent:{retained:2}}),/differs/);
});
test('original citation must retain both path and exact source hash',()=>{
    assertEvidence({members:[{result:{hits:[{citation:{path:'fixture.ts',content_hash:'abc'}}]}}]},'abc');
    assert.throws(()=>assertEvidence({items:[{citation:{path:'fixture.ts',content_hash:'different'}}]},'abc'),/exact original/);
    assert.throws(()=>assertEvidence({items:[{citation:{path:'foreign.ts',content_hash:'abc'}}]},'abc'),/exact original/);
});
function stopFixture(){
    const bundle={digest:'runtime'},install='/fixture/native',inspection={payload_verified:true,approval:{manifest_sha256:'runtime'},registry_namespace:'namespace',control_dir:'/fixture/native/control',lifecycle:{explicitly_stopped:true,pending:null}};
    const stop={started:10,dispatch:{dispatch:1},confirmation:{dispatch:2},requested_binding:{namespace:'namespace',installDir:install,runtimeSha256:'runtime'},capture:{qualification:'historical_native_lifecycle_response',capturedAt:11,force:false,before_health:{status:'health',observation:{instance:'instance'}},runtime:{installDir:install,runtimeSha256:'runtime',controlDir:inspection.control_dir},namespace:'namespace',response:{status:'lifecycle',record:{id:'new',phase:'complete',request:{action:'stop',expected_instance:'instance',runtime:'runtime',key:{namespace:'namespace'}}}}}};
    return {bundle,install,inspection,stop};
}
test('only complete public Stop with current authenticated runtime/namespace qualifies cleanup',()=>{
    const value=stopFixture();assert.equal(completeStop(value.stop,value.inspection,value.bundle,value.install).qualified,true);
    for(const mutate of [v=>v.stop.capture.force=true,v=>v.stop.capture.response.record.request.expected_instance='foreign',v=>v.stop.capture.runtime.runtimeSha256='old',v=>v.inspection.registry_namespace='foreign',v=>v.inspection.lifecycle.pending={id:'pending'},v=>v.stop.confirmation.dispatch=1,v=>v.stop.previous_capture={response:{record:{id:'new'}}}]){
        const changed=structuredClone(value);mutate(changed);assert.throws(()=>completeStop(changed.stop,changed.inspection,changed.bundle,changed.install));
    }
});
