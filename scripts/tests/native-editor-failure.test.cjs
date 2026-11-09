// SPDX-License-Identifier: Apache-2.0
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {withFailureStop,stopOwnedFixture}=require('../native-editor-failure.cjs');
const fsp=require('node:fs/promises'),os=require('node:os'),{createHash}=require('node:crypto');

function actualDriverFailureHandler(ports){
    // Execute only the real installed-runner catch boundary with injected side
    // effects. Importing the runner itself would launch Code, so never do that.
    const source=fs.readFileSync(path.join(__dirname,'../check-native-editor.cjs'),'utf8');
    const marker='main().catch(',offset=source.lastIndexOf(marker);
    assert.ok(offset>=0,'Actual driver must expose its terminal failure boundary');
    const tail=source.slice(offset+marker.length).trim();
    assert.ok(tail.endsWith(');'),'Actual terminal handler shape changed; review this extraction');
    return vm.runInNewContext('('+tail.slice(0,-2)+')',ports,{timeout:1000});
}

for(const boundary of ['before client creation during fresh setup','after an installed client body throws'])
test(`actual installed driver stops its owned native fixture ${boundary}`,async()=>{
    const original=new Error(boundary),events=[],reports=[];
    const process_={exitCode:0};
    const receipt={status:'lifecycle',record:{phase:'complete',request:{action:'stop',
        runtime:'a'.repeat(64),expected_instance:'b'.repeat(32),
        key:{namespace:'c'.repeat(32),epoch:1,nonce:'d'.repeat(32)},manager:false}}};
    const handler=actualDriverFailureHandler({
        closeOwnedEditors:async()=>{events.push('close-editors');return [{closed:true}];},
        stopOwnedServicesOnFailure:async()=>{events.push('native-stop');return [{receipt,acceptance:false}];},
        cleanupServices:async()=>{events.push('ordinary-cleanup');throw Error('original native-stop.json is absent');},
        cleanupProfiles:async()=>{events.push('profiles');return [];},
        console:{error:text=>reports.push(JSON.parse(text))},process:process_,
    });
    await handler(original);
    assert.equal(events.filter(event=>event==='native-stop').length,1,
        'Closing an editor or failing ordinary receipt cleanup never stops the owned native service');
    assert.ok(events.indexOf('close-editors')<events.indexOf('native-stop'));
    assert.equal(process_.exitCode,1,'Failure teardown must never qualify the editor journey');
    assert.equal(reports[0].error,String(original));
    assert.match(reports[0].serviceCleanup,/original native-stop.json is absent/,
        'A separate failure Stop must not manufacture the normal UI Stop receipt');
});

test('successful client leaves the ordinary final Stop owner in control',async()=>{
    assert.equal(await withFailureStop({run:async()=>42,stop:async()=>assert.fail('unexpected Stop')}),42);
});

test('failed client publishes exactly the real one-dispatch Stop receipt and rethrows',async()=>{
    const original=new Error('client TLS failed'),receipt={capture:{response:{record:{id:'operation'}}}};
    const events=[];
    await assert.rejects(withFailureStop({run:async()=>{throw original;},
        stop:async()=>{events.push('stop');return receipt;},
        publishStop:async value=>{assert.equal(value,receipt);events.push('publish');},
        retainFailure:async value=>{assert.equal(value.stop_receipt_published,true);events.push('retain');}}),error=>error===original);
    assert.deepEqual(events,['stop','publish','retain']);
});

for(const phase of ['stop','publish'])test(`failed ${phase} is retained without replay or a manufactured receipt`,async()=>{
    const original=new Error('client failed'),cleanup=new Error(`${phase} uncertain`);
    let stops=0,publishes=0,retained;
    await assert.rejects(withFailureStop({run:async()=>{throw original;},
        stop:async()=>{stops++;if(phase==='stop')throw cleanup;return {actual:true};},
        publishStop:async()=>{publishes++;throw cleanup;},
        retainFailure:async value=>{retained=value;}}),error=>error===original);
    assert.equal(stops,1);assert.equal(publishes,phase==='stop'?0:1);
    assert.equal(retained.stop_receipt_published,false);assert.equal(retained.cleanup_error,String(cleanup));
});

