// SPDX-License-Identifier: Apache-2.0
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const root=path.resolve(__dirname,'../../../..');
const {withFailureStop}=require(path.join(root,'scripts/native-editor-failure.cjs'));
test('actual Code driver injection binds its failure Stop and receipt to the fixture',async()=>{
    const audit=path.join(root,'artifacts/local/one-airk-1.8.14/client-protocol-audit-r1');
    let transformed;
    class CaptureModule{
        constructor(){this.exports={};}
        static _nodeModulePaths(){return [];}
        _compile(source){transformed=source;}
    }
    vm.runInNewContext(fs.readFileSync(path.join(audit,'code-driver-wrapper.cjs'),'utf8'),{
        require:name=>name==='node:module'?CaptureModule:require(name),__dirname:audit,module:{exports:{}}});
    const begin=transformed.indexOf('await require('+JSON.stringify(path.join(root,'scripts/native-editor-failure.cjs'))+')');
    assert.ok(begin>=0);
    const injection=transformed.slice(begin,transformed.indexOf('let client = new Mcp(definition, definition.ca);',begin));
    const AsyncFunction=Object.getPrototypeOf(async function(){}).constructor;
    const invoke=new AsyncFunction('require','vscode','extension','definition','transportSelection','mark',
        'sharedDeadline','scoped','ui','folder','process','publishMailbox','path',injection);
    const original=new Error('actual host TLS failed'),events=[],receipt={capture:{response:{record:{id:'real-stop'}}}};
    const folder=path.join(root,'fixture');
    await assert.rejects(invoke(name=>name.endsWith('native-editor-failure.cjs')?{withFailureStop}:{run:async()=>{throw original;}},
        {},{},{},{},async(name,value)=>events.push([name,value]),1000,()=>{},
        async(action,binding,route)=>{events.push([action,binding,route]);return receipt;},folder,
        {env:{AIRK_EDITOR_RUNTIME_SHA256:'candidate'}},async(file,value)=>events.push([file,value]),path),error=>error===original);
    assert.deepEqual(events[0],['serviceStop',{installDir:path.join(folder,'native-service'),runtimeSha256:'candidate'},'#manage/service']);
    assert.deepEqual(events[1],[path.join(folder,'native-stop.json'),receipt]);
    assert.equal(events[2][0],'actual-code-failure-cleanup');
});


test('actual runner always removes exact fixture trust after either host outcome',async()=>{
    const audit=__dirname;
    let transformed;
    class CaptureModule{
        constructor(){this.exports={};}
        static _nodeModulePaths(){return [];}
        _compile(source){transformed=source;}
    }
    const fakeFs={...fs,readFileSync:(file,...args)=>String(file).endsWith('code-source-identity.json')?JSON.stringify({sources:{}}):fs.readFileSync(file,...args)};
    vm.runInNewContext(fs.readFileSync(path.join(audit,'run-code-mcp-host.cjs'),'utf8'),{
        require:name=>name==='node:module'?CaptureModule:name==='node:fs'?fakeFs:require(name),
        __dirname:audit,module:{exports:{}},process:{platform:'win32',argv:['node','runner','Code.exe','/reviewed/cli.js'],env:{}}});
    assert.ok(transformed.includes('codeTrustOwners.push(folder);'));
    assert.ok(transformed.indexOf('codeTrustOwners.push(folder);')<transformed.indexOf('const caPreparation='));
    assert.ok(transformed.includes('prepared.os_user_trust_installed!==true'));
    assert.equal(transformed.match(/codeTrustOwners.push\(folder\)/g).length,1);
    new Function('require','module','process',transformed); // Parse complete real harness transformation.
    const start=transformed.lastIndexOf('.finally(async()=>{')+'.finally(async()=>{'.length;
    const end=transformed.lastIndexOf('});');
    const callback=transformed.slice(start,end);
    const AsyncFunction=Object.getPrototypeOf(async function(){}).constructor;
    const invoke=new AsyncFunction('codeTrustOwners','execute','environment','root','process','console',callback);
    const owners=['exact-fixture'],calls=[],errors=[],state={exitCode:1};
    await invoke(owners,async(...args)=>calls.push(args),{},root,state,{error:value=>errors.push(value)});
    assert.equal(calls.length,1);assert.deepEqual(calls[0][1],
        [path.join(audit,'code-user-trust.py'),'--remove-owner','exact-fixture']);
    assert.equal(state.exitCode,1,'Original host failure remains a failure after successful removal');
    const success={};
    await invoke(owners,async()=>{throw Error('remove uncertain');},{},root,success,{error:value=>errors.push(value)});
    assert.equal(success.exitCode,1);assert.equal(errors.length,1);
});