test('retention failure keeps both the original error and evidence failure',async()=>{
    const original=new Error('client failed'),retention=new Error('disk unavailable');
    await assert.rejects(withFailureStop({run:async()=>{throw original;},stop:async()=>({}),
        publishStop:async()=>{},retainFailure:async()=>{throw retention;}}),
        error=>error instanceof AggregateError&&error.errors[0]===original&&error.errors[1]===retention);
});

async function fixture(t){
    const root=await fsp.realpath(await fsp.mkdtemp(path.join(os.tmpdir(),'airk-failure-stop-')));
    t.after(()=>fsp.rm(root,{recursive:true,force:true}));
    const digest='a'.repeat(64),namespace='b'.repeat(32),instance='c'.repeat(32),bytes=Buffer.from('inert owned executable');
    const execution=createHash('sha256').update(bytes).digest('hex'),install=path.join(root,'native-service'),control=path.join(install,'control');
    const source=path.join(root,'bundle'),main=path.join(source,'airk.exe'),executable=path.join(install,'runtimes',digest,'airk.exe');
    await fsp.mkdir(control,{recursive:true});await fsp.mkdir(path.dirname(executable),{recursive:true});await fsp.mkdir(source);
    await fsp.writeFile(main,bytes);await fsp.writeFile(executable,bytes);
    await fsp.writeFile(path.join(source,'airk-runtime.json'),JSON.stringify({airk_sha256:execution}));
    const state={stops:0,stopped:false,calls:[],namespace,instance,digest,execution,executable,request:null};
    const inspection=()=>({payload_verified:true,approval:{manifest_sha256:digest},selection:{active:digest},executable,
        control_dir:control,registry_namespace:namespace,lifecycle:{explicitly_stopped:state.stopped,pending:null}});
    const health={status:'health',observation:{version:'1.8.16',protocol:1,runtime_sha256:digest,executable_sha256:execution,
        instance,process_id:123,catalog_clock:{namespace,revision:7},policy:{accepting:true}}};
    const input={fixture:root,install,control,bundle:{main,source,digest,version:'1.8.16'},editorsClosed:true,
        invoke:async(command,args,timeout,text,record)=>{
            assert.equal(command,main);state.calls.push(args);
            if(args[0]==='runtime'&&args[1]==='inspect')return inspection();
            const body=JSON.parse(text);
            if(args[0]==='service')return body==='health'?structuredClone(health):{status:'request_epoch',namespace,epoch:3};
            assert.deepEqual(args,['runtime','lifecycle','--install-dir',install]);assert.equal(timeout,30000);
            const intent=JSON.parse(await fsp.readFile(path.join(root,'failure-native-stop-intent.json'),'utf8'));
            assert.deepEqual(intent.request,body,'Durable exclusive intent must precede native Stop');
            state.stops++;state.request=body;
            if(state.uncertain){
                Object.assign(record,{stdout:'{"partial_native_receipt":true}',stderr:'actual native diagnostic',
                    truncated:state.uncertain==='oversize',outcome_unknown:true});
                throw Object.assign(Error('native dispatch outcome unknown'),{code:state.uncertain==='oversize'?'ERR_CHILD_PROCESS_STDIO_MAXBUFFER':7});
            }
            state.stopped=true;const {force,...request}=body;assert.equal(force,false);
            return {status:'lifecycle',record:{id:'d'.repeat(64),phase:'complete',request:{...request,manager:false},extra_native_field:{retained:true}}};
        }};
    return {root,input,state,inspection,health};
}

test('actual failure owner persists intent then one canonical Stop and the complete separate receipt',async t=>{
    const {root,input,state}=await fixture(t),result=await stopOwnedFixture(input);
    assert.equal(state.stops,1);assert.equal(result.complete,true);assert.equal(result.acceptance,false);
    assert.deepEqual(result.response.record.extra_native_field,{retained:true});
    assert.deepEqual(JSON.parse(await fsp.readFile(path.join(root,'failure-native-stop.json'),'utf8')),result);
    await assert.rejects(fsp.access(path.join(root,'native-stop.json')),{code:'ENOENT'});
});

for(const field of ['fixture','install','control','runtime','executable','namespace','epoch','instance','editors'])
test(`failure owner refuses crossed ${field} before any Stop`,async t=>{
    const {root,input,state}=await fixture(t),original=input.invoke;
    if(field==='fixture')input.fixture=path.join(root,'bundle');
    if(field==='install')input.install=root;
    if(field==='control')input.control=path.join(root,'bundle');
    if(field==='runtime')input.bundle.digest='e'.repeat(64);
    if(field==='executable')await fsp.writeFile(state.executable,'changed');
    if(field==='editors')input.editorsClosed=false;
    input.invoke=async(...args)=>{
        const value=await original(...args);
        if(field==='namespace'&&value.status==='health')value.observation.catalog_clock.namespace='e'.repeat(32);
        if(field==='epoch'&&value.status==='request_epoch')value.namespace='e'.repeat(32);
        if(field==='instance'&&value.status==='health')value.observation.instance='invalid';
        return value;
    };
    await assert.rejects(stopOwnedFixture(input));assert.equal(state.stops,0);
    await assert.rejects(fsp.access(path.join(root,'failure-native-stop.json')),{code:'ENOENT'});
});

for(const mode of ['nonzero JSON','oversize'])test(`uncertain Stop keeps original intent/error and never dispatches again (${mode})`,async t=>{
    const {root,input,state}=await fixture(t);state.uncertain=mode;
    await assert.rejects(stopOwnedFixture(input),/outcome unknown/);assert.equal(state.stops,1);
    const intent=await fsp.readFile(path.join(root,'failure-native-stop-intent.json'),'utf8');
    const error=JSON.parse(await fsp.readFile(path.join(root,'failure-native-stop-error.json'),'utf8'));
    assert.equal(error.dispatched,true);assert.equal(error.complete,false);assert.match(error.error,/outcome unknown/);
    const call=error.calls.find(call=>call.args[0]==='runtime'&&call.args[1]==='lifecycle');
    assert.equal(call.stdout,'{"partial_native_receipt":true}');assert.equal(call.stderr,'actual native diagnostic');
    assert.equal(call.truncated,mode==='oversize');assert.equal(call.outcome_unknown,true);
    await assert.rejects(stopOwnedFixture(input));assert.equal(state.stops,1);
    assert.equal(await fsp.readFile(path.join(root,'failure-native-stop-intent.json'),'utf8'),intent);
    await assert.rejects(fsp.access(path.join(root,'failure-native-stop.json')),{code:'ENOENT'});
});

test('crossed returned native request is retained verbatim without publishing completion',async t=>{
    const {root,input,state}=await fixture(t),original=input.invoke;
    input.invoke=async(...args)=>{const value=await original(...args);if(value.status==='lifecycle')value.record.request.key.nonce='e'.repeat(32);return value;};
    await assert.rejects(stopOwnedFixture(input),/crossed/);assert.equal(state.stops,1);
    const evidence=JSON.parse(await fsp.readFile(path.join(root,'failure-native-stop-error.json'),'utf8'));
    assert.equal(evidence.response.record.request.key.nonce,'e'.repeat(32));assert.equal(evidence.complete,false);
    await assert.rejects(fsp.access(path.join(root,'failure-native-stop.json')),{code:'ENOENT'});
});

test('an existing exclusive intent refuses mutation instead of overwriting prior uncertain evidence',async t=>{
    const {root,input,state}=await fixture(t);
    await fsp.writeFile(path.join(root,'failure-native-stop-intent.json'),'prior original');
    await assert.rejects(stopOwnedFixture(input),{code:'EEXIST'});assert.equal(state.stops,0);
    assert.equal(await fsp.readFile(path.join(root,'failure-native-stop-intent.json'),'utf8'),'prior original');
});

test('actual receipt publication failure retains the complete native response without replay',async t=>{
    const {root,input,state}=await fixture(t);
    await fsp.mkdir(path.join(root,'failure-native-stop.json'));
    await assert.rejects(stopOwnedFixture(input));assert.equal(state.stops,1);
    const error=JSON.parse(await fsp.readFile(path.join(root,'failure-native-stop-error.json'),'utf8'));
    assert.equal(error.response.record.phase,'complete');assert.equal(error.after.lifecycle.explicitly_stopped,true);
    assert.equal(error.complete,false);assert.equal(error.acceptance,false);
});

test('actual evidence retention failure reports both failures and preserves the original intent',async t=>{
    const {root,input,state}=await fixture(t);state.uncertain=true;
    await fsp.writeFile(path.join(root,'failure-native-stop-error.json'),'prior failure evidence');
    await assert.rejects(stopOwnedFixture(input),error=>error instanceof AggregateError
        &&String(error.errors[0]).includes('outcome unknown')&&error.errors[1].code==='EEXIST');
    assert.equal(state.stops,1);
    assert.equal(await fsp.readFile(path.join(root,'failure-native-stop-error.json'),'utf8'),'prior failure evidence');
    assert.equal(JSON.parse(await fsp.readFile(path.join(root,'failure-native-stop-intent.json'),'utf8')).request.action,'stop');
});

function normalProof(f){
    const request={caller:'e'.repeat(32),key:{namespace:f.state.namespace,epoch:3,nonce:'f'.repeat(32)},action:'stop',
        expected_instance:f.state.instance,runtime:f.state.digest,manager:false};
    return {started:10,dispatch:{dispatch:'first'},confirmation:{dispatch:'second'},requested_binding:{namespace:f.state.namespace},
        capture:{qualification:'historical_native_lifecycle_response',force:false,capturedAt:11,namespace:f.state.namespace,
            runtime:{installDir:f.input.install,controlDir:f.input.control,runtimeSha256:f.state.digest},before_health:f.health,
            response:{status:'lifecycle',record:{id:'d'.repeat(64),phase:'complete',request}}}};
}
test('complete actual UI Stop proof is reused for disposal with zero mutation',async t=>{
    const f=await fixture(t);f.state.stopped=true;f.input.normalStop=normalProof(f);
    const result=await stopOwnedFixture(f.input);assert.equal(f.state.stops,0);assert.equal(result.complete,true);
    assert.deepEqual(result.reused_normal_stop,f.input.normalStop);
    await assert.rejects(fsp.access(path.join(f.root,'failure-native-stop-intent.json')),{code:'ENOENT'});
});
test('crossed existing UI proof refuses a replacement Stop',async t=>{
    const f=await fixture(t);f.state.stopped=true;f.input.normalStop=normalProof(f);
    f.input.normalStop.capture.runtime.installDir=f.root;
    await assert.rejects(stopOwnedFixture(f.input),/incomplete or crossed/);assert.equal(f.state.stops,0);
});
test('actual driver both consumes and freezes its failure owner and retains normal cleanup semantics',()=>{
    const source=fs.readFileSync(path.join(__dirname,'../check-native-editor.cjs'),'utf8');
    assert.match(source,/require\('\.\/native-editor-failure\.cjs'\)/);
    assert.match(source,/const harnessPaths=\[[^\n]*'scripts\/native-editor-failure\.cjs'/);
    assert.match(source,/await stopOwnedFixture\(\{fixture,install,control,bundle,invoke,normalStop,editorsClosed:true\}\)/);
    assert.match(source,/failureNativeStop=await stopOwnedServicesOnFailure\(editorCleanup\)/);
    assert.match(source,/path\.join\(folder,'native-stop\.json'\)/);
});

test('actual driver preserves the complete existing consumed-input roster when adding failure disposal',()=>{
    const source=fs.readFileSync(path.join(__dirname,'../check-native-editor.cjs'),'utf8');
    const declaration=source.match(/const harnessPaths=(\[[^\n]*\]);/);
    assert.ok(declaration,'Actual driver consumed-input declaration must remain inspectable');
    const actual=vm.runInNewContext(declaration[1],{}, {timeout:1000});
    const expected=[
        'scripts/check-native-editor.cjs','scripts/editor-ui.cjs','scripts/native-editor-workbench.cjs',
        'scripts/editor-mailbox.cjs','scripts/native-editor-receipts.cjs',
        'scripts/native-editor-idle.cjs','packages/vscode/out/test/installedEditor.js',
        'packages/vscode/out/test/installedCompletion.js','packages/vscode/out/test/installedControlTrace.js',
        'packages/vscode/out/test/installedEditorC8.js','packages/vscode/out/test/installedEditorContracts.js',
        'packages/vscode/out/test/installedEditorHttp.js','scripts/native-editor-failure.cjs',
    ];
    assert.equal(actual.length,13,'All 12 existing consumed inputs plus failure disposal must be frozen');
    assert.equal(new Set(actual).size,13,'Consumed-input roster must contain no duplicates');
    assert.deepEqual([...actual].sort(),expected.sort(),'A new helper must not displace an existing consumed input');
});

for(const code of [7,'ERR_CHILD_PROCESS_STDIO_MAXBUFFER'])
test(`bounded native adapter preserves raw failed output without accepting JSON or retrying (${code})`,async()=>{
    const original=Object.assign(Error('exit uncertain'),{code,signal:null,killed:false});
    const observations=[],exports_={};let dispatches=0,input;
    const text=fs.readFileSync(path.join(__dirname,'../native-editor-failure.cjs'),'utf8');
    vm.runInNewContext(text,{exports:exports_,Buffer,require:name=>name==='node:child_process'?{
        execFile:(command,args,options,callback)=>{
            dispatches++;assert.equal(options.timeout,30000);assert.equal(options.maxBuffer,65536);
            assert.equal(options.env.private_fixture,'yes');
            callback(original,'{"native_partial":"retain all"}','complete native failure diagnostic');
            return {stdin:{on:()=>{},end:value=>{input=value;}}};
        }}:require(name)});
    const record={};observations.push(record);
    await assert.rejects(exports_.invokeNativeJson(path.resolve('inert.exe'),['runtime','lifecycle'],{
        timeout:30000,input:'{"force":false}',environment:{private_fixture:'yes'},record}),error=>error===original);
    assert.equal(dispatches,1);assert.equal(input,'{"force":false}');
    assert.equal(record.stdout,'{"native_partial":"retain all"}');assert.equal(record.stderr,'complete native failure diagnostic');
    assert.equal(record.exit_code,code);assert.equal(record.truncated,code==='ERR_CHILD_PROCESS_STDIO_MAXBUFFER');
    assert.equal(record.outcome_unknown,true);
});

for(const editorsClosed of [true,false])test(`actual failure driver passes closure and exact fixture to real disposal boundary (${editorsClosed})`,async()=>{
    const source=fs.readFileSync(path.join(__dirname,'../check-native-editor.cjs'),'utf8');
    const begin=source.indexOf('async function stopOwnedServicesOnFailure('),end=source.indexOf('async function cleanupServices()',begin);
    assert.ok(begin>=0&&end>begin);
    const folder=path.resolve('inert-owned-fixture'),retained=new Set();let calls=0;
    const handler=vm.runInNewContext('('+source.slice(begin,end).trim()+')',{
        services:[folder],retainedProfileFolders:retained,path,
        fs:{lstat:async()=>({}),readFile:async()=>{throw Object.assign(Error('missing normal receipt'),{code:'ENOENT'});}},
        bundle:{digest:'a'.repeat(64)},environment:{private:true},invokeNativeJson:()=>assert.fail('Do not execute native in driver contract'),
        stopOwnedFixture:async value=>{calls++;assert.equal(value.fixture,folder);assert.equal(value.install,path.join(folder,'native-service'));
            assert.equal(value.control,path.join(folder,'native-service','control'));assert.equal(value.normalStop,null);
            assert.equal(value.editorsClosed,true);return {complete:true,acceptance:false};},
    });
    const result=await handler([{closed:editorsClosed}]);assert.equal(calls,editorsClosed?1:0);
    assert.equal(retained.has(folder),true);assert.equal(result[0].acceptance,false);
    if(!editorsClosed)assert.match(result[0].error,/closure is unconfirmed/);
});
